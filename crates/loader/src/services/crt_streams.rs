// SPDX-License-Identifier: GPL-3.0-or-later

//! The standard streams of a hosted C program, and where their output goes.
//!
//! `__acrt_iob_func` hands out the `FILE` objects for `stdin`, `stdout` and
//! `stderr`. What the guest writes to `stdout` and to `stderr` is kept in two
//! host-side buffers that a host or a test reads through `ServicesState`.
//!
//! The standard streams "always open in text mode by default", and in text
//! mode "each line feed is replaced with a carriage return-line feed pair"
//! that "has no effect on the return value" (Microsoft Learn, "Text and
//! binary mode file I/O" and "fwrite"). The buffers hold the bytes after that
//! replacement, which is what the operating system would be handed.
//!
//! Output is never held back: there is no buffering to flush or to configure.
//! Any other stream, and input, are not modelled, and using one stops the
//! guest with the runtime-limit fault.

use super::crt_format::{self, FormatError, Source};
use crate::cpu::ExitReason;
use crate::memory::{GuestMemory, MemoryError, PROCESS_HEAP};

/// The exit for a request the runtime does not model.
const RUNTIME_LIMIT: ExitReason = ExitReason::IllegalInstruction(0xFE);
/// `typedef struct _iobuf { void* _Placeholder; } FILE;` (corecrt_wstdio.h).
const FILE_BYTES: u64 = 8;
/// The most a format string or one `fwrite` or `fputs` may hold.
const MAX_TRANSFER: u64 = 1 << 20;
/// The most each captured stream keeps. Writing past it stops the guest.
pub const MAX_CAPTURED_OUTPUT: usize = 16 << 20;

/// Option bits of `__stdio_common_vfprintf` (corecrt_stdio_config.h) that do
/// not change anything this engine formats: the wide-specifier bit concerns
/// the wide functions, and the exponent and rounding bits concern floating
/// point. Any other bit stops the guest.
const IGNORED_OPTIONS: u64 = 0x04 | 0x10 | 0x20;

/// `_IOFBF`, `_IOLBF` and `_IONBF` (stdio.h).
const IOFBF: i32 = 0x0000;
const IOLBF: i32 = 0x0040;
const IONBF: i32 = 0x0004;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stream {
    Input,
    Output,
    Error,
}

#[derive(Default)]
pub(super) struct StandardStreams {
    /// Guest address of the three `FILE` objects, `stdin` first.
    files: Option<u64>,
    output: Vec<u8>,
    error: Vec<u8>,
}

/// What a service returns when its documentation gives a failure result and
/// an `errno` value: the handler sets `errno` to `EINVAL` and returns this.
pub(super) struct Invalid<T>(pub T);

pub(super) type Outcome<T> = Result<Result<T, Invalid<T>>, ExitReason>;

fn fault(error: MemoryError) -> ExitReason {
    ExitReason::MemoryFault(error)
}

/// The bytes of the C string at `address`, without its terminator.
fn c_string(memory: &GuestMemory, address: u64, limit: u64) -> Result<Vec<u8>, MemoryError> {
    let mut bytes = Vec::new();
    for offset in 0..limit {
        let at = address.checked_add(offset).ok_or(MemoryError::Overflow)?;
        match memory.read_u8(at)? {
            0 => break,
            byte => bytes.push(byte),
        }
    }
    Ok(bytes)
}

/// Arguments from the guest's `va_list`: one 8-byte slot each, in order.
struct VaList<'a> {
    memory: &'a GuestMemory,
    next: u64,
}

impl Source for VaList<'_> {
    fn argument(&mut self) -> Result<u64, MemoryError> {
        let slot = self.memory.read_u64(self.next)?;
        self.next = self.next.checked_add(8).ok_or(MemoryError::Overflow)?;
        Ok(slot)
    }

    fn string(&mut self, address: u64, limit: usize) -> Result<Vec<u8>, MemoryError> {
        c_string(self.memory, address, limit as u64)
    }
}

impl StandardStreams {
    pub(super) fn output(&self) -> &[u8] {
        &self.output
    }

    pub(super) fn error(&self) -> &[u8] {
        &self.error
    }

    /// `__acrt_iob_func`: the `FILE` object of `stdin` (0), `stdout` (1) or
    /// `stderr` (2). The three are allocated together from the process heap
    /// on first use.
    pub(super) fn file(&mut self, memory: &mut GuestMemory, index: u32) -> Result<u64, ExitReason> {
        if index > 2 {
            return Err(RUNTIME_LIMIT);
        }
        let base = match self.files {
            Some(base) => base,
            None => {
                let base = memory
                    .map_heap_allocation(PROCESS_HEAP, (3 * FILE_BYTES) as usize)
                    .map_err(fault)?;
                self.files = Some(base);
                base
            }
        };
        Ok(base + FILE_BYTES * u64::from(index))
    }

    /// Which standard stream a non-null `FILE*` is. Any other pointer is a
    /// stream this runtime does not have.
    fn stream(&self, file: u64) -> Result<Stream, ExitReason> {
        let base = self.files.ok_or(RUNTIME_LIMIT)?;
        match file.wrapping_sub(base) {
            0 => Ok(Stream::Input),
            8 => Ok(Stream::Output),
            16 => Ok(Stream::Error),
            _ => Err(RUNTIME_LIMIT),
        }
    }

