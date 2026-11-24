// SPDX-License-Identifier: GPL-3.0-or-later

//! XADD, CMPXCHG and the LOCK prefix (issue #198).
//!
//! Expected values follow the operation sections of the Intel SDM Vol. 2 for
//! XADD, CMPXCHG and LOCK, stated here as literal results. Encodings were
//! assembled with `clang -target x86_64-apple-macos` and read with `objdump`.
//! Nothing was compared with x86 hardware.
//!
//! Both engines run one guest instruction at a time on one host thread, so
//! these tests establish the single-thread results that any later guest
//! scheduler has to preserve. They say nothing about concurrent execution.

use xenora_loader::cpu::{CpuState, ExitReason, Interpreter, RFlags};
use xenora_loader::dbt::{DbtEngine, is_native};
use xenora_loader::memory::{GuestMemory, MemoryPermissions};
use xenora_loader::x86_decode::decode;

const ENTRY: u64 = 0x1000;
/// Two writable pages. Nothing is mapped after them unless a test says so.
const DATA: u64 = 0x4000;
const PAGE: u64 = 0x1000;
const END: u64 = DATA + 2 * PAGE;

const RAX: usize = 0;
const RCX: usize = 1;
const RDX: usize = 2;
const RBX: usize = 3;
const RSI: usize = 6;

fn image(code: &[u8]) -> GuestMemory {
    let mut memory = GuestMemory::new();
    memory
        .map_region_with_data(ENTRY, code.len(), MemoryPermissions::READ_EXECUTE, code)
        .unwrap();
    memory
        .map_region(DATA, 2 * PAGE as usize, MemoryPermissions::READ_WRITE)
        .unwrap();
    memory
}

fn data(memory: &GuestMemory) -> Vec<u8> {
    let mut out = vec![0; 2 * PAGE as usize];
    memory.read_bytes(DATA, &mut out).unwrap();
    out
}

struct Run {
    name: &'static str,
    cpu: CpuState,
    exit: ExitReason,
    memory: GuestMemory,
    /// Instructions the DBT ran on the shared semantics.
    helpers: usize,
}

/// The interpreter and the DBT, each from its own copy of `cpu` and `memory`.
fn both(cpu: &CpuState, memory: &GuestMemory, budget: usize) -> [Run; 2] {
    let mut interpreted = memory.clone();
    let result = Interpreter::new(cpu.clone(), &mut interpreted, budget).run();
    let mut translated = memory.clone();
    let mut engine = DbtEngine::new(64 * 1024).unwrap();
    let mut state = cpu.clone();
    let native = engine.run(&mut state, &mut translated, budget);
    [
        Run {
            name: "interpreter",
            cpu: result.final_state,
            exit: result.exit_reason,
            memory: interpreted,
            helpers: 0,
        },
        Run {
            name: "DBT",
            cpu: native.final_state,
            exit: native.exit_reason,
            memory: translated,
            helpers: engine.metrics.interpreter_fallbacks,
        },
    ]
}

fn flags(cf: bool, zf: bool, sf: bool, of: bool, pf: bool, af: bool) -> RFlags {
    RFlags {
        cf,
        zf,
        sf,
        of,
        pf,
        af,
    }
}

fn low(bits: u32) -> u64 {
    u64::MAX >> (64 - bits)
}

