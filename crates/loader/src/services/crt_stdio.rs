// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded C-locale `sprintf`/`snprintf` and an in-memory FILE stream subset.
//!
//! Streams live entirely in the runtime: names are single path components with
//! no directory separators or host mapping. Standard streams from
//! `__acrt_iob_func` are permanent cookies (stdin empty, stdout/stderr discard).

use super::ServicesState;
use crate::cpu::ExitReason;
use crate::imports::{ImportRegistry, ServiceStatus, Win64Args};
use crate::memory::{GuestMemory, MemoryError, PROCESS_HEAP};
use std::collections::BTreeMap;
use std::sync::Arc;

const UCRT: &str = "ucrtbase.dll";
const STDIO_API: &str = "api-ms-win-crt-stdio-l1-1-0.dll";
const EINVAL: u32 = 22;
const ENOENT: u32 = 2;
const ENOMEM: u32 = 12;
const MAX_FORMAT: usize = 4096;
const MAX_OUTPUT: usize = 1 << 20;
const MAX_PATH: usize = 260;
const MAX_STREAMS: usize = 32;
const MAX_FILE_BYTES: usize = 16 * 1024 * 1024;
const COOKIE_BYTES: usize = 16;
const COOKIE_MAGIC: u32 = 0x5845_4E46; // "FENX" LE visual as XENF bytes
const SEEK_SET: i32 = 0;
const SEEK_CUR: i32 = 1;
const SEEK_END: i32 = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    File,
    StdIn,
    StdOut,
    StdErr,
}

struct Stream {
    kind: Kind,
    name: String,
    data: Vec<u8>,
    position: usize,
    readable: bool,
    writable: bool,
    append: bool,
    eof: bool,
    error: bool,
}

#[derive(Default)]
pub(super) struct StdioState {
    files: BTreeMap<String, Vec<u8>>,
    streams: BTreeMap<u32, Stream>,
    next_id: u32,
    stdin: u64,
    stdout: u64,
    stderr: u64,
}

fn fault(error: MemoryError) -> ExitReason {
    ExitReason::MemoryFault(error)
}

fn set_errno(memory: &mut GuestMemory, teb: u64, error: u32) -> Result<(), ExitReason> {
    let address = memory.crt_errno_address(teb).map_err(fault)?;
    memory.write_u32(address, error).map_err(fault)
}

fn read_c_string(memory: &GuestMemory, address: u64, limit: usize) -> Result<Vec<u8>, ExitReason> {
    let mut bytes = Vec::new();
    for index in 0..limit {
        let byte = memory.read_u8(address + index as u64).map_err(fault)?;
        if byte == 0 {
            return Ok(bytes);
        }
        bytes.push(byte);
    }
    Err(ExitReason::IllegalInstruction(0xFE))
}

fn unsupported() -> ExitReason {
    ExitReason::IllegalInstruction(0xFE)
}

fn normalize_name(raw: &[u8]) -> Result<String, u32> {
    let name = std::str::from_utf8(raw).map_err(|_| EINVAL)?;
    if name.is_empty()
        || name.len() > MAX_PATH
        || name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
        || name == "."
        || name == ".."
    {
        return Err(EINVAL);
    }
    Ok(name.to_ascii_lowercase())
}

fn parse_mode(raw: &[u8]) -> Result<(bool, bool, bool, bool), u32> {
    let mode = std::str::from_utf8(raw).map_err(|_| EINVAL)?;
    let mut readable = false;
    let mut writable = false;
    let mut append = false;
    let mut truncate = false;
    let bytes = mode.as_bytes();
    if bytes.is_empty() {
        return Err(EINVAL);
    }
    match bytes[0] {
        b'r' => readable = true,
        b'w' => {
            writable = true;
            truncate = true;
        }
        b'a' => {
            writable = true;
            append = true;
        }
        _ => return Err(EINVAL),
    }
    for &byte in &bytes[1..] {
        match byte {
            b'+' => {
                readable = true;
                writable = true;
            }
            b'b' | b't' => {}
            _ => return Err(EINVAL),
        }
    }
    Ok((readable, writable, append, truncate))
}

