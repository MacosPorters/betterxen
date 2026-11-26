//! Minimal Mach-O header reader: which CPU architectures a binary contains.
//!
//! Only the header is inspected (thin 64-bit images and universal "fat"
//! containers). This is enough to prove that a user-supplied
//! `D3DMetal.framework` is x86_64-only and must therefore be hosted by an
//! x86_64 process under Rosetta 2.

use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

/// Bytes read from the start of a file; covers a fat header with
/// [`MAX_FAT_ARCHS`] 64-bit entries.
pub const HEADER_PREFIX_LEN: u64 = 4096;
/// Upper bound on fat entries accepted before the header is rejected.
pub const MAX_FAT_ARCHS: u32 = 16;

const MH_MAGIC_64: u32 = 0xfeed_facf;
const FAT_MAGIC: u32 = 0xcafe_babe;
const FAT_MAGIC_64: u32 = 0xcafe_babf;
const FAT_HEADER_LEN: usize = 8;
const FAT_ARCH_LEN: usize = 20;
const FAT_ARCH_64_LEN: usize = 32;
const CPU_TYPE_X86_64: u32 = 0x0100_0007;
const CPU_TYPE_ARM64: u32 = 0x0100_000c;

/// A CPU architecture slice found in a Mach-O header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuArch {
    X86_64,
    Arm64,
    Other(u32),
}

impl CpuArch {
    fn from_cputype(cputype: u32) -> Self {
        match cputype {
            CPU_TYPE_X86_64 => Self::X86_64,
            CPU_TYPE_ARM64 => Self::Arm64,
            other => Self::Other(other),
        }
    }
}

impl fmt::Display for CpuArch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::X86_64 => f.write_str("x86_64"),
            Self::Arm64 => f.write_str("arm64"),
            Self::Other(cputype) => write!(f, "cputype 0x{cputype:08x}"),
        }
    }
}

/// Why a header could not be classified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MachOError {
    Truncated,
    UnknownMagic(u32),
    TooManyArchs(u32),
}

impl fmt::Display for MachOError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => f.write_str("truncated Mach-O header"),
            Self::UnknownMagic(magic) => {
                write!(f, "not a 64-bit or universal Mach-O (magic 0x{magic:08x})")
            }
            Self::TooManyArchs(n) => {
                write!(
                    f,
                    "universal header declares {n} slices (limit {MAX_FAT_ARCHS})"
                )
            }
        }
    }
}

impl std::error::Error for MachOError {}

fn u32_le(bytes: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(off..off + 4)?.try_into().ok()?,
    ))
}

fn u32_be(bytes: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(off..off + 4)?.try_into().ok()?,
    ))
}

/// Lists the architectures declared by a Mach-O header prefix.
pub fn archs(bytes: &[u8]) -> Result<Vec<CpuArch>, MachOError> {
    let magic_be = u32_be(bytes, 0).ok_or(MachOError::Truncated)?;
    let entry_len = match magic_be {
        FAT_MAGIC => FAT_ARCH_LEN,
        FAT_MAGIC_64 => FAT_ARCH_64_LEN,
        _ => {
            let magic_le = u32_le(bytes, 0).ok_or(MachOError::Truncated)?;
            if magic_le != MH_MAGIC_64 {
                return Err(MachOError::UnknownMagic(magic_le));
            }
            let cputype = u32_le(bytes, 4).ok_or(MachOError::Truncated)?;
            return Ok(vec![CpuArch::from_cputype(cputype)]);
        }
    };
    let count = u32_be(bytes, 4).ok_or(MachOError::Truncated)?;
    if count > MAX_FAT_ARCHS {
        return Err(MachOError::TooManyArchs(count));
    }
    (0..count as usize)
        .map(|i| {
            u32_be(bytes, FAT_HEADER_LEN + i * entry_len)
                .map(CpuArch::from_cputype)
                .ok_or(MachOError::Truncated)
        })
        .collect()
}

/// Reads the header prefix of `path` and lists its architectures.
pub fn read_archs(path: &Path) -> io::Result<Result<Vec<CpuArch>, MachOError>> {
    let mut prefix = Vec::new();
    File::open(path)?
        .take(HEADER_PREFIX_LEN)
        .read_to_end(&mut prefix)?;
    Ok(archs(&prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thin(cputype: u32) -> Vec<u8> {
        let mut b = MH_MAGIC_64.to_le_bytes().to_vec();
        b.extend_from_slice(&cputype.to_le_bytes());
        b.extend_from_slice(&[0; 24]);
        b
    }

    fn fat(magic: u32, entry_len: usize, cputypes: &[u32]) -> Vec<u8> {
        let mut b = magic.to_be_bytes().to_vec();
        b.extend_from_slice(&(cputypes.len() as u32).to_be_bytes());
        for &cputype in cputypes {
            let mut entry = vec![0; entry_len];
            entry[..4].copy_from_slice(&cputype.to_be_bytes());
            b.extend_from_slice(&entry);
        }
        b
    }

    #[test]
    fn thin_x86_64_and_arm64() {
        assert_eq!(archs(&thin(0x0100_0007)), Ok(vec![CpuArch::X86_64]));
        assert_eq!(archs(&thin(0x0100_000c)), Ok(vec![CpuArch::Arm64]));
        assert_eq!(archs(&thin(7)), Ok(vec![CpuArch::Other(7)]));
    }

    #[test]
    fn universal_32_and_64_bit_entries() {
        let both = [0x0100_0007, 0x0100_000c];
        let expected = vec![CpuArch::X86_64, CpuArch::Arm64];
        assert_eq!(archs(&fat(0xcafe_babe, 20, &both)), Ok(expected.clone()));
        assert_eq!(archs(&fat(0xcafe_babf, 32, &both)), Ok(expected));
    }

    #[test]
    fn rejects_unknown_truncated_and_oversized_headers() {
        assert_eq!(archs(&[0xcf, 0xfa]), Err(MachOError::Truncated));
        assert_eq!(
            archs(b"\x7fELF\x02\x01\x01\x00"),
            Err(MachOError::UnknownMagic(0x464c_457f))
        );
        let mut short = fat(0xcafe_babe, 20, &[0x0100_0007, 0x0100_000c]);
        short.truncate(FAT_HEADER_LEN + FAT_ARCH_LEN + 2);
        assert_eq!(archs(&short), Err(MachOError::Truncated));
        let mut huge = 0xcafe_babe_u32.to_be_bytes().to_vec();
        huge.extend_from_slice(&1_000_000u32.to_be_bytes());
        assert_eq!(archs(&huge), Err(MachOError::TooManyArchs(1_000_000)));
    }
}
