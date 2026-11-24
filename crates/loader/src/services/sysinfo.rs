// SPDX-License-Identifier: GPL-3.0-or-later

//! System information, calendar time and sleeping.
//!
//! The guest sees a simulated machine described by [`GuestSystemProfile`].
//! [`HostCapabilities`] reports what the Mac actually has. The two are kept
//! separate so detected host hardware is never presented to the guest as its
//! own, and guest-visible values do not change from one Mac to another.

use super::{ERROR_INVALID_PARAMETER, KERNEL32, ServicesState};
use crate::cpu::ExitReason;
use crate::imports::{ImportRegistry, ServiceStatus};
use crate::memory::{ALLOCATION_GRANULARITY, DYNAMIC_BUDGET, GuestMemory, MemoryError};
use std::sync::Arc;

const SYSINFO_API: &str = "api-ms-win-core-sysinfo-l1-1-0.dll";
const SYSINFO_API_2: &str = "api-ms-win-core-sysinfo-l1-2-0.dll";
const TIMEZONE_API: &str = "api-ms-win-core-timezone-l1-1-0.dll";
const SYNCH_API: &str = "api-ms-win-core-synch-l1-1-0.dll";
const SYNCH_API_2: &str = "api-ms-win-core-synch-l1-2-0.dll";
const REALTIME_API: &str = "api-ms-win-core-realtime-l1-1-0.dll";

const PROCESSOR_ARCHITECTURE_AMD64: u16 = 9;
const PROCESSOR_AMD_X8664: u32 = 8664;
const GUEST_PAGE_SIZE: u32 = 4096;
/// The x64 Windows user address range.
const MINIMUM_APPLICATION_ADDRESS: u64 = 0x1_0000;
const MAXIMUM_APPLICATION_ADDRESS: u64 = 0x7fff_fffe_ffff;
const MEMORYSTATUSEX_SIZE: u32 = 64;

const INFINITE: u32 = u32::MAX;
/// Longest sleep the runtime performs. A single guest thread sleeping longer
/// is reported rather than silently shortened.
const MAX_SLEEP_MILLIS: u32 = 10_000;

const TICKS_PER_MILLI: u64 = 10_000;
const MILLIS_PER_DAY: u64 = 86_400_000;
/// Days from 1601-01-01 to 1970-01-01.
const DAYS_1601_TO_1970: i64 = 134_774;
/// FILETIME values at or above this are rejected by FileTimeToSystemTime.
const MAX_FILETIME: u64 = 0x8000_0000_0000_0000;

/// What the guest is told about the machine it runs on.
#[derive(Clone, Debug)]
pub struct GuestSystemProfile {
    /// Reported processors. One, because guest code runs on one thread at a time.
    pub processors: u32,
}

impl Default for GuestSystemProfile {
    fn default() -> Self {
        Self { processors: 1 }
    }
}

/// Hardware detected on the host Mac. Never reported to the guest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostCapabilities {
    pub logical_processors: Option<u64>,
    pub physical_memory: Option<u64>,
    pub page_size: Option<u64>,
}

impl HostCapabilities {
    pub fn detect() -> Self {
        let logical_processors = std::thread::available_parallelism()
            .ok()
            .map(|count| count.get() as u64);
        // SAFETY: sysconf takes no pointers.
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        Self {
            logical_processors,
            physical_memory: physical_memory(),
            page_size: u64::try_from(page_size).ok().filter(|&size| size > 0),
        }
    }
}

#[cfg(target_os = "macos")]
fn physical_memory() -> Option<u64> {
    let mut bytes = 0u64;
    let mut size = std::mem::size_of::<u64>();
    // SAFETY: the name is NUL-terminated and the output buffer is a u64 whose
    // size is passed in `size`; no new value is written.
    let status = unsafe {
        libc::sysctlbyname(
            c"hw.memsize".as_ptr(),
            (&raw mut bytes).cast(),
            &raw mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    (status == 0 && size == std::mem::size_of::<u64>()).then_some(bytes)
}

#[cfg(not(target_os = "macos"))]
fn physical_memory() -> Option<u64> {
    None
}

/// A Windows `SYSTEMTIME`, in field order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SystemTime {
    year: u16,
    month: u16,
    day_of_week: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    milliseconds: u16,
}

impl SystemTime {
    fn to_bytes(self) -> [u8; 16] {
        let fields = [
            self.year,
            self.month,
            self.day_of_week,
            self.day,
            self.hour,
            self.minute,
            self.second,
            self.milliseconds,
        ];
        let mut bytes = [0; 16];
        for (chunk, field) in bytes.as_chunks_mut::<2>().0.iter_mut().zip(fields) {
            *chunk = field.to_le_bytes();
        }
        bytes
    }

    fn from_bytes(bytes: [u8; 16]) -> Self {
        let field = |index: usize| u16::from_le_bytes([bytes[2 * index], bytes[2 * index + 1]]);
        Self {
            year: field(0),
            month: field(1),
            day_of_week: field(2),
            day: field(3),
            hour: field(4),
            minute: field(5),
            second: field(6),
            milliseconds: field(7),
        }
    }
}

/// Proleptic Gregorian date for a day count relative to 1970-01-01.
fn civil_from_days(days: i64) -> (i64, u16, u16) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month as u16, day as u16)
}

