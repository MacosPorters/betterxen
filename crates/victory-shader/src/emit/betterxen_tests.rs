// SPDX-License-Identifier: GPL-3.0-or-later
// BetterXen tests for the vendored victory-shader crate (see ../../UPSTREAM.md).
//
// BetterXen's own synthetic gfx1030 Wave32 fixtures (`fixtures/rdna2/*.bin`, original, assembled
// with LLVM's llvm-mc) are translated to DXIL by the vendored recompiler. Their expected output
// words come from `fixtures/manifest.toml`, derived by hand there and checked by BetterXen's own
// independent reference; nothing in this file derives an expectation from the recompiler.
//
// Fixture ABI (docs/interfaces.md): v0 = grid index, V# of the input buffer in s[0:3], of the
// output buffer in s[4:7]. The vendored recompiler instead gets descriptors from a table, so
// the program is wrapped in a prologue that loads both from a descriptor table into s[0:3] and
// s[4:7] and then continues into the fixture's code unchanged.

use super::translate_native;
use crate::native::{
    ComputeIo, HwStage, NativeProgram, ResourceType, RootParam, RootType, StageIo, UsedResource,
};

const MANIFEST: &str = include_str!("../../../../fixtures/manifest.toml");

/// Fixtures that bind an input and an output buffer and have expected words in the manifest.
/// Keyed by the file name in the manifest's `path`.
const BINARIES: &[(&str, &[u8])] = &[
    ("add.bin", include_bytes!("../../../../fixtures/rdna2/add.bin")),
    ("float-exact.bin", include_bytes!("../../../../fixtures/rdna2/float-exact.bin")),
    ("bitwise-branch.bin", include_bytes!("../../../../fixtures/rdna2/bitwise-branch.bin")),
    ("or.bin", include_bytes!("../../../../fixtures/rdna2/or.bin")),
    ("shifts.bin", include_bytes!("../../../../fixtures/rdna2/shifts.bin")),
    ("ashr.bin", include_bytes!("../../../../fixtures/rdna2/ashr.bin")),
];

struct Fixture {
    id: String,
    code: &'static [u8],
    input: Vec<u32>,
    expected: Vec<u32>,
}

fn words(line: &str) -> Vec<u32> {
    line.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .filter(|t| !t.trim().is_empty())
        .map(|t| t.trim().parse().expect("a decimal word"))
        .collect()
}

/// Fixtures with input and expected words in the manifest. A fixture the manifest lists with
/// expected words but that is not embedded above fails the test instead of being skipped.
fn fixtures() -> Vec<Fixture> {
    let mut out = Vec::new();
    for block in MANIFEST.split("[[fixture]]").skip(1) {
        let get = |key: &str| {
            block
                .lines()
                .find_map(|l| l.strip_prefix(&format!("{key} = ")))
                .map(str::to_owned)
        };
        let (Some(input), Some(expected)) = (get("input"), get("expected")) else { continue };
        let (input, expected) = (words(&input), words(&expected));
        if input.is_empty() || expected.is_empty() {
            continue;
        }
        let id = get("id").unwrap().trim_matches('"').to_owned();
        let path = get("path").unwrap().trim_matches('"').to_owned();
        let name = path.rsplit('/').next().unwrap();
        let code = BINARIES
            .iter()
            .find(|(n, _)| *n == name)
            .unwrap_or_else(|| panic!("fixture {id} ({name}) is in the manifest but not embedded"))
            .1;
        out.push(Fixture { id, code, input, expected });
    }
    out
}

/// `s_mov_b32 sdst, 0` (inline constant 0 is source 0x80).
const fn s_mov_b32_zero(sdst: u32) -> u32 {
    0xBE80_0000 | sdst << 16 | 0x03 << 8 | 0x80
}

/// SMEM `s_load_dwordx4 s[dst:dst+3], s[base:base+1], byte_offset` (soffset = null).
const fn s_load_dwordx4(dst: u32, base: u32, byte_offset: u32) -> [u32; 2] {
    [0x3D << 26 | 0x02 << 18 | dst << 6 | base >> 1, 125 << 25 | byte_offset]
}

/// `s_waitcnt lgkmcnt(0)` with the other counters left alone.
const S_WAITCNT_LGKM0: u32 = 0xBF8C_C07F;

