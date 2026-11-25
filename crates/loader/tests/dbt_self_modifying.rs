// SPDX-License-Identifier: GPL-3.0-or-later

//! A store that rewrites a later instruction of the block that is executing
//! (#291).
//!
//! Expected behaviour: the Intel SDM (Vol. 3A, "Self-Modifying Code") and the
//! AMD APM (Vol. 2, "Self-Modifying Code") describe processors that detect a
//! store to an instruction that has already been fetched and execute the new
//! bytes. Intel recommends a branch between the store and the modified code for
//! portability; these fixtures deliberately omit it, so they check the stricter
//! case in which the next fetch must see the store. The interpreter decodes each
//! instruction from memory just before executing it, so it is the reference
//! engine here, and every expected value is also written out by hand.
//!
//! The bytes are original. Each program runs from a read-write-execute page.

use xenora_loader::cpu::{CpuState, ExecutionResult, ExitReason, Interpreter, RFlags};
use xenora_loader::dbt::DbtEngine;
use xenora_loader::memory::{GuestMemory, MemoryPermissions};

const CODE: u64 = 0x40_0000;
const CODE_LEN: usize = 0x1000;
const DATA: u64 = 0x50_0000;
const RAX: usize = 0;
const RCX: usize = 1;
const RBX: usize = 3;
const RSP: usize = 4;

fn memory(code: &[u8]) -> GuestMemory {
    let mut bytes = vec![0xF4; CODE_LEN];
    bytes[..code.len()].copy_from_slice(code);
    let mut memory = GuestMemory::new();
    memory
        .map_region_with_data(
            CODE,
            CODE_LEN,
            MemoryPermissions::READ_WRITE_EXECUTE,
            &bytes,
        )
        .unwrap();
    memory
        .map_region(DATA, 0x1000, MemoryPermissions::READ_WRITE)
        .unwrap();
    memory
}

fn cpu() -> CpuState {
    let mut cpu = CpuState::new(CODE, 0);
    cpu.gpr[RCX] = 0xFF;
    cpu.flags = RFlags {
        zf: true,
        ..RFlags::default()
    };
    cpu
}

fn code_bytes(memory: &GuestMemory) -> Vec<u8> {
    let mut bytes = vec![0; CODE_LEN];
    memory.read_bytes(CODE, &mut bytes).unwrap();
    bytes
}

/// Runs `code` with each budget in turn on the interpreter and on one DBT engine,
/// checks that the two agree after every slice, and returns the DBT's results.
fn run_both(code: &[u8], budgets: &[usize]) -> (Vec<ExecutionResult>, GuestMemory, DbtEngine) {
    run_both_from(code, cpu(), budgets)
}

/// [`run_both`] from a given starting state.
fn run_both_from(
    code: &[u8],
    start: CpuState,
    budgets: &[usize],
) -> (Vec<ExecutionResult>, GuestMemory, DbtEngine) {
    let mut reference_memory = memory(code);
    let mut dbt_memory = memory(code);
    let mut reference_cpu = start.clone();
    let mut dbt_cpu = start;
    let mut engine = DbtEngine::new(256 * 1024).unwrap();
    let mut results = Vec::new();
    for &budget in budgets {
        let reference =
            Interpreter::new(reference_cpu.clone(), &mut reference_memory, budget).run();
        let translated = engine.run(&mut dbt_cpu, &mut dbt_memory, budget);
        assert_eq!(
            translated.exit_reason, reference.exit_reason,
            "budget {budget}"
        );
        assert_eq!(
            translated.instructions_executed, reference.instructions_executed,
            "budget {budget}"
        );
        assert_eq!(
            translated.final_state, reference.final_state,
            "budget {budget}"
        );
        assert_eq!(
            code_bytes(&dbt_memory),
            code_bytes(&reference_memory),
            "budget {budget}"
        );
        reference_cpu = reference.final_state.clone();
        results.push(translated);
    }
    (results, dbt_memory, engine)
}

/// `mov byte ptr [rip+3], 0x2A` patches the low immediate byte of the
/// `mov eax, 0x11` two instructions later, in the same block.
const NATIVE_STORE: [u8; 15] = [
    0xC6, 0x05, 0x03, 0x00, 0x00, 0x00, 0x2A, // 0: mov byte ptr [rip+3], 0x2A -> writes 0x0A
    0x90, 0x90, // 7: nop; nop
    0xB8, 0x11, 0x00, 0x00, 0x00, // 9: mov eax, 0x11 (immediate at 0x0A)
    0xF4, // 14: hlt
];

