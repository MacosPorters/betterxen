//! Ties the generated semantic registry tables to the independent XML metadata table.

use xenora_isa::meta::{EncodingFamily, FORMS as META_FORMS};
use xenora_isa::semantics::{
    CarryIn, FORM_IDS, FORMS, GENERATED_SCHEMA, MaskEffect, SCHEMA, SccEffect, Signedness, Wave,
};
use xenora_isa::{
    CompareRelation, ScalarAlu64Op, ScalarAluOp, ScalarCompareOp, ScalarImmediateCompareOp,
    ScalarImmediateOp, ScalarUnaryOp, VectorAluOp, VectorAluVop3Op, VectorCompareOp,
};

#[test]
fn generated_tables_match_the_schema_and_the_metadata_forms() {
    assert_eq!(GENERATED_SCHEMA, SCHEMA);
    assert_eq!(FORM_IDS.len(), FORMS.len());
    for (row, (name, family)) in FORMS.iter().zip(FORM_IDS) {
        assert_eq!((row.name, row.family), (name, family));
        let meta = META_FORMS
            .iter()
            .find(|form| form.name == row.name && form.family == row.family)
            .unwrap_or_else(|| panic!("{} is not an identified form", row.name));
        assert_eq!(meta.opcode, row.opcode, "{}", row.name);
    }
}

/// Every word of `family` that the XML metadata identifies selects an operation
/// exactly when the registry declares that form, and no 32-bit word of another
/// family selects one.
fn dispatch_selects_exactly(family: EncodingFamily, select: impl Fn(u32) -> Option<&'static str>) {
    let declared: Vec<_> = FORMS.iter().filter(|row| row.family == family).collect();
    let mut selected = 0;
    for meta in META_FORMS.iter().filter(|form| form.family == family) {
        match select(meta.bits as u32) {
            Some(name) => {
                assert_eq!(name, meta.name);
                selected += 1;
            }
            None => assert!(
                declared.iter().all(|row| row.name != meta.name),
                "{} is declared but not dispatched",
                meta.name
            ),
        }
    }
    assert_eq!(selected, declared.len());
    for meta in META_FORMS
        .iter()
        .filter(|form| form.family != family && form.bytes == 4)
    {
        assert_eq!(select(meta.bits as u32), None, "{}", meta.name);
    }
}

#[test]
fn scalar_alu_dispatch_selects_exactly_the_registry_sop2_forms() {
    for op in ScalarAluOp::ALL {
        let row = op.semantics();
        assert_eq!(row.family, EncodingFamily::Sop2);
        assert_eq!(row.name.to_ascii_lowercase(), op.mnemonic());
        assert_eq!(op.reads_scc(), row.carry_in == CarryIn::Scc);
        assert_eq!(op.writes_scc(), row.scc != SccEffect::Unchanged);
    }
    for op in ScalarAlu64Op::ALL {
        let row = op.semantics();
        assert_eq!(row.family, EncodingFamily::Sop2);
        assert_eq!(row.width, 64);
        assert_eq!(row.name.to_ascii_lowercase(), op.mnemonic());
        assert_eq!(row.carry_in, CarryIn::None);
        assert_eq!(row.scc, SccEffect::Nonzero);
    }
    dispatch_selects_exactly(EncodingFamily::Sop2, |word| {
        let narrow = ScalarAluOp::from_word(word);
        let wide = ScalarAlu64Op::from_word(word);
        assert!(narrow.is_none() || wide.is_none());
        narrow
            .map(|op| op.semantics().name)
            .or_else(|| wide.map(|op| op.semantics().name))
    });
}

#[test]
fn scalar_compare_dispatch_selects_exactly_the_registry_sopc_forms() {
    for op in ScalarCompareOp::ALL {
        let row = op.semantics();
        assert_eq!(row.family, EncodingFamily::Sopc);
        assert_eq!(row.name.to_ascii_lowercase(), op.mnemonic());
        assert_eq!(row.relation, Some(op.relation()));
    }
    dispatch_selects_exactly(EncodingFamily::Sopc, |word| {
        ScalarCompareOp::from_word(word).map(|op| op.semantics().name)
    });
}

