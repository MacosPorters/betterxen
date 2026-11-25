// SPDX-License-Identifier: GPL-3.0-or-later
//! Trusted experiment-only C ABI; never accepts an untrusted host pointer.
use xenora_loader::cpu::{CpuState, ExitReason, Interpreter, Register};
use xenora_loader::dbt::DbtEngine;
use xenora_loader::memory::{GuestMemory, MemoryPermissions};

#[repr(C)]
pub struct ResultState {
    rax: u64,
    rsp: u64,
    rip: u64,
    flags: u64,
    data: u64,
    data_hi: u64,
    read_only: u64,
    exit: u64,
}

#[unsafe(no_mangle)]
pub extern "C" fn bx_memory_new() -> *mut GuestMemory {
    let mut memory = GuestMemory::new();
    memory
        .map_region(0x2000, 16, MemoryPermissions::READ_WRITE)
        .unwrap();
    memory
        .map_region_with_data(
            0x3000,
            16,
            MemoryPermissions::READ_WRITE,
            &0x4000u64.to_le_bytes(),
        )
        .unwrap();
    memory
        .map_region_with_data(0x6000, 16, MemoryPermissions::READ_ONLY, &[0xa5; 16])
        .unwrap();
    Box::into_raw(Box::new(memory))
}

/// # Safety
/// `memory` must be a live pointer returned by bx_memory_new, freed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bx_memory_free(memory: *mut GuestMemory) {
    unsafe {
        drop(Box::from_raw(memory));
    }
}

/// # Safety
/// Pointers must reference a live memory object and an eight-byte output cell.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bx_read(memory: *const GuestMemory, address: u64, value: *mut u64) -> u8 {
    match unsafe { &*memory }.read_u64(address) {
        Ok(result) => {
            unsafe {
                *value = result;
            }
            1
        }
        Err(_) => 0,
    }
}

/// # Safety
/// `memory` must exclusively reference a live bx_memory_new allocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bx_write(memory: *mut GuestMemory, address: u64, value: u64) -> u8 {
    u8::from(unsafe { &mut *memory }.write_u64(address, value).is_ok())
}

/// # Safety
/// The byte span and output pointer must be valid for this call. Engine is 0 or 1.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bx_run(
    engine: u32,
    code: *const u8,
    len: usize,
    a: u64,
    b: u64,
    address: u64,
    output: *mut ResultState,
) -> u8 {
    let code = unsafe { std::slice::from_raw_parts(code, len) };
    let raw = bx_memory_new();
    let memory = unsafe { &mut *raw };
    memory
        .map_region_with_data(0x1000, code.len(), MemoryPermissions::READ_EXECUTE, code)
        .unwrap();
    memory
        .map_region_with_data(0x4000, 1, MemoryPermissions::READ_EXECUTE, &[0xf4])
        .unwrap();
    let mut cpu = CpuState::new(0x1000, 0x3000);
    cpu.set(Register::Rax, a);
    cpu.set(Register::Rcx, b);
    cpu.set(Register::Rbx, address);
    let result = match engine {
        0 => Interpreter::new(cpu, memory, 32).run(),
        1 => match DbtEngine::new(1024 * 1024) {
            Ok(mut dbt) => dbt.run(&mut cpu, memory, 32),
            Err(_) => {
                unsafe {
                    bx_memory_free(raw);
                }
                return 0;
            }
        },
        _ => {
            unsafe {
                bx_memory_free(raw);
            }
            return 0;
        }
    };
    let state = result.final_state;
    let flags = state.flags;
    let exit = match result.exit_reason {
        ExitReason::Halt => 0,
        ExitReason::MemoryFault(_) => 1,
        ExitReason::StepLimitExceeded => 4,
        _ => 2,
    };
    unsafe {
        *output = ResultState {
            rax: state.get(Register::Rax),
            rsp: state.get(Register::Rsp),
            rip: state.rip,
            flags: u64::from(flags.cf)
                | u64::from(flags.pf) << 2
                | u64::from(flags.af) << 4
                | u64::from(flags.zf) << 6
                | u64::from(flags.sf) << 7
                | u64::from(flags.of) << 11,
            data: memory.read_u64(0x2000).unwrap(),
            data_hi: memory.read_u64(0x2008).unwrap(),
            read_only: memory.read_u64(0x6000).unwrap(),
            exit,
        };
        bx_memory_free(raw);
    }
    1
}

#[repr(C)]
pub struct SimdResult {
    xmm0: [u64; 2],
    xmm1: [u64; 2],
    mxcsr: u64,
    exit: u64,
}