fn write_cookie(memory: &mut GuestMemory, id: u32) -> Result<u64, ExitReason> {
    let cookie = memory
        .map_heap_allocation(PROCESS_HEAP, COOKIE_BYTES)
        .map_err(fault)?;
    let mut bytes = [0u8; COOKIE_BYTES];
    bytes[..4].copy_from_slice(&COOKIE_MAGIC.to_le_bytes());
    bytes[4..8].copy_from_slice(&id.to_le_bytes());
    memory.write_bytes(cookie, &bytes).map_err(fault)?;
    Ok(cookie)
}

fn read_cookie(memory: &GuestMemory, cookie: u64) -> Result<u32, ExitReason> {
    if cookie == 0 {
        return Err(ExitReason::IllegalInstruction(0xFE));
    }
    let mut bytes = [0u8; 8];
    memory.read_bytes(cookie, &mut bytes).map_err(fault)?;
    let magic = u32::from_le_bytes(bytes[..4].try_into().unwrap());
    if magic != COOKIE_MAGIC {
        return Err(ExitReason::IllegalInstruction(0xFE));
    }
    Ok(u32::from_le_bytes(bytes[4..8].try_into().unwrap()))
}

fn ensure_std_cookies(
    state: &Arc<ServicesState>,
    memory: &mut GuestMemory,
) -> Result<(), ExitReason> {
    let mut stdio = state.stdio.lock().expect("stdio state");
    if stdio.stdin != 0 {
        return Ok(());
    }
    let kinds = [Kind::StdIn, Kind::StdOut, Kind::StdErr];
    let mut cookies = [0u64; 3];
    for (index, kind) in kinds.into_iter().enumerate() {
        let id = stdio.next_id;
        stdio.next_id = id
            .checked_add(1)
            .ok_or(ExitReason::MemoryFault(MemoryError::AllocationFailed))?;
        drop(stdio);
        let cookie = write_cookie(memory, id)?;
        cookies[index] = cookie;
        let mut locked = state.stdio.lock().expect("stdio state");
        locked.streams.insert(
            id,
            Stream {
                kind,
                name: match kind {
                    Kind::StdIn => "stdin".into(),
                    Kind::StdOut => "stdout".into(),
                    Kind::StdErr => "stderr".into(),
                    Kind::File => unreachable!(),
                },
                data: Vec::new(),
                position: 0,
                readable: kind == Kind::StdIn,
                writable: kind != Kind::StdIn,
                append: false,
                eof: kind == Kind::StdIn,
                error: false,
            },
        );
        stdio = locked;
    }
    stdio.stdin = cookies[0];
    stdio.stdout = cookies[1];
    stdio.stderr = cookies[2];
    Ok(())
}

fn open_stream(
    state: &Arc<ServicesState>,
    memory: &mut GuestMemory,
    teb: u64,
    path: u64,
    mode: u64,
) -> Result<u64, ExitReason> {
    let path = read_c_string(memory, path, MAX_PATH)?;
    let mode = read_c_string(memory, mode, 16)?;
    let name = match normalize_name(&path) {
        Ok(name) => name,
        Err(error) => {
            set_errno(memory, teb, error)?;
            return Ok(0);
        }
    };
    let (readable, writable, append, truncate) = match parse_mode(&mode) {
        Ok(mode) => mode,
        Err(error) => {
            set_errno(memory, teb, error)?;
            return Ok(0);
        }
    };
    let mut stdio = state.stdio.lock().expect("stdio state");
    if stdio.streams.len() >= MAX_STREAMS {
        set_errno(memory, teb, ENOMEM)?;
        return Ok(0);
    }
    let existing = stdio.files.get(&name).cloned();
    let data = if truncate {
        Vec::new()
    } else if let Some(data) = existing {
        data
    } else if readable && !writable {
        set_errno(memory, teb, ENOENT)?;
        return Ok(0);
    } else {
        Vec::new()
    };
    if data.len() > MAX_FILE_BYTES {
        set_errno(memory, teb, ENOMEM)?;
        return Ok(0);
    }
    let position = if append { data.len() } else { 0 };
    let id = stdio.next_id;
    stdio.next_id = id
        .checked_add(1)
        .ok_or(ExitReason::MemoryFault(MemoryError::AllocationFailed))?;
    if truncate || !stdio.files.contains_key(&name) {
        stdio.files.insert(name.clone(), Vec::new());
    }
    drop(stdio);
    let cookie = write_cookie(memory, id)?;
    let mut stdio = state.stdio.lock().expect("stdio state");
    stdio.streams.insert(
        id,
        Stream {
            kind: Kind::File,
            name,
            data,
            position,
            readable,
            writable,
            append,
            eof: false,
            error: false,
        },
    );
    Ok(cookie)
}