/// Wraps a fixture so its descriptors arrive through a descriptor table. The table's two
/// 32-byte slots hold the input SRV (heap index 0) and the output UAV (heap index 1).
fn program(code: &[u8], threads: u32) -> NativeProgram {
    let mut w: Vec<u32> = Vec::new();
    w.push(s_mov_b32_zero(9)); // upper half of the table address; the lower half is user data s8
    w.extend(s_load_dwordx4(0, 8, 0x00)); // input V# -> s[0:3]
    w.extend(s_load_dwordx4(4, 8, 0x20)); // output V# -> s[4:7]
    w.push(S_WAITCNT_LGKM0);
    let mut bytes: Vec<u8> = w.iter().flat_map(|x| x.to_le_bytes()).collect();
    bytes.extend_from_slice(code);
    NativeProgram {
        stage: HwStage::Cs,
        wave64: false,
        vgprs: 32,
        layout: vec![RootParam {
            root: RootType::Table,
            table_offset_dwords: 0,
            sgprs: 1,
            base_register: 0,
            user_data: 8,
            slot: 0,
        }],
        used: vec![
            UsedResource {
                kind: ResourceType::Srv,
                root: RootType::Table,
                table_offset_dwords: 0,
                lower_bound: 0,
                stride: 4, // 4-byte elements, as the fixtures index them (idxen)
                dwords: 4,
                user_data: 8,
                slot: 0,
            },
            UsedResource {
                kind: ResourceType::Uav,
                root: RootType::Table,
                table_offset_dwords: 8,
                lower_bound: 0,
                stride: 4, // 4-byte elements, as the fixtures index them (idxen)
                dwords: 4,
                user_data: 8,
                slot: 0,
            },
        ],
        io: StageIo::Cs(ComputeIo { rsrc2: 9 << 1, threads: [threads, 1, 1] }),
        code: bytes,
    }
}

#[test]
fn every_embedded_fixture_is_listed_in_the_manifest() {
    let listed = fixtures();
    assert_eq!(listed.len(), BINARIES.len(), "embedded list and manifest disagree");
}

#[test]
fn fixtures_translate_to_a_dxil_container() {
    for f in fixtures() {
        assert!(!f.expected.is_empty(), "{}: no expected words", f.id);
        let native = program(f.code, f.input.len() as u32);
        let shader = translate_native(&native)
            .unwrap_or_else(|e| panic!("{}: translate_native failed: {e}", f.id));
        assert_eq!(&shader.dxil[..4], b"DXBC", "{}: not a DXBC/DXIL container", f.id);
        assert!(shader.dxil.len() > 100, "{}: container too small", f.id);
    }
}

#[cfg(windows)]
mod on_the_gpu {
    use super::*;
    use crate::emit::host::{Dispatch, Host, Root, Stride, View};

    fn run_on_gpu(host: &Host, f: &Fixture) -> Vec<u32> {
        let shader = translate_native(&program(f.code, f.input.len() as u32)).expect("translate");
        let (root, pso) = host.compute_pipeline(&shader).expect("compute pipeline");
        // The table parameter's value is the heap offset: both views start at heap index 0.
        let ud = vec![0u32; shader.user_data_dwords().max(1) as usize];
        let out = host
            .dispatch(
                &root,
                &pso,
                &Dispatch {
                    roots: vec![(0, Root::Constants(ud))],
                    views: &[
                        View::Buffer { data: &f.input, stride: Stride::Raw },
                        View::RawUav(f.input.len() as u32),
                    ],
                    samplers: &[],
                    groups: [1, 1, 1],
                },
            )
            .unwrap_or_else(|e| panic!("{}: dispatch failed: {e}", f.id));
        out[0][..f.expected.len()].to_vec()
    }

    /// Runs each fixture on the default D3D12 device and compares every output word with the
    /// manifest. If no suitable device exists the test fails loudly rather than passing: a
    /// skipped run is not GPU verification.
    #[test]
    fn fixtures_on_the_default_d3d12_device_match_the_manifest() {
        let _one = crate::emit::valu_tests::ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let host = Host::new(false).expect("no SM 6.6 D3D12 hardware device: not verified");
        let mut ran = 0;
        for f in fixtures() {
            let got = run_on_gpu(&host, &f);
            assert_eq!(got, f.expected, "{}: output words differ from the manifest", f.id);
            eprintln!("betterxen fixture {} ran on the GPU and matched the manifest", f.id);
            ran += 1;
        }
        assert_eq!(ran, fixtures().len());
    }

    /// Negative control: the comparison above must be able to fail. A copy of each expected
    /// output with one bit flipped must not equal what the GPU produced, and the output must
    /// not be the untouched zeroed buffer or an echo of the input (unless the manifest says so).
    #[test]
    fn a_wrong_expectation_is_detected() {
        let _one = crate::emit::valu_tests::ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let host = Host::new(false).expect("no SM 6.6 D3D12 hardware device: not verified");
        for f in fixtures() {
            let got = run_on_gpu(&host, &f);
            let mut wrong = f.expected.clone();
            wrong[0] ^= 1;
            assert_ne!(got, wrong, "{}: a flipped expectation was accepted", f.id);
            assert!(got.iter().any(|&w| w != 0) || f.expected.iter().all(|&w| w == 0), "{}: output is the zeroed buffer", f.id);
        }
    }
}