#[test]
fn lock_is_accepted_only_on_xadd_and_cmpxchg_with_a_memory_destination() {
    let accepted: &[&[u8]] = &[
        &[0xF0, 0x0F, 0xC1, 0x0E],       // lock xadd [rsi], ecx
        &[0xF0, 0x0F, 0xB1, 0x0E],       // lock cmpxchg [rsi], ecx
        &[0x66, 0xF0, 0x0F, 0xC1, 0x0E], // lock xadd [rsi], cx, in LLVM's prefix order
        &[0xF0, 0x66, 0x0F, 0xC1, 0x0E], // the same with LOCK first
        &[0x65, 0xF0, 0x0F, 0xB0, 0x0E], // lock cmpxchg gs:[rsi], cl
        &[0xF0, 0x48, 0x0F, 0xC1, 0x0E], // lock xadd [rsi], rcx
    ];
    for code in accepted {
        let lock = code.iter().position(|byte| *byte == 0xF0).unwrap();
        let mut unlocked = code.to_vec();
        unlocked.remove(lock);
        let insn = decode(ENTRY, |at| {
            code.get((at - ENTRY) as usize).copied().ok_or(())
        })
        .unwrap();
        assert!(insn.lock && !is_native(&insn), "{code:02X?}");
        assert_eq!(usize::from(insn.len), code.len(), "{code:02X?}");

        // The accumulator matches the operand, so each exchange stores RCX.
        let mut cpu = CpuState::new(ENTRY, 0);
        cpu.gpr[RAX] = 0x2122_2324_2526_2728;
        cpu.gpr[RCX] = 0x1112_1314_1516_1718;
        cpu.gpr[RSI] = DATA + 0x40;
        let mut memory = image(code);
        memory
            .write_u64(DATA + 0x40, 0x2122_2324_2526_2728)
            .unwrap();
        let mut plain = image(&unlocked);
        plain.write_u64(DATA + 0x40, 0x2122_2324_2526_2728).unwrap();

        // LOCK changes no result here: only the instruction is a byte longer.
        for (locked, wanted) in both(&cpu, &memory, 1).iter().zip(both(&cpu, &plain, 1)) {
            assert_eq!(locked.exit, ExitReason::StepLimitExceeded, "{code:02X?}");
            assert_eq!(locked.cpu.rip, ENTRY + code.len() as u64, "{code:02X?}");
            let mut shifted = wanted.cpu.clone();
            shifted.rip += 1;
            assert_eq!(locked.cpu, shifted, "{code:02X?} on the {}", locked.name);
            assert_eq!(data(&locked.memory), data(&wanted.memory), "{code:02X?}");
            assert_ne!(data(&locked.memory), data(&memory), "{code:02X?}");
        }
    }

    let rejected: &[&[u8]] = &[
        &[0xF0, 0x0F, 0xC1, 0xC8], // lock xadd eax, ecx: register destination
        &[0xF0, 0x0F, 0xB1, 0xCB], // lock cmpxchg ebx, ecx: register destination
        &[0xF0, 0x01, 0x0E],       // lock add [rsi], ecx: outside the locked subset
        &[0xF0, 0x87, 0x0E],       // lock xchg [rsi], ecx: outside the locked subset
        &[0xF0, 0x0F, 0xC7, 0x0E], // lock cmpxchg8b [rsi]: not implemented
        &[0xF0, 0x8B, 0x06],       // lock mov eax, [rsi]: never lockable
        &[0xF0, 0x90],             // lock nop
    ];
    for code in rejected {
        let mut padded = code.to_vec();
        padded.extend([0x90; 8]);
        let mut cpu = CpuState::new(ENTRY, 0);
        cpu.gpr[RSI] = DATA;
        let memory = image(&padded);
        for run in both(&cpu, &memory, 1) {
            assert!(
                matches!(run.exit, ExitReason::IllegalInstruction(_)),
                "{code:02X?} on the {}: {:?}",
                run.name,
                run.exit
            );
            assert_eq!(run.cpu, cpu, "{code:02X?}");
            assert_eq!(data(&run.memory), data(&memory), "{code:02X?}");
        }
    }
}

