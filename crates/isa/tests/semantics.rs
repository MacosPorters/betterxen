//! Ties the generated semantic registry tables to the independent XML metadata table.

use xenora_isa::meta::{EncodingFamily, FORMS as META_FORMS};
use xenora_isa::semantics::{
    CarryIn, FORM_IDS, FORMS, GENERATED_SCHEMA, MaskEffect, SCHEMA, SccEffect, Signedness, Wave,
};
use xenora_isa::{
    CompareRelation, ScalarAluOp, ScalarCompareOp, ScalarUnaryOp, VectorAluOp, VectorCompareOp,
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
    dispatch_selects_exactly(EncodingFamily::Sop2, |word| {
        ScalarAluOp::from_word(word).map(|op| op.semantics().name)
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
    // The hand-written VOP2 forms (v_and_b32, v_add_f32, ...) must not be
    // dispatched here.
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
