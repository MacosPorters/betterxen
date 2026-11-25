// SPDX-License-Identifier: GPL-3.0-or-later

//! Character and line stdio on in-memory CRT streams through the guest import
//! table. Expectations follow the public UCRT pages for fputc, putc, fputs,
//! puts, fgetc, getc and fflush: the character functions return the byte as an
//! unsigned char or EOF, fputs and puts return a non-negative value or EOF,
//! and fflush returns 0 or EOF.

mod support;

use support::*;

const UCRT: &str = "ucrtbase.dll";
const IMPORTS: &[(&str, &str)] = &[
    (UCRT, "fopen"),
    (UCRT, "fclose"),
    (UCRT, "feof"),
    (UCRT, "ferror"),
    (UCRT, "fputc"),
    (UCRT, "putc"),
    (UCRT, "fputs"),
    (UCRT, "puts"),
    (UCRT, "fgetc"),
    ("api-ms-win-crt-stdio-l1-1-0.dll", "getc"),
    (UCRT, "fflush"),
];

const EOF: u64 = u32::MAX as u64;
const NAME: u64 = SCRATCH;
const WRITE: u64 = SCRATCH + 0x40;
const READ: u64 = SCRATCH + 0x48;
const TEXT: u64 = SCRATCH + 0x80;

fn guest() -> Guest {
    let mut guest = Guest::new(IMPORTS, &[]);
    guest.memory.write_bytes(NAME, b"chars.txt\0").unwrap();
    guest.memory.write_bytes(WRITE, b"w\0").unwrap();
    guest.memory.write_bytes(READ, b"r\0").unwrap();
    guest
}

fn read_all(guest: &mut Guest, stream: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let byte = guest.call("fgetc", &[stream]);
        if byte == EOF {
            return bytes;
        }
        assert!(byte <= 0xFF, "fgetc returns an unsigned char");
        bytes.push(byte as u8);
    }
}

#[test]
fn characters_and_lines_round_trip_through_a_file() {
    let mut guest = guest();
    let out = guest.call("fopen", &[NAME, WRITE]);
    assert_ne!(out, 0);
    // fputc writes (unsigned char)c and returns it.
    assert_eq!(guest.call("fputc", &[0x141, out]), 0x41);
    assert_eq!(guest.call("putc", &[0xFF, out]), 0xFF);
    guest.memory.write_bytes(TEXT, b"line one\0").unwrap();
    assert_eq!(guest.call("fputs", &[TEXT, out]), 0, "no newline appended");
    assert_eq!(guest.call("fclose", &[out]), 0);

    let input = guest.call("fopen", &[NAME, READ]);
    assert_eq!(read_all(&mut guest, input), b"A\xFFline one");
    assert_ne!(guest.call("feof", &[input]), 0);
    assert_eq!(guest.call("ferror", &[input]), 0);
    // Writing to a read-only stream fails and sets the error indicator.
    assert_eq!(guest.call("fputc", &[b'x' as u64, input]), EOF);
    assert_ne!(guest.call("ferror", &[input]), 0);
    assert_eq!(guest.call("fclose", &[input]), 0);
}

#[test]
fn reading_a_write_only_stream_fails() {
    let mut guest = guest();
    let out = guest.call("fopen", &[NAME, WRITE]);
    assert_eq!(guest.call("fgetc", &[out]), EOF);
    assert_ne!(guest.call("ferror", &[out]), 0);
    assert_eq!(
        guest.call("getc", &[out]),
        EOF,
        "the stdio API set reaches the same service"
    );
    assert_eq!(guest.call("fclose", &[out]), 0);
}

#[test]
fn fflush_publishes_contents_before_close() {
    let mut guest = guest();
    let out = guest.call("fopen", &[NAME, WRITE]);
    guest.memory.write_bytes(TEXT, b"early\0").unwrap();
    guest.call("fputs", &[TEXT, out]);
    assert_eq!(guest.call("fflush", &[out]), 0);
    // A second stream sees the flushed contents while the first is open.
    let input = guest.call("fopen", &[NAME, READ]);
    assert_eq!(read_all(&mut guest, input), b"early");
    assert_eq!(
        guest.call("fflush", &[input]),
        0,
        "flushing input has no effect"
    );
    assert_eq!(guest.call("fflush", &[0]), 0, "null flushes every stream");
    assert_eq!(guest.call("fclose", &[input]), 0);
    assert_eq!(guest.call("fclose", &[out]), 0);
}

#[test]
fn puts_writes_a_line_to_standard_output() {
    let mut guest = guest();
    guest.memory.write_bytes(TEXT, b"hello\0").unwrap();
    // Standard output accepts and discards output; puts reports success.
    assert_eq!(guest.call("puts", &[TEXT]), 0);
}
