// SPDX-License-Identifier: GPL-3.0-or-later

//! CRT calendar time against the injectable guest clock.
//!
//! `time` / `_time64` report Unix seconds derived from the same FILETIME clock
//! as `GetSystemTimeAsFileTime`. `gmtime_s` / `localtime_s` and their explicit
//! 64-bit forms fill a Windows `tm` in UTC; there is no guest timezone
//! database, so local time matches UTC and `tm_isdst` is always zero.
//!
//! On 64-bit Windows `time_t` is 64 bits unless a program defines
//! `_USE_32BIT_TIME_T` (Microsoft Learn, "time, _time32, _time64"), so the
//! unsuffixed names here read and write 64-bit values, like the `64` forms.

use super::ServicesState;
use crate::cpu::ExitReason;
use crate::imports::{ImportRegistry, ServiceStatus};
use crate::memory::{GuestMemory, MemoryError};
use std::sync::Arc;

const UCRT: &str = "ucrtbase.dll";
const CRT_TIME_API: &str = "api-ms-win-crt-time-l1-1-0.dll";
const EINVAL: u32 = 22;
/// 3000-12-31 23:59:59 UTC, the last time Microsoft Learn says
/// `_gmtime64_s` can express ("gmtime_s, _gmtime32_s, _gmtime64_s").
///
/// The `localtime_s` page rejects values "greater than `_MAX__TIME64_T`"
/// without giving the number, and its prose names 23:59:59 January 18, 3001
/// instead. The two pages disagree. Guest local time is UTC here, so both
/// conversions use the one bound the `gmtime_s` page states explicitly.
const MAX_TIME64: i64 = 32_535_215_999;
const FILETIME_TICKS_PER_SEC: u64 = 10_000_000;
/// Seconds from 1601-01-01 to 1970-01-01.
const SECS_1601_TO_1970: u64 = 11_644_473_600;

fn fault(error: MemoryError) -> ExitReason {
    ExitReason::MemoryFault(error)
}

fn set_errno(memory: &mut GuestMemory, teb: u64, error: u32) -> Result<(), ExitReason> {
    let address = memory.crt_errno_address(teb).map_err(fault)?;
    memory.write_u32(address, error).map_err(fault)
}

