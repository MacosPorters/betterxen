// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for Xenora X14.5 non-executing PE compatibility probe.

use xenora_loader::pe::{IMAGE_SCN_CNT_CODE, IMAGE_SCN_MEM_EXECUTE, IMAGE_SCN_MEM_READ};
use xenora_loader::probe::{ProbeConfig, probe_pe_bytes, run_probe_and_export};

fn create_synthetic_probe_pe() -> Vec<u8> {
    let mut pe = vec![0u8; 1024];
    // MZ header
    pe[0] = b'M';
    pe[1] = b'Z';
    // e_lfanew = 0x80
    pe[0x3c] = 0x80;

    let pe_sig = 0x80;
    pe[pe_sig..pe_sig + 4].copy_from_slice(b"PE\0\0");

    let coff = pe_sig + 4;
    pe[coff..coff + 2].copy_from_slice(&0x8664u16.to_le_bytes()); // AMD64
    pe[coff + 2..coff + 4].copy_from_slice(&1u16.to_le_bytes()); // 1 section
    pe[coff + 16..coff + 18].copy_from_slice(&240u16.to_le_bytes()); // opt header size

    let opt = coff + 20; // 0x80 + 24 = 0x98
    pe[opt..opt + 2].copy_from_slice(&0x20bu16.to_le_bytes()); // PE32+
    pe[opt + 16..opt + 20].copy_from_slice(&0x1000u32.to_le_bytes()); // entry point RVA = 0x1000
    pe[opt + 24..opt + 32].copy_from_slice(&0x140000000u64.to_le_bytes()); // image base
    pe[opt + 32..opt + 36].copy_from_slice(&0x1000u32.to_le_bytes()); // section alignment
    pe[opt + 36..opt + 40].copy_from_slice(&0x200u32.to_le_bytes()); // file alignment
    pe[opt + 56..opt + 60].copy_from_slice(&0x2000u32.to_le_bytes()); // size of image
    pe[opt + 60..opt + 64].copy_from_slice(&0x400u32.to_le_bytes()); // size of headers
    pe[opt + 108..opt + 112].copy_from_slice(&16u32.to_le_bytes()); // number of rva and sizes

    // Section header at opt + 240 = 0x98 + 240 = 0x188
    let sec = opt + 240;
    pe[sec..sec + 5].copy_from_slice(b".text");
    pe[sec + 8..sec + 12].copy_from_slice(&0x200u32.to_le_bytes()); // virtual size
    pe[sec + 12..sec + 16].copy_from_slice(&0x1000u32.to_le_bytes()); // virtual address
    pe[sec + 16..sec + 20].copy_from_slice(&0x200u32.to_le_bytes()); // raw data size
    pe[sec + 20..sec + 24].copy_from_slice(&0x200u32.to_le_bytes()); // raw data pointer (offset 0x200)
    let chars = IMAGE_SCN_CNT_CODE | IMAGE_SCN_MEM_EXECUTE | IMAGE_SCN_MEM_READ;
    pe[sec + 36..sec + 40].copy_from_slice(&chars.to_le_bytes());

    // Place instructions in .text at raw offset 0x200:
    // 48 83 ec 28 : sub rsp, 0x28
    // 48 89 5c 24 20 : mov [rsp + 0x20], rbx (SIB store, first unsupported DBT)
    // c3 : ret
    pe[0x200..0x204].copy_from_slice(&[0x48, 0x83, 0xec, 0x28]);
    pe[0x204..0x209].copy_from_slice(&[0x48, 0x89, 0x5c, 0x24, 0x20]);
    pe[0x209] = 0xc3;

    pe
}

#[test]
fn test_probe_static_analysis_does_not_execute_code() {
    let pe_bytes = create_synthetic_probe_pe();
    let config = ProbeConfig::default();

    let report = probe_pe_bytes(&pe_bytes, &config).expect("probe should succeed");

    assert_eq!(report.parsed.sections.len(), 1);
    assert_eq!(report.parsed.entry_point_rva, 0x1000);
    assert_eq!(report.parsed.image_base, 0x140000000);

    // Verify first unsupported DBT instruction detected at 0x1004 (RVA 0x1004)
    let blocker = report
        .first_unsupported_dbt_insn
        .expect("blocker must be detected");
    assert_eq!(blocker.rva, 0x1004);
    assert_eq!(blocker.bytes, vec![0x48, 0x89, 0x5c, 0x24, 0x20]);
    assert!(blocker.notes.contains("SIB"));
}

#[test]
fn test_probe_artifact_export_creates_all_files() {
    let pe_bytes = create_synthetic_probe_pe();
    let temp_dir = std::env::temp_dir().join("xenora_probe_test_export");
    let _ = std::fs::remove_dir_all(&temp_dir);

    let config = ProbeConfig::default();
    let report =
        run_probe_and_export(&pe_bytes, &temp_dir, &config).expect("export should succeed");

    assert!(temp_dir.join("candidate_static_report.md").exists());
    assert!(temp_dir.join("candidate_static_report.json").exists());
    assert!(temp_dir.join("imports.csv").exists());
    assert!(temp_dir.join("cpu_coverage.csv").exists());
    assert!(temp_dir.join("runtime_gap_matrix.md").exists());
    assert!(temp_dir.join("x15_priority_backlog.md").exists());
    assert!(temp_dir.join("test_commands.txt").exists());

    assert_eq!(report.parsed.sections.len(), 1);

    let _ = std::fs::remove_dir_all(&temp_dir);
}
