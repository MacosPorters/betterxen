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

// ---- f64 whole-wave sum (research/shader-wave-semantics) --------------------------------------
//
// The console's whole-wave f64 sum, as `valu_tests::whole_wave_f64_sum_and_cvt_f32_f64` builds it,
// but storing both words of the f64 result instead of converting to f32, so all 64 bits are
// compared. Upstream lowered it to a native `double` wave operation, which D3DMetal does not run
// (docs/research/shader-wave-semantics.md); it is now done with the integer fp64 add.
// Expected results come from a CPU model of the balanced tree over the lanes in their order
// (lanes 2k and 2k+1, then pairs, ...) with the CPU's own f64 additions, `+0.0` for lanes that
// are not launched; nothing is taken from the recompiler.

const fn vop1(op: u16, vdst: u8, src0: u16) -> u32 {
    0x7E00_0000 | (vdst as u32) << 17 | (op as u32) << 9 | (src0 as u32 & 0x1FF)
}

const fn vop3(op: u16, vdst: u8, src0: u16, src1: u16, src2: u16) -> [u32; 2] {
    [
        0x35 << 26 | (op as u32 & 0x3FF) << 16 | vdst as u32,
        (src0 as u32 & 0x1FF) | (src1 as u32 & 0x1FF) << 9 | (src2 as u32 & 0x1FF) << 18,
    ]
}

const fn buffer_store_dword(vdata: u8, srsrc: u8, byte_off: u16) -> [u32; 2] {
    [
        0x38 << 26 | 0x1C << 18 | (byte_off as u32 & 0xFFF),
        0x80 << 24 | ((srsrc as u32 >> 2) & 0x1F) << 16 | (vdata as u32) << 8,
    ]
}

/// A one-group compute program over `threads` threads: each lane's f64 is root constants
/// `(s0, s1)`; the program sums the wave and stores the sum's low then high word to the UAV.
fn f64_wave_sum_program(threads: u32) -> NativeProgram {
    let v_mov_dpp8 = |vdst: u8, vsrc: u8, sel: u32| [vop1(0x001, vdst, 0xE9), vsrc as u32 | (sel << 8)];
    let v_mov_row_mirror = |vdst: u8, vsrc: u8| {
        [
            vop1(0x001, vdst, 0xFA),
            vsrc as u32 | (0x140 << 8) | (1 << 19) | (0xF << 24) | (0xF << 28),
        ]
    };
    let mut w: Vec<u32> = Vec::new();
    w.push(0xBE80_0000 | 5 << 16 | 0x03 << 8 | 0x80); // s_mov_b32 s5, 0 (table address high)
    w.extend_from_slice(&[0x3D << 26 | 0x02 << 18 | 8 << 6 | (4 >> 1), 125 << 25]); // s_load_dwordx4 s[8:11], s[4:5], 0
    w.push(vop1(0x001, 6, 0)); // v_mov_b32 v6, s0
    w.push(vop1(0x001, 7, 1)); // v_mov_b32 v7, s1
    w.push(0xBE80_0000 | 12 << 16 | 0x3D << 8 | 0xC1); // s_or_saveexec_b32 s12, -1
    w.extend_from_slice(&vop3(0x101, 2, 0x80, 256 + 6, 12));
    w.extend_from_slice(&vop3(0x101, 3, 0x80, 256 + 7, 12));
    for sel in [14570689u32, 9658386, 2340] {
        w.extend_from_slice(&v_mov_dpp8(4, 2, sel));
        w.extend_from_slice(&v_mov_dpp8(5, 3, sel));
        w.extend_from_slice(&vop3(0x164, 2, 256 + 4, 256 + 2, 0)); // v_add_f64
    }
    w.extend_from_slice(&v_mov_row_mirror(4, 2));
    w.extend_from_slice(&v_mov_row_mirror(5, 3));
    w.extend_from_slice(&vop3(0x164, 2, 256 + 4, 256 + 2, 0));
    w.extend_from_slice(&vop3(0x378, 4, 256 + 2, 0x80, 0x80)); // v_permlanex16_b32
    w.extend_from_slice(&vop3(0x378, 5, 256 + 3, 0x80, 0x80));
    w.extend_from_slice(&vop3(0x164, 2, 256 + 4, 256 + 2, 0));
    w.extend_from_slice(&vop3(0x360, 14, 256 + 2, 0x80, 0)); // v_readlane_b32 s14, v2, 0
    w.extend_from_slice(&vop3(0x360, 15, 256 + 3, 0x80, 0)); // v_readlane_b32 s15, v3, 0
    w.push(0xBE80_0000 | 16 << 16 | 0x03 << 8 | 14); // s_mov_b32 s16, s14
    w.push(0xBE80_0000 | 17 << 16 | 0x03 << 8 | 15); // s_mov_b32 s17, s15
    w.push(0xBE80_0000 | 126 << 16 | 0x03 << 8 | 12); // s_mov_b32 exec_lo, s12
    w.push(vop1(0x001, 0, 16)); // v_mov_b32 v0, s16 (low word)
    w.extend_from_slice(&buffer_store_dword(0, 8, 0));
    w.push(vop1(0x001, 0, 17)); // v_mov_b32 v0, s17 (high word)
    w.extend_from_slice(&buffer_store_dword(0, 8, 4));
    w.push(0xBF81_0000); // s_endpgm
    let code: Vec<u8> = w.iter().flat_map(|x| x.to_le_bytes()).collect();
    let param = |root, sgprs, user_data, slot| RootParam {
        root,
        table_offset_dwords: 0,
        sgprs,
        base_register: 0,
        user_data,
        slot,
    };
    NativeProgram {
        stage: HwStage::Cs,
        wave64: false,
        vgprs: 12,
        layout: vec![param(RootType::Constants, 4, 0, 0), param(RootType::Table, 1, 4, 1)],
        used: vec![UsedResource {
            kind: ResourceType::Uav,
            root: RootType::Table,
            table_offset_dwords: 0,
            lower_bound: 0,
            stride: 0,
            dwords: 4,
            user_data: 4,
            slot: 1,
        }],
        io: StageIo::Cs(ComputeIo { rsrc2: 5 << 1, threads: [threads, 1, 1] }),
        code,
    }
}