#[test]
fn compare_and_exchange_takes_each_branch_at_every_width() {
    // Destination [rsi] or RBX, source RCX, accumulator RAX.
    let forms: &[(&[u8], u32, bool)] = &[
        (&[0xF0, 0x0F, 0xB0, 0x0E], 8, true), // lock cmpxchg [rsi], cl
        (&[0x66, 0xF0, 0x0F, 0xB1, 0x0E], 16, true), // lock cmpxchg [rsi], cx
        (&[0xF0, 0x0F, 0xB1, 0x0E], 32, true), // lock cmpxchg [rsi], ecx
        (&[0xF0, 0x48, 0x0F, 0xB1, 0x0E], 64, true), // lock cmpxchg [rsi], rcx
        (&[0x0F, 0xB0, 0xCB], 8, false),      // cmpxchg bl, cl
        (&[0x66, 0x0F, 0xB1, 0xCB], 16, false), // cmpxchg bx, cx
        (&[0x0F, 0xB1, 0xCB], 32, false),     // cmpxchg ebx, ecx
        (&[0x48, 0x0F, 0xB1, 0xCB], 64, false), // cmpxchg rbx, rcx
    ];
    const ACCUMULATOR: u64 = 0x1111_2222_3333_4444;
    const SOURCE: u64 = 0xAAAA_BBBB_CCCC_DDDD;
    const ABOVE: u64 = 0x5555_6666_7777_8888;
    for &(code, bits, in_memory) in forms {
        let mask = low(bits);
        for matches in [true, false] {
            // The destination holds the accumulator's low bits, or differs in
            // bit 0, under bits the operation must not read.
            let old = (ABOVE & !mask) | ((ACCUMULATOR ^ u64::from(!matches)) & mask);
            let mut cpu = CpuState::new(ENTRY, 0);
            cpu.gpr[RAX] = ACCUMULATOR;
            cpu.gpr[RCX] = SOURCE;
            cpu.gpr[RBX] = old;
            cpu.gpr[RSI] = DATA + 0x80;
            // accumulator - destination is 0, or -1 when the destination is
            // one more. Start from the opposite flags so each must be written.
            let after = if matches {
                flags(false, true, false, false, true, false)
            } else {
                flags(true, false, true, false, true, true)
            };
            cpu.flags = flags(
                !after.cf, !after.zf, !after.sf, !after.of, !after.pf, !after.af,
            );
            let mut memory = image(code);
            memory.write_u64(DATA + 0x80, old).unwrap();

            // A match stores the source. A mismatch stores the old value back
            // and loads it into the accumulator. An 8- or 16-bit register write
            // keeps the bits above it, a 32-bit one clears them, and a memory
            // write touches only its own bytes.
            let stored = if matches { SOURCE & mask } else { old & mask };
            let merged = |before: u64, value: u64| match bits {
                32 => value,
                _ => (before & !mask) | value,
            };
            let mut wanted = cpu.clone();
            wanted.rip = ENTRY + code.len() as u64;
            wanted.flags = after;
            if !matches {
                wanted.gpr[RAX] = merged(ACCUMULATOR, old & mask);
            }
            let wanted_memory = if in_memory {
                (old & !mask) | stored
            } else {
                wanted.gpr[RBX] = merged(old, stored);
                old
            };

            for run in both(&cpu, &memory, 1) {
                let what = format!("{code:02X?} matches={matches} on the {}", run.name);
                assert_eq!(run.exit, ExitReason::StepLimitExceeded, "{what}");
                assert_eq!(run.cpu, wanted, "{what}");
                assert_eq!(
                    run.memory.read_u64(DATA + 0x80).unwrap(),
                    wanted_memory,
                    "{what}"
                );
            }
        }
    }
}

#[test]
fn locked_operands_may_be_unaligned_and_span_two_regions() {
    // No alignment is required of a locked operand (SDM Vol. 2, "LOCK"). The
    // last address straddles the data and a second writable region after it.
    let spanning = |code: &[u8]| {
        let mut memory = image(code);
        memory
            .map_region(END, PAGE as usize, MemoryPermissions::READ_WRITE)
            .unwrap();
        memory
    };
    for at in [DATA + 1, DATA + 0x33, DATA + PAGE - 3, END - 8, END - 3] {
        // lock xadd [rsi], rcx
        let mut cpu = CpuState::new(ENTRY, 0);
        cpu.gpr[RCX] = 0x0000_0001_0000_0003;
        cpu.gpr[RSI] = at;
        let mut memory = spanning(&[0xF0, 0x48, 0x0F, 0xC1, 0x0E]);
        memory.write_u64(at, 0xFFFF_FFFF_FFFF_FFFE).unwrap();
        for run in both(&cpu, &memory, 1) {
            assert_eq!(run.exit, ExitReason::StepLimitExceeded, "{at:#x}");
            assert_eq!(run.cpu.gpr[RCX], 0xFFFF_FFFF_FFFF_FFFE, "{at:#x}");
            assert_eq!(
                run.memory.read_u64(at).unwrap(),
                0x0000_0001_0000_0001,
                "{at:#x}"
            );
            // 0xFFFF_FFFF_FFFF_FFFE + 0x0000_0001_0000_0003 carries out of bit
            // 63 and bit 3; the sum is positive, nonzero and ends in 0x01.
            assert_eq!(
                run.cpu.flags,
                flags(true, false, false, false, false, true),
                "{at:#x}"
            );
        }

        // lock cmpxchg [rsi], rcx, with a matching accumulator
        let mut cpu = CpuState::new(ENTRY, 0);
        cpu.gpr[RAX] = 0x8877_6655_4433_2211;
        cpu.gpr[RCX] = 0x0102_0304_0506_0708;
        cpu.gpr[RSI] = at;
        let mut memory = spanning(&[0xF0, 0x48, 0x0F, 0xB1, 0x0E]);
        memory.write_u64(at, 0x8877_6655_4433_2211).unwrap();
        for run in both(&cpu, &memory, 1) {
            assert_eq!(run.exit, ExitReason::StepLimitExceeded, "{at:#x}");
            assert_eq!(run.cpu.gpr[RAX], 0x8877_6655_4433_2211, "{at:#x}");
            assert_eq!(
                run.memory.read_u64(at).unwrap(),
                0x0102_0304_0506_0708,
                "{at:#x}"
            );
        }
    }
}

