// SPDX-License-Identifier: GPL-3.0-or-later

//! Xenora Loader crate.
//!
//! Provides PE32+ parsing, isolated guest virtual memory, bounded x86-64 CPU
//! execution bootstrap, and read-only binary triage.

pub mod arm64;
pub mod cpu;
pub mod dbt;
pub mod jit;
pub mod memory;
pub mod pe;
pub mod triage;

pub use arm64::Arm64Assembler;
pub use cpu::{CpuState, ExecutionResult, ExitReason, Interpreter, RFlags, Register};
pub use dbt::{DbtEngine, DbtMetrics};
pub use jit::{JitError, JitMemory};
pub use memory::{GuestMemory, MemoryError, MemoryPermissions};
pub use pe::{LoadedImage, ParsedPe, PeError, PeSection};
pub use triage::{TriageReport, triage_bytes, triage_file};