fn with_stream<T>(
    state: &Arc<ServicesState>,
    memory: &mut GuestMemory,
    cookie: u64,
    body: impl FnOnce(&mut Stream, &mut GuestMemory) -> Result<T, ExitReason>,
) -> Result<T, ExitReason> {
    let id = read_cookie(memory, cookie)?;
    let mut stdio = state.stdio.lock().expect("stdio state");
    let stream = stdio
        .streams
        .get_mut(&id)
        .ok_or(ExitReason::IllegalInstruction(0xFE))?;
    body(stream, memory)
}

#[derive(Clone, Copy)]
enum Length {
    Default,
    Long,
    LongLong,
}

fn parse_length(format: &[u8], index: &mut usize) -> Length {
    if format.get(*index) == Some(&b'l') {
        *index += 1;
        if format.get(*index) == Some(&b'l') {
            *index += 1;
            Length::LongLong
        } else {
            Length::Long
        }
    } else if format.get(*index) == Some(&b'z') || format.get(*index) == Some(&b't') {
        *index += 1;
        Length::LongLong
    } else {
        Length::Default
    }
}

fn format_into(
    memory: &GuestMemory,
    args: &Win64Args,
    format_va: u64,
    first_arg: usize,
) -> Result<Vec<u8>, ExitReason> {
    let format = read_c_string(memory, format_va, MAX_FORMAT)?;
    let mut out = Vec::new();
    let mut index = 0;
    let mut arg = first_arg;
    while index < format.len() {
        if out.len() > MAX_OUTPUT {
            return Err(unsupported());
        }
        let byte = format[index];
        index += 1;
        if byte != b'%' {
            out.push(byte);
            continue;
        }
        if format.get(index) == Some(&b'%') {
            index += 1;
            out.push(b'%');
            continue;
        }
        let mut width = 0usize;
        let mut pad = b' ';
        if format.get(index) == Some(&b'0') {
            pad = b'0';
            index += 1;
        }
        while let Some(digit @ b'0'..=b'9') = format.get(index).copied() {
            width = width
                .checked_mul(10)
                .and_then(|value| value.checked_add((digit - b'0') as usize))
                .ok_or_else(unsupported)?;
            index += 1;
        }
        let length = parse_length(&format, &mut index);
        let spec = *format.get(index).ok_or_else(unsupported)?;
        index += 1;
        let value = args.get(arg, memory).map_err(fault)?;
        arg += 1;
        let rendered = match spec {
            b'c' => vec![value as u8],
            b's' => {
                if value == 0 {
                    b"(null)".to_vec()
                } else {
                    read_c_string(memory, value, MAX_OUTPUT)?
                }
            }
            b'd' | b'i' => {
                let number = match length {
                    Length::Default | Length::Long => value as i32 as i64,
                    Length::LongLong => value as i64,
                };
                number.to_string().into_bytes()
            }
            b'u' => {
                let number = match length {
                    Length::Default | Length::Long => value as u32 as u64,
                    Length::LongLong => value,
                };
                number.to_string().into_bytes()
            }
            b'x' | b'X' => {
                let number = match length {
                    Length::Default | Length::Long => value as u32 as u64,
                    Length::LongLong => value,
                };
                let text = if spec == b'X' {
                    format!("{number:X}")
                } else {
                    format!("{number:x}")
                };
                text.into_bytes()
            }
            b'p' => format!("{value:x}").into_bytes(),
            _ => return Err(unsupported()),
        };
        if width > rendered.len() {
            let padding = width - rendered.len();
            if pad == b'0' && rendered.first() == Some(&b'-') {
                out.push(b'-');
                out.extend(std::iter::repeat_n(b'0', padding));
                out.extend_from_slice(&rendered[1..]);
            } else {
                out.extend(std::iter::repeat_n(pad, padding));
                out.extend_from_slice(&rendered);
            }
        } else {
            out.extend_from_slice(&rendered);
        }
    }
    Ok(out)
}