#[test]
fn a_native_store_to_a_later_instruction_in_the_block_is_executed() {
    let (results, memory, engine) = run_both(&NATIVE_STORE, &[64]);
    let result = &results[0];
    assert_eq!(result.exit_reason, ExitReason::Halt);
    assert_eq!(result.instructions_executed, 5);
    assert_eq!(
        result.final_state.gpr[RAX], 0x2A,
        "the patched immediate is used"
    );
    assert_eq!(result.final_state.rip, CODE + 15);
    assert_eq!(code_bytes(&memory)[0x0A], 0x2A);
    // The block ends after the store and the rest is translated again.
    assert_eq!(engine.metrics.dbt_dispatches, 2);
}

/// `add byte ptr [rip+3], 0x19` runs through the shared semantics (byte ALU
/// forms are not lowered) and turns the immediate 0x11 into 0x2A.
#[test]
fn a_store_through_the_shared_semantics_is_executed() {
    let mut code = NATIVE_STORE;
    code[..7].copy_from_slice(&[0x80, 0x05, 0x03, 0x00, 0x00, 0x00, 0x19]);
    let (results, _, _) = run_both(&code, &[64]);
    let result = &results[0];
    assert_eq!(result.exit_reason, ExitReason::Halt);
    assert_eq!(result.instructions_executed, 5);
    assert_eq!(result.final_state.gpr[RAX], 0x11 + 0x19);
}

/// `xor dword ptr [rip+0], 0x00515436` rewrites the next four bytes from
/// `cmp eax, eax; nop; nop` (39 C0 90 90) into `setz cl; nop` (0F 94 C1 90).
/// The old `cmp` overwrote every flag, so the XOR's own flags looked dead when
/// the block was translated; the new `setz` reads its ZF.
#[test]
fn the_writing_instruction_leaves_its_flags_for_the_new_code() {
    let code = [
        0x81, 0x35, 0x00, 0x00, 0x00, 0x00, 0x36, 0x54, 0x51,
        0x00, // 0: xor dword ptr [rip+0], 0x00515436
        0x39, 0xC0, 0x90, 0x90, // 10: cmp eax, eax; nop; nop
        0xF4, // 14: hlt
    ];
    let (results, memory, _) = run_both(&code, &[64]);
    let result = &results[0];
    assert_eq!(&code_bytes(&memory)[10..14], &[0x0F, 0x94, 0xC1, 0x90]);
    assert_eq!(result.exit_reason, ExitReason::Halt);
    // xor, setz, nop, hlt.
    assert_eq!(result.instructions_executed, 4);
    // 0x9090C039 ^ 0x00515436 = 0x90C1940F: nonzero, sign set, low byte 0x0F
    // has four set bits. XOR clears CF and OF; AF is undefined.
    let flags = result.final_state.flags;
    assert!(!flags.zf && flags.sf && flags.pf && !flags.cf && !flags.of);
    assert_eq!(
        result.final_state.gpr[RCX], 0,
        "setz cl saw ZF = 0 from the XOR"
    );
}

#[test]
fn a_budget_that_ends_at_the_store_resumes_on_the_new_code() {
    let (results, _, _) = run_both(&NATIVE_STORE, &[1, 64]);
    assert_eq!(results[0].exit_reason, ExitReason::StepLimitExceeded);
    assert_eq!(results[0].instructions_executed, 1);
    assert_eq!(results[0].final_state.rip, CODE + 7);
    assert_eq!(results[1].exit_reason, ExitReason::Halt);
    assert_eq!(results[1].final_state.gpr[RAX], 0x2A);
}

/// A budget below a whole block uses the budget-limited cache.
#[test]
fn a_budget_limited_block_sees_the_store() {
    let (results, _, _) = run_both(&NATIVE_STORE, &[4]);
    let result = &results[0];
    assert_eq!(result.exit_reason, ExitReason::StepLimitExceeded);
    assert_eq!(result.instructions_executed, 4);
    assert_eq!(result.final_state.gpr[RAX], 0x2A);
    assert_eq!(result.final_state.rip, CODE + 14);
}

/// A store to executable bytes outside the block changes nothing the block
/// executes. Ending the block early must not change the outcome.
#[test]
fn a_store_to_other_executable_bytes_keeps_the_result() {
    let mut code = NATIVE_STORE;
    // mov byte ptr [rip+0x100], 0x2A writes CODE + 0x107, which never runs.
    code[2..6].copy_from_slice(&0x100u32.to_le_bytes());
    let (results, memory, _) = run_both(&code, &[64]);
    let result = &results[0];
    assert_eq!(result.exit_reason, ExitReason::Halt);
    assert_eq!(result.instructions_executed, 5);
    assert_eq!(result.final_state.gpr[RAX], 0x11);
    assert_eq!(code_bytes(&memory)[0x107], 0x2A);
}