/// The CPU model: the balanced tree over 32 lanes, the first `launched` holding `x`, the rest `+0.0`.
fn tree_sum(x: f64, launched: usize) -> f64 {
    let mut v: Vec<f64> = (0..32).map(|l| if l < launched { x } else { 0.0 }).collect();
    while v.len() > 1 {
        v = v.chunks(2).map(|p| p[0] + p[1]).collect();
    }
    v[0]
}

#[test]
fn the_f64_wave_sum_is_lowered_without_native_doubles() {
    let shader = translate_native(&f64_wave_sum_program(4)).expect("translate");
    let c = crate::dxbc::Container::parse(&shader.dxil).expect("container");
    let sfi0 = c.part(crate::dxbc::FourCC::SFI0).expect("SFI0 part").data;
    let features = u64::from_le_bytes(sfi0[..8].try_into().unwrap());
    assert_ne!(features & 0x4000, 0, "the sum uses wave operations (sanity check of this parse)");
    assert_eq!(features & 0x1, 0, "no native double may be declared: D3DMetal does not run them");
}

#[test]
fn the_cpu_model_is_the_balanced_tree() {
    // 4 lanes of 1.25: (1.25 + 1.25) twice, then 2.5 + 2.5, then the rest +0
    assert_eq!(tree_sum(1.25, 4), 5.0);
    assert_eq!(tree_sum(1.25, 32), 40.0);
    // the order matters for values that round: this is not the sequential sum
    let x = 0.1;
    let sequential: f64 = (0..32).fold(0.0, |a, _| a + x);
    assert_ne!(tree_sum(x, 32).to_bits(), sequential.to_bits());
}

#[cfg(windows)]
mod f64_on_the_gpu {
    use super::*;
    use crate::emit::host::{Dispatch, Host, Root, View};

    /// Whole-wave f64 sums on the default D3D12 device, all 64 bits compared with the CPU model,
    /// for full and partial waves and for values that round. Fails when there is no device.
    #[test]
    fn f64_wave_sums_match_the_cpu_tree_bit_for_bit() {
        let _one = crate::emit::valu_tests::ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let host = Host::new(false).expect("no SM 6.6 D3D12 hardware device: not verified");
        let mut ran = 0;
        for threads in [1u32, 4, 20, 32] {
            let shader = translate_native(&f64_wave_sum_program(threads)).expect("translate");
            let (root, pso) = host.compute_pipeline(&shader).expect("compute pipeline");
            for x in [1.25f64, 0.1, -3.75e10, 1.0e-300, 123456.789, f64::from_bits(0x3FF0_0000_0000_0001)] {
                let b = x.to_bits();
                let inputs = [b as u32, (b >> 32) as u32, 0, 0];
                let mut ud = vec![0u32; shader.user_data_dwords().max(1) as usize];
                for u in &shader.user_data {
                    ud[u.index as usize] = match (u.root, u.slot) {
                        (RootType::Constants, 0) => inputs[u.dword as usize],
                        (RootType::Table, 1) => 0,
                        other => panic!("unexpected user data {other:?}"),
                    };
                }
                let out = host
                    .dispatch(
                        &root,
                        &pso,
                        &Dispatch {
                            roots: vec![(0, Root::Constants(ud))],
                            views: &[View::RawUav(2)],
                            samplers: &[],
                            groups: [1, 1, 1],
                        },
                    )
                    .unwrap_or_else(|e| panic!("dispatch failed: {e}"));
                let got = u64::from(out[0][0]) | u64::from(out[0][1]) << 32;
                let want = tree_sum(x, threads as usize).to_bits();
                assert_eq!(got, want, "{threads} threads of {x:e}: got {got:#018x}, want {want:#018x}");
                ran += 1;
            }
        }
        eprintln!("betterxen: {ran} f64 wave sums ran on the GPU and matched the CPU tree bit for bit");
    }
}