fn write_formatted(
    memory: &mut GuestMemory,
    teb: u64,
    buffer: u64,
    capacity: Option<u64>,
    args: &Win64Args,
    format_index: usize,
    first_arg: usize,
) -> Result<u64, ExitReason> {
    if buffer == 0 {
        set_errno(memory, teb, EINVAL)?;
        return Ok(u64::from(u32::MAX));
    }
    let format = args.get(format_index, memory).map_err(fault)?;
    if format == 0 {
        set_errno(memory, teb, EINVAL)?;
        return Ok(u64::from(u32::MAX));
    }
    let formatted = format_into(memory, args, format, first_arg)?;
    let needed = formatted.len() as u64;
    match capacity {
        None => {
            let mut bytes = formatted;
            bytes.push(0);
            memory.write_bytes(buffer, &bytes).map_err(fault)?;
            Ok(needed)
        }
        Some(0) => Ok(needed),
        Some(count) => {
            let room = count as usize;
            if room == 0 {
                return Ok(needed);
            }
            let copy = formatted.len().min(room.saturating_sub(1));
            memory
                .write_bytes(buffer, &formatted[..copy])
                .map_err(fault)?;
            memory.write_u8(buffer + copy as u64, 0).map_err(fault)?;
            Ok(needed)
        }
    }
}