    /// A stream that can be written: `stdout` or `stderr`.
    fn writable(&self, file: u64) -> Result<Stream, ExitReason> {
        match self.stream(file)? {
            Stream::Input => Err(RUNTIME_LIMIT),
            stream => Ok(stream),
        }
    }

    /// Append to a captured stream, in text mode: each line feed becomes a
    /// carriage return and a line feed.
    fn write(&mut self, stream: Stream, bytes: &[u8]) -> Result<(), ExitReason> {
        let captured = match stream {
            Stream::Output => &mut self.output,
            Stream::Error => &mut self.error,
            Stream::Input => return Err(RUNTIME_LIMIT),
        };
        let feeds = bytes.iter().filter(|byte| **byte == b'\n').count();
        if bytes.len() + feeds > MAX_CAPTURED_OUTPUT - captured.len() {
            return Err(RUNTIME_LIMIT);
        }
        for &byte in bytes {
            if byte == b'\n' {
                captured.push(b'\r');
            }
            captured.push(byte);
        }
        Ok(())
    }

    /// `__stdio_common_vfprintf`. Returns the number of characters written,
    /// before the text-mode replacement.
    pub(super) fn print(
        &mut self,
        memory: &GuestMemory,
        options: u64,
        file: u64,
        format: u64,
        locale: u64,
        arguments: u64,
    ) -> Outcome<i32> {
        // A null stream or format: the function returns -1 and sets errno.
        if file == 0 || format == 0 {
            return Ok(Err(Invalid(-1)));
        }
        if options & !IGNORED_OPTIONS != 0 || locale != 0 {
            return Err(RUNTIME_LIMIT);
        }
        let stream = self.writable(file)?;
        let template = c_string(memory, format, MAX_TRANSFER + 1).map_err(fault)?;
        if template.len() as u64 > MAX_TRANSFER {
            return Err(RUNTIME_LIMIT);
        }
        let mut source = VaList {
            memory,
            next: arguments,
        };
        let text = match crt_format::format(&template, &mut source) {
            Ok(text) => text,
            // Windows calls the invalid parameter handler for an invalid
            // format. Xenora's choice for when execution continues: nothing
            // is written and the result is that of a null format.
            Err(FormatError::Invalid) => return Ok(Err(Invalid(-1))),
            Err(FormatError::Unsupported | FormatError::TooLong) => return Err(RUNTIME_LIMIT),
            Err(FormatError::Fault(error)) => return Err(fault(error)),
        };
        self.write(stream, &text)?;
        Ok(Ok(text.len() as i32))
    }

    /// `fwrite`. Returns the number of whole items written.
    pub(super) fn write_items(
        &mut self,
        memory: &GuestMemory,
        buffer: u64,
        size: u64,
        count: u64,
        file: u64,
    ) -> Outcome<u64> {
        if buffer == 0 || file == 0 {
            return Ok(Err(Invalid(0)));
        }
        let stream = self.writable(file)?;
        let Some(length) = size
            .checked_mul(count)
            .filter(|bytes| *bytes <= MAX_TRANSFER)
        else {
            return Err(RUNTIME_LIMIT);
        };
        if length == 0 {
            return Ok(Ok(0));
        }
        let mut bytes = vec![0; length as usize];
        memory.read_bytes(buffer, &mut bytes).map_err(fault)?;
        self.write(stream, &bytes)?;
        Ok(Ok(count))
    }

    /// `fputs`. Writes the string without its terminator and returns a
    /// nonnegative value, here 0; `EOF` is -1.
    pub(super) fn put_string(
        &mut self,
        memory: &GuestMemory,
        text: u64,
        file: u64,
    ) -> Outcome<i32> {
        if text == 0 || file == 0 {
            return Ok(Err(Invalid(-1)));
        }
        let stream = self.writable(file)?;
        let bytes = c_string(memory, text, MAX_TRANSFER + 1).map_err(fault)?;
        if bytes.len() as u64 > MAX_TRANSFER {
            return Err(RUNTIME_LIMIT);
        }
        self.write(stream, &bytes)?;
        Ok(Ok(0))
    }

    /// `fflush`. A null stream means every open stream. Nothing is ever held
    /// back, so there is nothing to do.
    pub(super) fn flush(&self, file: u64) -> Result<i32, ExitReason> {
        if file != 0 {
            self.stream(file)?;
        }
        Ok(0)
    }

    /// `setvbuf`. The mode and size are checked as documented and then have
    /// no effect.
    pub(super) fn set_buffer(&self, file: u64, mode: i32, size: u64) -> Outcome<i32> {
        if file == 0 {
            return Ok(Err(Invalid(-1)));
        }
        self.stream(file)?;
        let valid = match mode {
            // "No buffer is used, regardless of buffer or size."
            IONBF => true,
            // "Allowable range: 2 <= size <= INT_MAX".
            IOFBF | IOLBF => (2..=i32::MAX as u64).contains(&size),
            _ => false,
        };
        Ok(if valid { Ok(0) } else { Err(Invalid(-1)) })
    }
}