#[test]
fn scalar_immediate_dispatch_selects_exactly_the_registry_sopk_forms() {
    for op in ScalarImmediateOp::ALL {
        let row = op.semantics();
        assert_eq!(row.family, EncodingFamily::Sopk);
        assert_eq!(row.name.to_ascii_lowercase(), op.mnemonic());
        assert_eq!(op.reads_scc(), row.carry_in == CarryIn::Scc);
        assert_eq!(op.writes_scc(), row.scc != SccEffect::Unchanged);
    }
    for op in ScalarImmediateCompareOp::ALL {
        let row = op.semantics();
        assert_eq!(row.family, EncodingFamily::Sopk);
        assert_eq!(row.name.to_ascii_lowercase(), op.mnemonic());
        assert_eq!(row.relation, Some(op.relation()));
    }
    // The two enums together are the registry's SOPK forms; the fixed
    // s_waitcnt_vscnt word (opcode 23) stays hand-written and is in neither.
    dispatch_selects_exactly(EncodingFamily::Sopk, |word| {
        ScalarImmediateOp::from_word(word)
            .map(|op| op.semantics().name)
            .or_else(|| ScalarImmediateCompareOp::from_word(word).map(|op| op.semantics().name))
    });
}

#[test]
fn scalar_immediate_evaluation_extends_simm16_as_the_form_declares() {
    // SIMM16 = 0x8000 is -32768 signed and 32768 unsigned.
    let d = 0xffff_8000;
    assert_eq!(
        ScalarImmediateOp::MovkI32.evaluate(0, 0x8000, false),
        (Some(d), None)
    );
    assert_eq!(
        ScalarImmediateOp::CmovkI32.evaluate(7, 0x8000, true),
        (Some(d), None)
    );
    assert_eq!(
        ScalarImmediateOp::CmovkI32.evaluate(7, 0x8000, false),
        (None, None)
    );
    assert_eq!(
        ScalarImmediateOp::AddkI32.evaluate(0x7fff_ffff, 1, false),
        (Some(0x8000_0000), Some(true))
    );
    assert_eq!(
        ScalarImmediateOp::AddkI32.evaluate(5, 0xffff, false),
        (Some(4), Some(false))
    );
    assert_eq!(
        ScalarImmediateOp::MulkI32.evaluate(3, 0xffff, true),
        (Some(0xffff_fffd), None)
    );
    assert!(ScalarImmediateCompareOp::CmpkEqI32.test(d, 0x8000));
    assert!(!ScalarImmediateCompareOp::CmpkEqU32.test(d, 0x8000));
    assert!(ScalarImmediateCompareOp::CmpkGtU32.test(d, 0xffff));
    assert!(!ScalarImmediateCompareOp::CmpkGtI32.test(d, 0xffff));
}

#[test]
fn scalar_unary_dispatch_selects_exactly_the_registry_sop1_forms() {
    for op in ScalarUnaryOp::ALL {
        let row = op.semantics();
        assert_eq!(row.family, EncodingFamily::Sop1);
        assert_eq!(row.name.to_ascii_lowercase(), op.mnemonic());
        assert_eq!(op.writes_scc(), row.scc != SccEffect::Unchanged);
    }
    // s_mov_b32 (opcode 3) stays hand-written, so it must not be dispatched here.
    dispatch_selects_exactly(EncodingFamily::Sop1, |word| {
        ScalarUnaryOp::from_word(word).map(|op| op.semantics().name)
    });
}