#[test]
fn a_fault_on_any_byte_of_the_operand_changes_nothing() {
    let xadd: &[u8] = &[0xF0, 0x48, 0x0F, 0xC1, 0x0E]; // lock xadd [rsi], rcx
    let cmpxchg: &[u8] = &[0xF0, 0x48, 0x0F, 0xB1, 0x0E]; // lock cmpxchg [rsi], rcx
    // The last three bytes of writable memory, then five bytes that are
    // unmapped or read-only.
    let at = END - 3;
    for code in [xadd, cmpxchg] {
        for read_only_tail in [false, true] {
            // Accumulators that match and that do not match the operand's
            // writable bytes: a failed compare needs the write access too.
            for accumulator in [0x0000_0000_00CC_BBAA_u64, 7] {
                let mut memory = image(code);
                memory.write_bytes(at, &[0xAA, 0xBB, 0xCC]).unwrap();
                if read_only_tail {
                    memory
                        .map_region(END, PAGE as usize, MemoryPermissions::READ_ONLY)
                        .unwrap();
                }
                let mut cpu = CpuState::new(ENTRY, 0);
                cpu.gpr[RAX] = accumulator;
                cpu.gpr[RCX] = 0x1122_3344_5566_7788;
                cpu.gpr[RSI] = at;
                cpu.flags = flags(true, false, true, false, true, false);
                for run in both(&cpu, &memory, 1) {
                    let what = format!(
                        "{code:02X?} tail read-only={read_only_tail} rax={accumulator:#x} on the {}",
                        run.name
                    );
                    assert!(
                        matches!(run.exit, ExitReason::MemoryFault(_)),
                        "{what}: {:?}",
                        run.exit
                    );
                    assert_eq!(run.cpu, cpu, "{what}");
                    assert_eq!(data(&run.memory), data(&memory), "{what}");
                }
            }
        }
    }
}

#[test]
fn compare_and_swap_loop_retries_then_fetch_and_add_follows() {
    // retry: lea rcx, [rax + 1]
    //        lock cmpxchg [rsi], rcx
    //        jne retry
    //        mov edx, 5
    //        lock xadd [rsi], rdx
    //        hlt
    let code = [
        0x48, 0x8D, 0x48, 0x01, 0xF0, 0x48, 0x0F, 0xB1, 0x0E, 0x75, 0xF5, 0xBA, 0x05, 0x00, 0x00,
        0x00, 0xF0, 0x48, 0x0F, 0xC1, 0x16, 0xF4,
    ];
    const COUNTER: u64 = DATA + 0x100;
    let mut cpu = CpuState::new(ENTRY, 0);
    // A stale expectation: the first exchange fails and reloads RAX, the
    // second succeeds.
    cpu.gpr[RAX] = 0xDEAD;
    cpu.gpr[RSI] = COUNTER;
    let mut memory = image(&code);
    memory.write_u64(COUNTER, 41).unwrap();
    for run in both(&cpu, &memory, 64) {
        assert_eq!(run.exit, ExitReason::Halt, "{}", run.name);
        assert_eq!(run.cpu.gpr[RAX], 41, "{}", run.name);
        assert_eq!(run.cpu.gpr[RCX], 42, "{}", run.name);
        // XADD returns the value before its addition.
        assert_eq!(run.cpu.gpr[RDX], 42, "{}", run.name);
        assert_eq!(run.memory.read_u64(COUNTER).unwrap(), 47, "{}", run.name);
        assert_eq!(run.cpu.rip, ENTRY + code.len() as u64, "{}", run.name);
        // Two exchanges and one addition, none lowered to ARM64 yet.
        if run.name == "DBT" {
            assert_eq!(run.helpers, 3);
        }
    }
}