fn unix_from_filetime(filetime: u64) -> Option<i64> {
    let secs = filetime / FILETIME_TICKS_PER_SEC;
    if secs < SECS_1601_TO_1970 {
        return Some(0);
    }
    i64::try_from(secs - SECS_1601_TO_1970).ok()
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

fn day_of_year(year: i64, month: u16, day: u16) -> i32 {
    const CUMULATIVE: [i32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let extra = i32::from(leap && month > 2);
    CUMULATIVE[month as usize - 1] + i32::from(day) - 1 + extra
}

/// Windows `struct tm` field order (nine 32-bit ints).
fn write_tm(memory: &mut GuestMemory, dest: u64, unix: i64) -> Result<(), ExitReason> {
    let days = unix.div_euclid(86_400);
    let of_day = unix.rem_euclid(86_400) as i32;
    let (year, month, day) = civil_from_days(days);
    let fields = [
        of_day % 60,                       // tm_sec
        (of_day / 60) % 60,                // tm_min
        of_day / 3600,                     // tm_hour
        i32::from(day),                    // tm_mday
        i32::from(month) - 1,              // tm_mon
        (year - 1900) as i32,              // tm_year
        ((days + 4).rem_euclid(7)) as i32, // tm_wday (1970-01-01 was Thursday)
        day_of_year(year, month, day),     // tm_yday
        0,                                 // tm_isdst
    ];
    for (index, field) in fields.into_iter().enumerate() {
        memory
            .write_u32(dest + index as u64 * 4, field as u32)
            .map_err(fault)?;
    }
    Ok(())
}

fn store_time(memory: &mut GuestMemory, dest: u64, value: i64) -> Result<(), ExitReason> {
    if dest == 0 {
        return Ok(());
    }
    memory.write_u64(dest, value as u64).map_err(fault)
}

pub(super) fn register(registry: &mut ImportRegistry, state: &Arc<ServicesState>) {
    let time64 = Arc::clone(state);
    registry.register(UCRT, "_time64", move |args, _, memory| {
        let now = unix_from_filetime(time64.clock.now_filetime()).unwrap_or(0);
        store_time(memory, args.rcx, now)?;
        Ok(now as u64)
    });

    let time = Arc::clone(state);
    registry.register(UCRT, "time", move |args, _, memory| {
        let now = unix_from_filetime(time.clock.now_filetime()).unwrap_or(0);
        store_time(memory, args.rcx, now)?;
        Ok(now as u64)
    });

    for symbol in ["gmtime_s", "localtime_s", "_gmtime64_s", "_localtime64_s"] {
        let clock = Arc::clone(state);
        registry.register(UCRT, symbol, move |args, cpu, memory| {
            // Error rows from Microsoft Learn: a null `tm` is left alone; a
            // null time pointer or a time outside 1970-01-01 to MAX_TIME64
            // sets every `tm` field to -1. All return EINVAL. The pages
            // document errno only for the null rows; it is set for the range
            // row too, which is this project's choice. The UCRT
            // invalid-parameter handler is not modelled.
            let (tm, time_ptr) = (args.rcx, args.rdx);
            if tm == 0 {
                set_errno(memory, cpu.gs_base, EINVAL)?;
                return Ok(u64::from(EINVAL));
            }
            let unix = if time_ptr == 0 {
                None
            } else {
                Some(memory.read_u64(time_ptr).map_err(fault)? as i64)
            };
            let _ = clock; // timezone-free: local == UTC
            match unix {
                Some(unix) if (0..=MAX_TIME64).contains(&unix) => {
                    write_tm(memory, tm, unix)?;
                    Ok(0)
                }
                _ => {
                    for index in 0..9 {
                        memory.write_u32(tm + index * 4, u32::MAX).map_err(fault)?;
                    }
                    set_errno(memory, cpu.gs_base, EINVAL)?;
                    Ok(u64::from(EINVAL))
                }
            }
        });
    }

    for symbol in [
        "time",
        "_time64",
        "gmtime_s",
        "localtime_s",
        "_gmtime64_s",
        "_localtime64_s",
    ] {
        registry.alias(CRT_TIME_API, UCRT, symbol);
        registry.set_status(UCRT, symbol, ServiceStatus::Partial);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::{CpuState, Register};
    use crate::import_table::ImportName;
    use crate::imports::Win64Args;
    use crate::memory::PROCESS_HEAP;
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
    fn time64_and_gmtime_s_match_unix_epoch() {
        // 1970-01-01 00:00:00 UTC as FILETIME.
        let filetime = SECS_1601_TO_1970 * FILETIME_TICKS_PER_SEC;
        let state = Arc::new(ServicesState::new(Arc::new(MockClock::new(filetime))));
        let mut registry = ImportRegistry::new();
        register(&mut registry, &state);
        let mut memory = GuestMemory::new();
        let mut cpu = CpuState {
            gs_base: 0x10000,
            ..CpuState::default()
        };
        let slot = memory.map_heap_allocation(PROCESS_HEAP, 8).unwrap();
        assert_eq!(
            call(&registry, "_time64", [slot, 0, 0, 0], &mut cpu, &mut memory),
            0
        );
        assert_eq!(memory.read_u64(slot).unwrap(), 0);
        let tm = memory.map_heap_allocation(PROCESS_HEAP, 36).unwrap();
        assert_eq!(
            call(
                &registry,
                "_gmtime64_s",
                [tm, slot, 0, 0],
                &mut cpu,
                &mut memory
            ),
            0
        );
        assert_eq!(memory.read_u32(tm).unwrap(), 0); // sec
        assert_eq!(memory.read_u32(tm + 12).unwrap(), 1); // mday
        assert_eq!(memory.read_u32(tm + 16).unwrap(), 0); // mon
        assert_eq!(memory.read_u32(tm + 20).unwrap(), 70); // year
        assert_eq!(memory.read_u32(tm + 24).unwrap(), 4); // Thursday
    }

    #[test]
    fn gmtime_s_and_localtime_s_reject_times_outside_the_documented_range() {
        // Microsoft Learn, "gmtime_s, _gmtime32_s, _gmtime64_s": dates from
        // midnight 1970-01-01 up through 23:59:59, December 31, 3000, UTC;
        // a negative time, or a null time pointer, returns EINVAL with every
        // `tm` field set to -1, and a null `tm` is not modified. The bound,
        // 32535215999, was checked with `date -u -r` (Wed Dec 31 23:59:59
        // UTC 3000) and `cal 12 3000`, not with this module.
        let state = Arc::new(ServicesState::new(Arc::new(MockClock::new(0))));
        let mut registry = ImportRegistry::new();
        register(&mut registry, &state);
        let mut memory = GuestMemory::new();
        let mut cpu = CpuState {
            gs_base: 0x10000,
            ..CpuState::default()
        };
        let errno = memory.crt_errno_address(cpu.gs_base).unwrap();
        let slot = memory.map_heap_allocation(PROCESS_HEAP, 8).unwrap();
        let tm = memory.map_heap_allocation(PROCESS_HEAP, 36).unwrap();
        let fields = |memory: &GuestMemory| -> Vec<i32> {
            (0..9)
                .map(|i| memory.read_u32(tm + i * 4).unwrap() as i32)
                .collect()
        };
        for symbol in ["gmtime_s", "localtime_s", "_gmtime64_s", "_localtime64_s"] {
            // The last representable second.
            memory.write_u64(slot, 32_535_215_999).unwrap();
            assert_eq!(
                call(&registry, symbol, [tm, slot, 0, 0], &mut cpu, &mut memory),
                0,
                "{symbol}"
            );
            // sec, min, hour, mday, mon (December), year - 1900, wday
            // (Wednesday), yday (3000 is not a leap year), isdst.
            assert_eq!(
                fields(&memory),
                [59, 59, 23, 31, 11, 1100, 3, 364, 0],
                "{symbol}"
            );

            // One second later, a negative time, and i64::MAX are rejected.
            for bad in [32_535_216_000_i64, -1, i64::MIN, i64::MAX] {
                memory.write_u32(errno, 0).unwrap();
                memory.write_u64(slot, bad as u64).unwrap();
                assert_eq!(
                    call(&registry, symbol, [tm, slot, 0, 0], &mut cpu, &mut memory),
                    u64::from(EINVAL),
                    "{symbol} {bad}"
                );
                assert_eq!(fields(&memory), [-1; 9], "{symbol} {bad}");
                assert_eq!(memory.read_u32(errno).unwrap(), EINVAL, "{symbol} {bad}");
            }

            // A null time pointer also fills `tm` with -1.
            memory.write_u64(slot, 0).unwrap();
            call(&registry, symbol, [tm, slot, 0, 0], &mut cpu, &mut memory);
            assert_eq!(
                call(&registry, symbol, [tm, 0, 0, 0], &mut cpu, &mut memory),
                u64::from(EINVAL),
                "{symbol}"
            );
            assert_eq!(fields(&memory), [-1; 9], "{symbol}");

            // A null `tm` returns EINVAL and writes nothing.
            memory.write_u64(slot, 0).unwrap();
            call(&registry, symbol, [tm, slot, 0, 0], &mut cpu, &mut memory);
            let before = fields(&memory);
            assert_eq!(
                call(&registry, symbol, [0, slot, 0, 0], &mut cpu, &mut memory),
                u64::from(EINVAL),
                "{symbol}"
            );
            assert_eq!(fields(&memory), before, "{symbol}");
        }
    }

    #[test]
    fn unsuffixed_time_names_use_a_64_bit_time_t() {
        // 2040-01-01 00:00:00 UTC is past the 32-bit range.
        let unix: u64 = 2_208_988_800;
        let filetime = (SECS_1601_TO_1970 + unix) * FILETIME_TICKS_PER_SEC;
        let state = Arc::new(ServicesState::new(Arc::new(MockClock::new(filetime))));
        let mut registry = ImportRegistry::new();
        register(&mut registry, &state);
        let mut memory = GuestMemory::new();
        let mut cpu = CpuState {
            gs_base: 0x10000,
            ..CpuState::default()
        };
        let slot = memory.map_heap_allocation(PROCESS_HEAP, 8).unwrap();
        memory.write_u64(slot, u64::MAX).unwrap();
        let returned = call(&registry, "time", [slot, 0, 0, 0], &mut cpu, &mut memory);
        assert_eq!(returned, unix);
        assert_eq!(memory.read_u64(slot).unwrap(), unix);
        let tm = memory.map_heap_allocation(PROCESS_HEAP, 36).unwrap();
        for symbol in ["gmtime_s", "localtime_s"] {
            assert_eq!(
                call(&registry, symbol, [tm, slot, 0, 0], &mut cpu, &mut memory),
                0
            );
            assert_eq!(memory.read_u32(tm + 12).unwrap(), 1); // mday
            assert_eq!(memory.read_u32(tm + 16).unwrap(), 0); // January
            assert_eq!(memory.read_u32(tm + 20).unwrap(), 140); // 2040 - 1900
        }
    }
}