/// A store that faults leaves the state as it was before the store.
#[test]
fn a_faulting_store_keeps_its_precise_state() {
    let code = [
        0xBB, 0x07, 0x00, 0x00, 0x00, // 0: mov ebx, 7
        // 5: mov byte ptr [rip+0x2000], 0x2A -> CODE + 0x200C, which is unmapped
        0xC6, 0x05, 0x00, 0x20, 0x00, 0x00, 0x2A, //
        0xB8, 0x11, 0x00, 0x00, 0x00, // 12: mov eax, 0x11
        0xF4, // 17: hlt
    ];
    let (results, memory, _) = run_both(&code, &[64]);
    let result = &results[0];
    assert!(matches!(result.exit_reason, ExitReason::MemoryFault(_)));
    assert_eq!(result.instructions_executed, 1);
    assert_eq!(result.final_state.rip, CODE + 5);
    assert_eq!(result.final_state.gpr[RBX], 7);
    assert_eq!(result.final_state.gpr[RAX], 0);
    assert_eq!(code_bytes(&memory)[..code.len()], code);
}

/// Control: an ordinary data store does not end the block.
#[test]
fn a_data_store_does_not_end_the_block() {
    let code = [
        0xC6, 0x04, 0x25, 0x10, 0x00, 0x50, 0x00, 0x2A, // 0: mov byte ptr [0x500010], 0x2A
        0xB8, 0x11, 0x00, 0x00, 0x00, // 8: mov eax, 0x11
        0xF4, // 13: hlt
    ];
    let (results, _, engine) = run_both(&code, &[64]);
    let result = &results[0];
    assert_eq!(result.exit_reason, ExitReason::Halt);
    assert_eq!(result.final_state.gpr[RAX], 0x11);
    assert_eq!(engine.metrics.dbt_dispatches, 1);
    assert_eq!(engine.metrics.blocks_invalidated, 0);
}

/// `push rax` stores the eight bytes `B8 2A 00 00 00 90 90 90` (`mov eax, 0x2A`
/// and three NOPs) over `mov eax, 0x11; nop; nop; nop` later in the same block.
/// RSP starts on the code page, so the push writes CODE + 0x0C to CODE + 0x13.
#[test]
fn a_push_over_a_later_instruction_in_the_block_is_executed() {
    let code = [
        // 0: mov rax, 0x9090_9000_0000_2AB8
        0x48, 0xB8, 0xB8, 0x2A, 0x00, 0x00, 0x00, 0x90, 0x90, 0x90, //
        0x50, // 10: push rax -> writes CODE + 0x0C..0x13
        0x90, // 11: nop
        0xB8, 0x11, 0x00, 0x00, 0x00, // 12: mov eax, 0x11
        0x90, 0x90, 0x90, // 17: nop; nop; nop
        0xF4, // 20: hlt
    ];
    let mut start = cpu();
    start.gpr[RSP] = CODE + 0x14;
    let (results, memory, engine) = run_both_from(&code, start, &[64]);
    let result = &results[0];
    assert_eq!(
        &code_bytes(&memory)[0x0C..0x14],
        &[0xB8, 0x2A, 0x00, 0x00, 0x00, 0x90, 0x90, 0x90]
    );
    assert_eq!(result.exit_reason, ExitReason::Halt);
    // mov, push, nop, mov, three nops, hlt.
    assert_eq!(result.instructions_executed, 8);
    assert_eq!(result.final_state.gpr[RAX], 0x2A, "the pushed code ran");
    assert_eq!(result.final_state.gpr[RSP], CODE + 0x0C);
    assert_eq!(result.final_state.rip, CODE + 0x15);
    assert_eq!(engine.metrics.dbt_dispatches, 2);
}

/// `movups [rip+2], xmm0` stores sixteen bytes over `mov eax, 0x11` and the
/// eleven NOPs after it in the same block. XMM0 holds `mov eax, 0x2A` and NOPs.
#[test]
fn a_128_bit_store_over_a_later_instruction_in_the_block_is_executed() {
    let mut code = vec![
        0x0F, 0x11, 0x05, 0x02, 0x00, 0x00, 0x00, // 0: movups [rip+2], xmm0 -> CODE + 9
        0x90, 0x90, // 7: nop; nop
        0xB8, 0x11, 0x00, 0x00, 0x00, // 9: mov eax, 0x11
    ];
    code.extend([0x90; 11]); // 14: eleven nops, to CODE + 0x18
    code.push(0xF4); // 25: hlt
    let mut new_code = [0x90; 16];
    new_code[..5].copy_from_slice(&[0xB8, 0x2A, 0x00, 0x00, 0x00]);
    let mut start = cpu();
    start.xmm[0] = u128::from_le_bytes(new_code);
    let (results, memory, engine) = run_both_from(&code, start, &[64]);
    let result = &results[0];
    assert_eq!(&code_bytes(&memory)[9..25], &new_code);
    assert_eq!(result.exit_reason, ExitReason::Halt);
    // movups, two nops, mov, eleven nops, hlt.
    assert_eq!(result.instructions_executed, 16);
    assert_eq!(result.final_state.gpr[RAX], 0x2A, "the stored code ran");
    assert_eq!(engine.metrics.dbt_dispatches, 2);
}

