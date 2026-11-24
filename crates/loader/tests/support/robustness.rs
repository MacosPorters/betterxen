// SPDX-License-Identifier: GPL-3.0-or-later

//! Drives sequences of standard runtime services with adversarial arguments.
//!
//! Shared by the `runtime_services` libFuzzer target and the stable-toolchain
//! regression test. Each call takes its arguments from the input: zero, all
//! ones, pointers into writable, read-only and unmapped memory, raw values,
//! small integers or an earlier result (so handles flow between calls). After
//! every call the driver checks that read-only and code memory are unchanged
//! and that the Win64 nonvolatile registers are preserved. A panic, a broken
//! invariant or a hang is a bug.

use std::sync::Arc;
use xenora_loader::cpu::{CpuState, Register};
use xenora_loader::imports::{ImportRegistry, Win64Args};
use xenora_loader::memory::{GuestMemory, MemoryPermissions};
use xenora_loader::services::{MockClock, ServicesState, register_standard_services};

const WRITABLE: u64 = 0x5_0000;
const WRITABLE_LEN: usize = 0x1_0000;
const READ_ONLY: u64 = 0x7_0000;
const CODE: u64 = 0x7_8000;
const REGION_LEN: usize = 0x1000;
const STACK: u64 = 0x9_0000;
const STACK_LEN: usize = 0x2000;
const UNMAPPED: u64 = 0xDEAD_0000;
const MAX_CALLS: usize = 48;

/// Services that wait on real time, run guest code or reach native Metal are
/// left out, so each input runs quickly and deterministically.
fn excluded(module: &str, symbol: &str) -> bool {
    module.eq_ignore_ascii_case("xenora_fixture.dll")
        || matches!(symbol, "WSAPoll" | "XTaskQueueDispatch")
}

pub struct Driver {
    services: Vec<(String, String)>,
}

impl Default for Driver {
    fn default() -> Self {
        Self::new()
    }
}

impl Driver {
    pub fn new() -> Self {
        let mut registry = ImportRegistry::new();
        register_standard_services(
            &mut registry,
            Arc::new(ServicesState::new(Arc::new(MockClock::new(0)))),
        );
        let mut services: Vec<(String, String)> = registry
            .iter_registered()
            .filter(|(module, symbol, _)| !excluded(module, symbol))
            .map(|(module, symbol, _)| (module.to_owned(), symbol.to_owned()))
            .collect();
        services.sort();
        Self { services }
    }

    pub fn service_count(&self) -> usize {
        self.services.len()
    }

    /// Run one input against fresh service state and fresh guest memory.
    pub fn run(&self, data: &[u8]) {
        let mut registry = ImportRegistry::new();
        let clock = Arc::new(MockClock::new(0x01DA_0000_0000_0000));
        register_standard_services(&mut registry, Arc::new(ServicesState::new(clock)));

        let mut input = Input(data);
        let mut memory = GuestMemory::new();
        memory
            .map_region(WRITABLE, WRITABLE_LEN, MemoryPermissions::READ_WRITE)
            .unwrap();
        memory
            .map_region(STACK, STACK_LEN, MemoryPermissions::READ_WRITE)
            .unwrap();
        let fill: Vec<u8> = (0..WRITABLE_LEN).map(|i| input.byte_or(i as u8)).collect();
        memory.write_bytes(WRITABLE, &fill).unwrap();
        let read_only: Vec<u8> = (0..REGION_LEN).map(|i| (i * 7) as u8).collect();
        memory
            .map_region_with_data(
                READ_ONLY,
                REGION_LEN,
                MemoryPermissions::READ_ONLY,
                &read_only,
            )
            .unwrap();
        memory
            .map_region_with_data(
                CODE,
                REGION_LEN,
                MemoryPermissions::READ_EXECUTE,
                &[0xC3; REGION_LEN],
            )
            .unwrap();

        let mut results = vec![0u64];
        for _ in 0..MAX_CALLS {
            let Some(selector) = input.u16() else { break };
            let (module, symbol) = &self.services[selector as usize % self.services.len()];
            let mut args = [0u64; 7];
            for arg in &mut args {
                *arg = input.argument(&results);
            }
            let rsp = STACK + STACK_LEN as u64 / 2;
            let mut cpu = CpuState::new(CODE, rsp);
            cpu.gs_base = [0x7_F000, 0x7_F800][input.byte_or(0) as usize % 2];
            for (register, value) in [Register::Rcx, Register::Rdx, Register::R8, Register::R9]
                .into_iter()
                .zip(args)
            {
                cpu.set(register, value);
            }
            for (slot, value) in args[4..].iter().enumerate() {
                memory
                    .write_u64(rsp + 40 + 8 * slot as u64, *value)
                    .unwrap();
            }
            use Register::{R12, R13, R14, R15, Rbp, Rbx, Rdi, Rsi, Rsp};
            let preserved = [Rbx, Rbp, Rdi, Rsi, Rsp, R12, R13, R14, R15];
            let before = preserved.map(|register| cpu.get(register));

            let service = registry
                .binding(module, symbol)
                .expect("registered service");
            let outcome = (service.handler)(&Win64Args::new(&cpu), &mut cpu, &mut memory);

            assert_eq!(
                preserved.map(|register| cpu.get(register)),
                before,
                "{module}!{symbol} changed a nonvolatile register"
            );
            let mut check = vec![0; REGION_LEN];
            memory.read_bytes(READ_ONLY, &mut check).unwrap();
            assert_eq!(check, read_only, "{module}!{symbol} wrote read-only memory");
            memory.read_bytes(CODE, &mut check).unwrap();
            assert!(
                check.iter().all(|&byte| byte == 0xC3),
                "{module}!{symbol} wrote code memory"
            );
            if let Ok(value) = outcome {
                results.push(value);
            }
        }
    }
}

struct Input<'a>(&'a [u8]);

impl Input<'_> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (head, rest) = self.0.split_first_chunk::<N>()?;
        self.0 = rest;
        Some(*head)
    }

    fn byte_or(&mut self, default: u8) -> u8 {
        self.take::<1>().map_or(default, |[byte]| byte)
    }

    fn u16(&mut self) -> Option<u16> {
        self.take::<2>().map(u16::from_le_bytes)
    }

    fn argument(&mut self, results: &[u64]) -> u64 {
        let offset = u64::from(self.u16().unwrap_or(0));
        match self.byte_or(0) % 9 {
            0 => 0,
            1 => u64::MAX,
            2 => WRITABLE + offset % WRITABLE_LEN as u64,
            3 => READ_ONLY + offset % REGION_LEN as u64,
            4 => CODE + offset % REGION_LEN as u64,
            5 => UNMAPPED + offset,
            6 => self.take::<8>().map_or(offset, u64::from_le_bytes),
            7 => offset % 512,
            _ => results[offset as usize % results.len()],
        }
    }
}