#[test]
fn vector_alu_dispatch_selects_exactly_the_registry_vop2_forms() {
    for op in VectorAluOp::ALL {
        let row = op.semantics();
        assert_eq!(row.family, EncodingFamily::Vop2);
        assert_eq!(row.name.to_ascii_lowercase(), op.mnemonic());
        assert_eq!(op.assembly_name(), format!("{}_e32", op.mnemonic()));
        assert_eq!(op.swaps_sources(), row.swap_sources);
        assert_eq!(row.scc, SccEffect::Unchanged);
    }
    // Exactly the registry VOP2 forms are dispatched here; the hand-written
    // ones (v_add_f32, v_mul_f32) must not be.
    dispatch_selects_exactly(EncodingFamily::Vop2, |word| {
        VectorAluOp::from_word(word).map(|op| op.semantics().name)
    });
}

#[test]
fn vector_compare_dispatch_selects_exactly_the_registry_vopc_forms() {
    const OPERANDS: [u32; 6] = [0, 1, 0x7fff_ffff, 0x8000_0000, 0xffff_fffe, 0xffff_ffff];
    for op in VectorCompareOp::ALL {
        let row = op.semantics();
        assert_eq!(row.family, EncodingFamily::Vopc);
        assert_eq!(row.name.to_ascii_lowercase(), op.mnemonic());
        assert_eq!(op.assembly_name(), format!("{}_e32", op.mnemonic()));
        assert_eq!(row.relation, Some(op.relation()));
        assert_eq!(op.signed(), row.signedness == Signedness::Signed);
        assert_eq!(
            (row.scc, row.vcc, row.exec, row.wave),
            (
                SccEffect::Unchanged,
                MaskEffect::LaneMask,
                MaskEffect::Unchanged,
                Wave::Wave32
            )
        );
        // The per-lane test is the declared relation over the declared
        // signedness, with SRC0 on the left.
        for src0 in OPERANDS {
            for src1 in OPERANDS {
                let ordering = if op.signed() {
                    (src0 as i32).cmp(&(src1 as i32))
                } else {
                    src0.cmp(&src1)
                };
                let expected = match op.relation() {
                    CompareRelation::Less => ordering.is_lt(),
                    CompareRelation::Equal => ordering.is_eq(),
                    CompareRelation::LessOrEqual => ordering.is_le(),
                    CompareRelation::Greater => ordering.is_gt(),
                    CompareRelation::NotEqual => ordering.is_ne(),
                    CompareRelation::GreaterOrEqual => ordering.is_ge(),
                    CompareRelation::False => false,
                    CompareRelation::True => true,
                };
                assert_eq!(
                    op.test(src0, src1),
                    expected,
                    "{} {src0:#x} {src1:#x}",
                    row.name
                );
            }
        }
    }
    // v_cmpx and the 16-, 64-bit and float compares must not be dispatched.
    dispatch_selects_exactly(EncodingFamily::Vopc, |word| {
        VectorCompareOp::from_word(word).map(|op| op.semantics().name)
    });
}

#[test]
fn vop3_variant_dispatch_selects_exactly_the_registry_vop3_rows() {
    for op in VectorAluVop3Op::ALL {
        let (row, base) = (op.semantics(), op.base().semantics());
        assert_eq!((row.name, row.family), (base.name, EncodingFamily::Vop3));
        // The variant inherits the base declaration; only identity and the
        // operand rule differ.
        assert_eq!(
            (
                base.operation,
                base.signedness,
                base.scc,
                base.vcc,
                base.exec
            ),
            (row.operation, row.signedness, row.scc, row.vcc, row.exec)
        );
        assert_eq!(row.swap_sources, base.swap_sources);
        assert_eq!(op.assembly_name(), format!("{}_e64", op.base().mnemonic()));
    }
    dispatch_selects_exactly(EncodingFamily::Vop3, |word| {
        VectorAluVop3Op::from_word(word).map(|op| op.semantics().name)
    });
    // No first word of the other 64-bit families (VOP3B shares VOP3's
    // encoding bits) selects a variant.
    for meta in META_FORMS
        .iter()
        .filter(|form| form.family != EncodingFamily::Vop3 && form.bytes == 8)
    {
        assert_eq!(
            VectorAluVop3Op::from_word(meta.bits as u32),
            None,
            "{}",
            meta.name
        );
    }
}