fn days_from_civil(year: i64, month: u16, day: u16) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (i64::from(month) + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn is_leap(year: u16) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

fn days_in_month(year: u16, month: u16) -> u16 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn filetime_to_system_time(filetime: u64) -> Option<SystemTime> {
    if filetime >= MAX_FILETIME {
        return None;
    }
    let millis = filetime / TICKS_PER_MILLI;
    let days = (millis / MILLIS_PER_DAY) as i64;
    let of_day = millis % MILLIS_PER_DAY;
    let (year, month, day) = civil_from_days(days - DAYS_1601_TO_1970);
    Some(SystemTime {
        year: year as u16,
        month,
        // 1601-01-01 was a Monday, and Sunday is zero.
        day_of_week: ((days + 1) % 7) as u16,
        day,
        hour: (of_day / 3_600_000) as u16,
        minute: (of_day / 60_000 % 60) as u16,
        second: (of_day / 1000 % 60) as u16,
        milliseconds: (of_day % 1000) as u16,
    })
}

/// The day of week is ignored, as documented for SystemTimeToFileTime.
fn system_time_to_filetime(time: SystemTime) -> Option<u64> {
    let valid = (1601..=30827).contains(&time.year)
        && (1..=12).contains(&time.month)
        && (1..=days_in_month(time.year, time.month)).contains(&time.day)
        && time.hour < 24
        && time.minute < 60
        && time.second < 60
        && time.milliseconds < 1000;
    if !valid {
        return None;
    }
    let days = days_from_civil(i64::from(time.year), time.month, time.day) + DAYS_1601_TO_1970;
    let millis = days as u64 * MILLIS_PER_DAY
        + u64::from(time.hour) * 3_600_000
        + u64::from(time.minute) * 60_000
        + u64::from(time.second) * 1000
        + u64::from(time.milliseconds);
    Some(millis * TICKS_PER_MILLI)
}

fn fault(error: MemoryError) -> ExitReason {
    ExitReason::MemoryFault(error)
}

fn write_system_info(
    state: &ServicesState,
    memory: &mut GuestMemory,
    address: u64,
) -> Result<(), ExitReason> {
    let processors = state.guest_profile.processors.clamp(1, 64);
    let mask = u64::MAX >> (64 - processors);
    let mut info = [0u8; 48];
    info[0..2].copy_from_slice(&PROCESSOR_ARCHITECTURE_AMD64.to_le_bytes());
    info[4..8].copy_from_slice(&GUEST_PAGE_SIZE.to_le_bytes());
    info[8..16].copy_from_slice(&MINIMUM_APPLICATION_ADDRESS.to_le_bytes());
    info[16..24].copy_from_slice(&MAXIMUM_APPLICATION_ADDRESS.to_le_bytes());
    info[24..32].copy_from_slice(&mask.to_le_bytes());
    info[32..36].copy_from_slice(&processors.to_le_bytes());
    info[36..40].copy_from_slice(&PROCESSOR_AMD_X8664.to_le_bytes());
    info[40..44].copy_from_slice(&(ALLOCATION_GRANULARITY as u32).to_le_bytes());
    // wProcessorLevel and wProcessorRevision stay zero: no CPU model is simulated.
    memory.write_bytes(address, &info).map_err(fault)
}

pub(super) fn register(registry: &mut ImportRegistry, state: &Arc<ServicesState>) {
    for symbol in ["GetSystemInfo", "GetNativeSystemInfo"] {
        let s = Arc::clone(state);
        registry.register(KERNEL32, symbol, move |args, _, mem| {
            write_system_info(&s, mem, args.rcx)?;
            Ok(0)
        });
    }

    let s = Arc::clone(state);
    registry.register(KERNEL32, "GlobalMemoryStatusEx", move |args, cpu, mem| {
        let address = args.rcx;
        if mem.read_u32(address).map_err(fault)? != MEMORYSTATUSEX_SIZE {
            return s.fail(cpu.gs_base, ERROR_INVALID_PARAMETER, 0);
        }
        let (mapped, virtual_bytes, heap_bytes) = mem.usage();
        // Guest physical memory is the virtual-allocation and heap budgets;
        // there is no paging file.
        let total = 2 * DYNAMIC_BUDGET;
        let used = (virtual_bytes + heap_bytes).min(total);
        let total_virtual = MAXIMUM_APPLICATION_ADDRESS - MINIMUM_APPLICATION_ADDRESS + 1;
        let mut status = [0u8; 64];
        status[0..4].copy_from_slice(&MEMORYSTATUSEX_SIZE.to_le_bytes());
        status[4..8].copy_from_slice(&((used * 100 / total) as u32).to_le_bytes());
        for (offset, value) in [
            (8, total),
            (16, total - used),
            (24, total),
            (32, total - used),
            (40, total_virtual),
            (48, total_virtual.saturating_sub(mapped)),
        ] {
            status[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        }
        mem.write_bytes(address, &status).map_err(fault)?;
        Ok(1)
    });

    // The guest time zone is UTC, so local time equals system time.
    for symbol in ["GetSystemTime", "GetLocalTime"] {
        let s = Arc::clone(state);
        registry.register(KERNEL32, symbol, move |args, _, mem| {
            let time = filetime_to_system_time(s.clock.now_filetime())
                .ok_or(ExitReason::IllegalInstruction(0xFE))?;
            mem.write_bytes(args.rcx, &time.to_bytes()).map_err(fault)?;
            Ok(0)
        });
    }

    let s = Arc::clone(state);
    registry.register(
        KERNEL32,
        "GetSystemTimePreciseAsFileTime",
        move |args, _, mem| {
            mem.write_u64(args.rcx, s.clock.now_filetime())
                .map_err(fault)?;
            Ok(0)
        },
    );

    let s = Arc::clone(state);
    registry.register(KERNEL32, "FileTimeToSystemTime", move |args, cpu, mem| {
        let filetime = mem.read_u64(args.rcx).map_err(fault)?;
        let Some(time) = filetime_to_system_time(filetime) else {
            return s.fail(cpu.gs_base, ERROR_INVALID_PARAMETER, 0);
        };
        mem.write_bytes(args.rdx, &time.to_bytes()).map_err(fault)?;
        Ok(1)
    });

    let s = Arc::clone(state);
    registry.register(KERNEL32, "SystemTimeToFileTime", move |args, cpu, mem| {
        let mut bytes = [0; 16];
        mem.read_bytes(args.rcx, &mut bytes).map_err(fault)?;
        let Some(filetime) = system_time_to_filetime(SystemTime::from_bytes(bytes)) else {
            return s.fail(cpu.gs_base, ERROR_INVALID_PARAMETER, 0);
        };
        mem.write_u64(args.rdx, filetime).map_err(fault)?;
        Ok(1)
    });

    let s = Arc::clone(state);
    registry.register(
        KERNEL32,
        "QueryUnbiasedInterruptTime",
        move |args, _, mem| {
            let elapsed = s.clock.performance_count().saturating_sub(s.clock_origin);
            mem.write_u64(args.rcx, elapsed).map_err(fault)?;
            Ok(1)
        },
    );

    // No APCs can be queued, so an alertable SleepEx always runs to completion
    // and returns zero rather than WAIT_IO_COMPLETION.
    for symbol in ["Sleep", "SleepEx"] {
        let s = Arc::clone(state);
        registry.register(KERNEL32, symbol, move |args, _, _| {
            let millis = args.rcx as u32;
            if millis == INFINITE || millis > MAX_SLEEP_MILLIS {
                return Err(ExitReason::IllegalInstruction(0xFE));
            }
            s.clock.sleep(u64::from(millis) * TICKS_PER_MILLI);
            Ok(0)
        });
    }

    for (symbol, api_set) in [
        ("GetSystemInfo", SYSINFO_API),
        ("GetNativeSystemInfo", SYSINFO_API_2),
        ("GlobalMemoryStatusEx", SYSINFO_API),
        ("GetSystemTime", SYSINFO_API),
        ("GetLocalTime", SYSINFO_API),
        ("GetSystemTimePreciseAsFileTime", SYSINFO_API_2),
        ("FileTimeToSystemTime", TIMEZONE_API),
        ("SystemTimeToFileTime", TIMEZONE_API),
        ("QueryUnbiasedInterruptTime", REALTIME_API),
        ("Sleep", SYNCH_API_2),
        ("SleepEx", SYNCH_API),
    ] {
        registry.alias(api_set, KERNEL32, symbol);
        registry.set_status(KERNEL32, symbol, ServiceStatus::Partial);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_conversion_round_trips_across_leap_and_century_boundaries() {
        for days in (-DAYS_1601_TO_1970..2_000_000).step_by(997) {
            let (year, month, day) = civil_from_days(days);
            assert_eq!(days_from_civil(year, month, day), days);
        }
        assert!(is_leap(2000) && is_leap(2024) && !is_leap(1900) && !is_leap(2100));
    }

    #[test]
    fn host_capabilities_are_detected_on_this_mac() {
        let host = HostCapabilities::detect();
        assert!(host.logical_processors.unwrap() >= 1);
        assert!(host.page_size.unwrap() >= 4096);
        #[cfg(target_os = "macos")]
        assert!(host.physical_memory.unwrap() >= 1 << 30);
    }
}