/// # Safety
/// All pointers refer to trusted harness storage with the indicated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bx_simd_run(
    engine: u32,
    code: *const u8,
    len: usize,
    inputs: *const u64,
    mxcsr: u32,
    output: *mut SimdResult,
) -> u8 {
    let code = unsafe { std::slice::from_raw_parts(code, len) };
    let inputs = unsafe { std::slice::from_raw_parts(inputs, 4) };
    let raw = bx_memory_new();
    let memory = unsafe { &mut *raw };
    memory
        .map_region_with_data(0x1000, code.len(), MemoryPermissions::READ_EXECUTE, code)
        .unwrap();
    memory
        .map_region_with_data(0x4000, 1, MemoryPermissions::READ_EXECUTE, &[0xf4])
        .unwrap();
    let mut cpu = CpuState::new(0x1000, 0x3000);
    cpu.xmm[0] = u128::from(inputs[0]) | u128::from(inputs[1]) << 64;
    cpu.xmm[1] = u128::from(inputs[2]) | u128::from(inputs[3]) << 64;
    cpu.mxcsr.0 = mxcsr;
    let result = match engine {
        0 => Interpreter::new(cpu, memory, 32).run(),
        1 => match DbtEngine::new(1024 * 1024) {
            Ok(mut dbt) => dbt.run(&mut cpu, memory, 32),
            Err(_) => {
                unsafe {
                    bx_memory_free(raw);
                }
                return 0;
            }
        },
        _ => {
            unsafe {
                bx_memory_free(raw);
            }
            return 0;
        }
    };
    let state = result.final_state;
    unsafe {
        *output = SimdResult {
            xmm0: [state.xmm[0] as u64, (state.xmm[0] >> 64) as u64],
            xmm1: [state.xmm[1] as u64, (state.xmm[1] >> 64) as u64],
            mxcsr: u64::from(state.mxcsr.0),
            exit: match result.exit_reason {
                ExitReason::Halt => 0,
                ExitReason::MemoryFault(_) => 1,
                ExitReason::SimdException => 3,
                ExitReason::IllegalInstruction(_) => 5,
                _ => 2,
            },
        };
        bx_memory_free(raw);
    }
    1
}

pub struct Bench {
    memory: GuestMemory,
    dbt: Option<DbtEngine>,
    engine: u32,
}

#[repr(C)]
#[derive(Default)]
pub struct BenchResult {
    ns: u64,
    compiled: u64,
    cache_hits: u64,
    fallbacks: u64,
    native_bytes: u64,
}

/// # Safety
/// The code span is valid trusted straight-line fixture code, ending with RET.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bx_bench_new(engine: u32, code: *const u8, len: usize) -> *mut Bench {
    if engine > 2 {
        return std::ptr::null_mut();
    }
    let mut memory = unsafe { *Box::from_raw(bx_memory_new()) };
    let code = unsafe { std::slice::from_raw_parts(code, len) };
    memory
        .map_region_with_data(0x1000, len, MemoryPermissions::READ_EXECUTE, code)
        .unwrap();
    memory
        .map_region_with_data(0x4000, 1, MemoryPermissions::READ_EXECUTE, &[0xf4])
        .unwrap();
    let dbt = if engine == 1 {
        match DbtEngine::new(1024 * 1024) {
            Ok(dbt) => Some(dbt),
            Err(_) => return std::ptr::null_mut(),
        }
    } else {
        None
    };
    Box::into_raw(Box::new(Bench {
        memory,
        dbt,
        engine,
    }))
}

/// # Safety
/// Bench is a live exclusive allocation returned by bx_bench_new; output is valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bx_bench_run(
    bench: *mut Bench,
    repetitions: u32,
    output: *mut BenchResult,
) -> u8 {
    let bench = unsafe { &mut *bench };
    if repetitions == 0 || repetitions > 10_000 {
        return 0;
    }
    let mut stats = BenchResult::default();
    let before = bench
        .dbt
        .as_ref()
        .map(|dbt| dbt.metrics)
        .unwrap_or_default();
    let start = std::time::Instant::now();
    for _ in 0..repetitions {
        bench.memory.write_u64(0x2000, 0).unwrap();
        let mut cpu = CpuState::new(0x1000, 0x3000);
        cpu.set(Register::Rax, 1);
        cpu.set(Register::Rcx, 1);
        cpu.set(Register::Rbx, 0x2000);
        let result = if let Some(dbt) = &mut bench.dbt {
            dbt.run(&mut cpu, &mut bench.memory, 1024)
        } else if bench.engine == 0 {
            Interpreter::new(cpu, &mut bench.memory, 1024).run()
        } else {
            let Ok(mut dbt) = DbtEngine::new(1024 * 1024) else {
                return 0;
            };
            let result = dbt.run(&mut cpu, &mut bench.memory, 1024);
            stats.compiled += dbt.metrics.blocks_compiled as u64;
            stats.cache_hits += dbt.metrics.cache_hits as u64;
            stats.fallbacks += dbt.metrics.interpreter_fallbacks as u64;
            stats.native_bytes += dbt.metrics.native_bytes_emitted as u64;
            result
        };
        let state = result.final_state;
        if result.exit_reason != ExitReason::Halt
            || state.get(Register::Rax) != 257
            || state.get(Register::Rsp) != 0x3008
            || state.flags != Default::default()
            || bench.memory.read_u64(0x2000).unwrap() != 257
            || bench.memory.read_u64(0x2008).unwrap() != 0
            || bench.memory.read_u64(0x6000).unwrap() != 0xa5a5_a5a5_a5a5_a5a5
        {
            return 0;
        }
    }
    stats.ns = start.elapsed().as_nanos() as u64;
    if let Some(dbt) = &bench.dbt {
        stats.compiled = (dbt.metrics.blocks_compiled - before.blocks_compiled) as u64;
        stats.cache_hits = (dbt.metrics.cache_hits - before.cache_hits) as u64;
        stats.fallbacks = (dbt.metrics.interpreter_fallbacks - before.interpreter_fallbacks) as u64;
        stats.native_bytes =
            (dbt.metrics.native_bytes_emitted - before.native_bytes_emitted) as u64;
    }
    unsafe {
        *output = stats;
    }
    1
}

/// # Safety
/// Bench is a live allocation returned by bx_bench_new and is freed exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bx_bench_free(bench: *mut Bench) {
    unsafe {
        drop(Box::from_raw(bench));
    }
}