pub(super) fn register(registry: &mut ImportRegistry, state: &Arc<ServicesState>) {
    registry.register(UCRT, "sprintf", |args, cpu, memory| {
        write_formatted(memory, cpu.gs_base, args.rcx, None, args, 1, 2)
    });
    registry.register(UCRT, "snprintf", |args, cpu, memory| {
        write_formatted(memory, cpu.gs_base, args.rcx, Some(args.rdx), args, 2, 3)
    });

    let fopen = Arc::clone(state);
    registry.register(UCRT, "fopen", move |args, cpu, memory| {
        open_stream(&fopen, memory, cpu.gs_base, args.rcx, args.rdx)
    });

    let fclose = Arc::clone(state);
    registry.register(UCRT, "fclose", move |args, cpu, memory| {
        if args.rcx == 0 {
            set_errno(memory, cpu.gs_base, EINVAL)?;
            return Ok(u64::from(u32::MAX));
        }
        let id = read_cookie(memory, args.rcx)?;
        let mut stdio = fclose.stdio.lock().expect("stdio state");
        let Some(stream) = stdio.streams.remove(&id) else {
            set_errno(memory, cpu.gs_base, EINVAL)?;
            return Ok(u64::from(u32::MAX));
        };
        if stream.kind == Kind::File {
            stdio.files.insert(stream.name, stream.data);
        }
        let _ = memory.release_heap_allocation(PROCESS_HEAP, args.rcx);
        Ok(0)
    });

    let fread = Arc::clone(state);
    registry.register(UCRT, "fread", move |args, _cpu, memory| {
        let (buffer, size, count, cookie) = (args.rcx, args.rdx, args.r8, args.r9);
        if size == 0 || count == 0 {
            return Ok(0);
        }
        let total = size
            .checked_mul(count)
            .ok_or(ExitReason::IllegalInstruction(0xFE))?;
        if total as usize > MAX_OUTPUT {
            return Err(ExitReason::IllegalInstruction(0xFE));
        }
        with_stream(&fread, memory, cookie, |stream, memory| {
            if !stream.readable {
                stream.error = true;
                return Ok(0);
            }
            if stream.kind == Kind::StdIn {
                stream.eof = true;
                return Ok(0);
            }
            let available = stream.data.len().saturating_sub(stream.position);
            let take = (total as usize).min(available);
            let complete = take / size as usize * size as usize;
            if complete == 0 {
                stream.eof = true;
                return Ok(0);
            }
            memory
                .write_bytes(buffer, &stream.data[stream.position..stream.position + complete])
                .map_err(fault)?;
            stream.position += complete;
            if stream.position >= stream.data.len() {
                stream.eof = true;
            }
            Ok((complete / size as usize) as u64)
        })
    });

    let fwrite = Arc::clone(state);
    registry.register(UCRT, "fwrite", move |args, _cpu, memory| {
        let (buffer, size, count, cookie) = (args.rcx, args.rdx, args.r8, args.r9);
        if size == 0 || count == 0 {
            return Ok(0);
        }
        let total = size
            .checked_mul(count)
            .ok_or(ExitReason::IllegalInstruction(0xFE))?;
        if total as usize > MAX_OUTPUT {
            return Err(ExitReason::IllegalInstruction(0xFE));
        }
        let mut bytes = vec![0u8; total as usize];
        memory.read_bytes(buffer, &mut bytes).map_err(fault)?;
        with_stream(&fwrite, memory, cookie, |stream, _| {
            if !stream.writable || stream.kind == Kind::StdIn {
                stream.error = true;
                return Ok(0);
            }
            if matches!(stream.kind, Kind::StdOut | Kind::StdErr) {
                return Ok(count);
            }
            if stream.append {
                stream.position = stream.data.len();
            }
            let end = stream
                .position
                .checked_add(bytes.len())
                .ok_or(ExitReason::IllegalInstruction(0xFE))?;
            if end > MAX_FILE_BYTES {
                stream.error = true;
                return Ok(0);
            }
            if end > stream.data.len() {
                stream.data.resize(end, 0);
            }
            stream.data[stream.position..end].copy_from_slice(&bytes);
            stream.position = end;
            stream.eof = false;
            Ok(count)
        })
    });

    let fseek = Arc::clone(state);
    registry.register(UCRT, "fseek", move |args, cpu, memory| {
        let cookie = args.rcx;
        let offset = args.rdx as i32 as i64;
        let whence = args.r8 as i32;
        with_stream(&fseek, memory, cookie, |stream, memory| {
            if stream.kind != Kind::File {
                set_errno(memory, cpu.gs_base, EINVAL)?;
                return Ok(u64::from(u32::MAX));
            }
            let base = match whence {
                SEEK_SET => 0i64,
                SEEK_CUR => stream.position as i64,
                SEEK_END => stream.data.len() as i64,
                _ => {
                    set_errno(memory, cpu.gs_base, EINVAL)?;
                    return Ok(u64::from(u32::MAX));
                }
            };
            let Some(position) = base.checked_add(offset) else {
                set_errno(memory, cpu.gs_base, EINVAL)?;
                return Ok(u64::from(u32::MAX));
            };
            if position < 0 {
                set_errno(memory, cpu.gs_base, EINVAL)?;
                return Ok(u64::from(u32::MAX));
            }
            stream.position = position as usize;
            stream.eof = false;
            Ok(0)
        })
    });

    let ftell = Arc::clone(state);
    registry.register(UCRT, "ftell", move |args, cpu, memory| {
        with_stream(&ftell, memory, args.rcx, |stream, memory| {
            if stream.kind != Kind::File {
                set_errno(memory, cpu.gs_base, EINVAL)?;
                return Ok(u64::from(u32::MAX));
            }
            Ok(stream.position as u64)
        })
    });

    let feof = Arc::clone(state);
    registry.register(UCRT, "feof", move |args, _, memory| {
        with_stream(&feof, memory, args.rcx, |stream, _| Ok(u64::from(stream.eof)))
    });

    let ferror = Arc::clone(state);
    registry.register(UCRT, "ferror", move |args, _, memory| {
        with_stream(&ferror, memory, args.rcx, |stream, _| {
            Ok(u64::from(stream.error))
        })
    });

    let rewind = Arc::clone(state);
    registry.register(UCRT, "rewind", move |args, _, memory| {
        with_stream(&rewind, memory, args.rcx, |stream, _| {
            stream.position = 0;
            stream.eof = false;
            stream.error = false;
            Ok(0)
        })
    });

    let iob = Arc::clone(state);
    registry.register(UCRT, "__acrt_iob_func", move |args, _, memory| {
        ensure_std_cookies(&iob, memory)?;
        let stdio = iob.stdio.lock().expect("stdio state");
        Ok(match args.rcx as u32 {
            0 => stdio.stdin,
            1 => stdio.stdout,
            2 => stdio.stderr,
            _ => 0,
        })
    });

    let fprintf = Arc::clone(state);
    registry.register(UCRT, "fprintf", move |args, cpu, memory| {
        ensure_std_cookies(&fprintf, memory)?;
        let cookie = args.rcx;
        let formatted = format_into(memory, args, args.rdx, 2)?;
        if cookie == 0 {
            set_errno(memory, cpu.gs_base, EINVAL)?;
            return Ok(u64::from(u32::MAX));
        }
        let count = formatted.len() as u64;
        with_stream(&fprintf, memory, cookie, |stream, _| {
            if !stream.writable || stream.kind == Kind::StdIn {
                stream.error = true;
                return Ok(u64::from(u32::MAX));
            }
            if matches!(stream.kind, Kind::StdOut | Kind::StdErr) {
                return Ok(count);
            }
            let end = stream
                .position
                .checked_add(formatted.len())
                .ok_or(ExitReason::IllegalInstruction(0xFE))?;
            if end > MAX_FILE_BYTES {
                stream.error = true;
                return Ok(u64::from(u32::MAX));
            }
            if end > stream.data.len() {
                stream.data.resize(end, 0);
            }
            stream.data[stream.position..end].copy_from_slice(&formatted);
            stream.position = end;
            stream.eof = false;
            Ok(count)
        })
    });

    let printf = Arc::clone(state);
    registry.register(UCRT, "printf", move |args, cpu, memory| {
        ensure_std_cookies(&printf, memory)?;
        let stdout = {
            let stdio = printf.stdio.lock().expect("stdio state");
            stdio.stdout
        };
        let formatted = format_into(memory, args, args.rcx, 1)?;
        let count = formatted.len() as u64;
        with_stream(&printf, memory, stdout, |stream, _| {
            if !stream.writable {
                stream.error = true;
                set_errno(memory, cpu.gs_base, EINVAL)?;
                return Ok(u64::from(u32::MAX));
            }
            // stdout discards payload and reports the formatted length.
            let _ = stream;
            Ok(count)
        })
    });

    for symbol in [
        "sprintf",
        "snprintf",
        "fopen",
        "fclose",
        "fread",
        "fwrite",
        "fseek",
        "ftell",
        "feof",
        "ferror",
        "rewind",
        "__acrt_iob_func",
        "fprintf",
        "printf",
    ] {
        registry.alias(STDIO_API, UCRT, symbol);
        registry.set_status(UCRT, symbol, ServiceStatus::Partial);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::{CpuState, Register};
    use crate::import_table::ImportName;
    use crate::services::MockClock;

    fn call(
        registry: &ImportRegistry,
        symbol: &str,
        args: [u64; 4],
        cpu: &mut CpuState,
        memory: &mut GuestMemory,
    ) -> u64 {
        cpu.set(Register::Rcx, args[0]);
        cpu.set(Register::Rdx, args[1]);
        cpu.set(Register::R8, args[2]);
        cpu.set(Register::R9, args[3]);
        let service = registry
            .resolve(UCRT, &ImportName::Name(symbol.into()))
            .unwrap();
        (service.handler)(&Win64Args::new(cpu), cpu, memory).unwrap()
    }

    #[test]
    fn sprintf_formats_the_supported_subset() {
        let state = Arc::new(ServicesState::new(Arc::new(MockClock::new(0))));
        let mut registry = ImportRegistry::new();
        register(&mut registry, &state);
        let mut memory = GuestMemory::new();
        let mut cpu = CpuState {
            gs_base: 0x10000,
            ..CpuState::default()
        };
        let buffer = memory.map_heap_allocation(PROCESS_HEAP, 64).unwrap();
        let format = memory.map_heap_allocation(PROCESS_HEAP, 32).unwrap();
        memory.write_bytes(format, b"%s-%d-%x\0").unwrap();
        let text = memory.map_heap_allocation(PROCESS_HEAP, 8).unwrap();
        memory.write_bytes(text, b"ok\0").unwrap();
        let stack = memory.map_heap_allocation(PROCESS_HEAP, 64).unwrap();
        cpu.set(Register::Rsp, stack);
        memory.write_u64(stack + 40, 0x2a).unwrap();
        let written = call(
            &registry,
            "sprintf",
            [buffer, format, text, 7],
            &mut cpu,
            &mut memory,
        );
        assert_eq!(written, 7);
        let mut out = [0u8; 8];
        memory.read_bytes(buffer, &mut out).unwrap();
        assert_eq!(&out, b"ok-7-2a\0");
    }

    #[test]
    fn fopen_fwrite_fread_round_trip_in_memory() {
        let state = Arc::new(ServicesState::new(Arc::new(MockClock::new(0))));
        let mut registry = ImportRegistry::new();
        register(&mut registry, &state);
        let mut memory = GuestMemory::new();
        let mut cpu = CpuState {
            gs_base: 0x10000,
            ..CpuState::default()
        };
        let path = memory.map_heap_allocation(PROCESS_HEAP, 16).unwrap();
        let mode_w = memory.map_heap_allocation(PROCESS_HEAP, 8).unwrap();
        let mode_r = memory.map_heap_allocation(PROCESS_HEAP, 8).unwrap();
        let payload = memory.map_heap_allocation(PROCESS_HEAP, 16).unwrap();
        let out = memory.map_heap_allocation(PROCESS_HEAP, 16).unwrap();
        memory.write_bytes(path, b"save.dat\0").unwrap();
        memory.write_bytes(mode_w, b"wb\0").unwrap();
        memory.write_bytes(mode_r, b"rb\0").unwrap();
        memory.write_bytes(payload, b"hello\0").unwrap();
        let file = call(
            &registry,
            "fopen",
            [path, mode_w, 0, 0],
            &mut cpu,
            &mut memory,
        );
        assert_ne!(file, 0);
        assert_eq!(
            call(&registry, "fwrite", [payload, 1, 5, file], &mut cpu, &mut memory),
            5
        );
        assert_eq!(call(&registry, "fclose", [file, 0, 0, 0], &mut cpu, &mut memory), 0);
        let reader = call(
            &registry,
            "fopen",
            [path, mode_r, 0, 0],
            &mut cpu,
            &mut memory,
        );
        assert_ne!(reader, 0);
        assert_eq!(
            call(&registry, "fread", [out, 1, 5, reader], &mut cpu, &mut memory),
            5
        );
        let mut got = [0u8; 5];
        memory.read_bytes(out, &mut got).unwrap();
        assert_eq!(&got, b"hello");
        assert_eq!(call(&registry, "fclose", [reader, 0, 0, 0], &mut cpu, &mut memory), 0);
    }
}