/// Control: CALL always ends its block, so the return address it pushes can
/// only rewrite another block. Here it lands one byte into the target, turning
/// `mov eax, 0x11111111; nop x4` into `mov eax, 0x400005` (the return address)
/// followed by two `add byte ptr [rax], al`, which add 5 twice to the `hlt` at
/// CODE + 5. The next dispatch translates the new bytes.
#[test]
fn a_call_that_rewrites_its_target_runs_the_new_code() {
    let mut code = vec![0xE8, 0x0B, 0x00, 0x00, 0x00]; // 0: call CODE + 0x10
    code.extend([0xF4; 11]); // 5: hlt, never reached
    code.extend([0xB8, 0x11, 0x11, 0x11, 0x11]); // 16: mov eax, 0x11111111
    code.extend([0x90; 4]); // 21: four nops
    code.push(0xF4); // 25: hlt
    let mut start = cpu();
    start.gpr[RSP] = CODE + 0x19;
    let (results, memory, _) = run_both_from(&code, start, &[64]);
    let result = &results[0];
    let bytes = code_bytes(&memory);
    // The return address CODE + 5 = 0x400005, little-endian, at CODE + 0x11.
    assert_eq!(
        &bytes[0x11..0x19],
        &[0x05, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00]
    );
    assert_eq!(bytes[5], 0xF4 + 5 + 5);
    assert_eq!(result.exit_reason, ExitReason::Halt);
    // call, mov, two adds, hlt.
    assert_eq!(result.instructions_executed, 5);
    assert_eq!(result.final_state.gpr[RAX], 0x40_0005);
    assert_eq!(result.final_state.gpr[RSP], CODE + 0x11);
}

/// Times one translated loop body, as the median of `runs` whole runs after a
/// warm-up run. Not a pass/fail check: run it on a revision with and without
/// a change and compare, as in the #291 pull request.
fn median_loop_time(body: &[u8], iterations: u32, runs: usize) -> std::time::Duration {
    // mov ecx, iterations; mov edi, DATA; loop: body; dec ecx; jnz loop; hlt
    let mut code = vec![0xB9];
    code.extend(iterations.to_le_bytes());
    code.push(0xBF);
    code.extend((DATA as u32).to_le_bytes());
    let top = code.len();
    code.extend(body);
    code.extend([0xFF, 0xC9, 0x0F, 0x85]); // dec ecx; jnz rel32
    let rel = top as i64 - (code.len() as i64 + 4);
    code.extend((rel as i32).to_le_bytes());
    code.push(0xF4);
    let mut engine = DbtEngine::new(256 * 1024).unwrap();
    let mut times = Vec::new();
    for run in 0..=runs {
        let mut memory = memory(&code);
        let mut cpu = cpu();
        let started = std::time::Instant::now();
        let result = engine.run(&mut cpu, &mut memory, usize::MAX);
        let elapsed = started.elapsed();
        assert_eq!(result.exit_reason, ExitReason::Halt);
        if run > 0 {
            times.push(elapsed);
        }
    }
    times.sort();
    times[times.len() / 2]
}

/// Reproducible timing for the cost of the code-write check (#291): a loop with
/// one data store per iteration, and one with two memory loads and no store,
/// each one dispatch of a short block per iteration, so per-dispatch costs
/// show up fully. Compare revisions by running both alternately on an idle
/// machine; the medians can shift between runs.
///
/// ```sh
/// cargo test --release -p xenora-loader --test dbt_self_modifying -- \
///     --ignored --nocapture --test-threads=1 code_write_check_timing
/// ```
#[test]
#[ignore = "timing report, not a correctness check"]
fn code_write_check_timing() {
    let store = [
        0x89, 0x07, // mov [rdi], eax
        0x01, 0xD8, // add eax, ebx
        0x31, 0xD3, // xor ebx, edx
        0x01, 0xC2, // add edx, eax
    ];
    let loads = [
        0x8B, 0x07, // mov eax, [rdi]
        0x03, 0x5F, 0x08, // add ebx, [rdi + 8]
        0x31, 0xD3, // xor ebx, edx
        0x01, 0xC2, // add edx, eax
    ];
    for (name, body) in [("one store", &store[..]), ("two loads", &loads[..])] {
        let median = median_loop_time(body, 100_000, 41);
        println!("{name}: median {median:?} for 100000 iterations, 41 runs");
    }
}
