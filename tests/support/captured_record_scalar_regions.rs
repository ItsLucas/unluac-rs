//! 记录、捕获标量和开放数组必须共同保持原帧、常量插入与开放结果存活期。
use super::*;

const SOURCE: &str = include_str!("../regress-case/regress_430_captured_record_scalar_open.lua");

fn whole_builder(proto: &LoweredProto) -> Option<&[LowInstr]> {
    Some(&proto.instrs)
}

#[test]
fn regressions_430_captured_record_scalar_boundary_keeps_open_suffix_stack() {
    assert_roundtrip_trace(&Workspace::new(), SOURCE, whole_builder);
}

#[test]
fn regressions_430_record_rounding_keeps_scalar_declaration_boundary() {
    let workspace = Workspace::new();
    for extra in [1, 2, 4, 5] {
        let mut fields = String::new();
        for field in 0..extra {
            writeln!(fields, "extra{field} = {field},").unwrap();
        }
        let source = SOURCE.replace(
            "a = 1, b = 1, text =",
            &format!("{fields} a = 1, b = 1, text ="),
        );
        assert_roundtrip_trace(&workspace, &source, whole_builder);
    }
}

#[test]
fn regressions_430_record_does_not_invent_an_uncaptured_scalar_frame_slot() {
    let workspace = Workspace::new();
    let source = SOURCE.replace(
        "return ids, count, labels, record",
        "return ids, 1, labels, record",
    );
    for strip in [true, false] {
        let bytes = compile(&workspace, &source, strip);
        let error = decompile(&bytes, options(NamingMode::Simple))
            .expect_err("unproved scalar suffix must retain residual");
        assert!(
            error.to_string().contains("residual table-set-list"),
            "{error}"
        );
    }
}

#[test]
fn regressions_430_record_scalar_boundary_keeps_large_constant_pool() {
    let mut prefix = String::new();
    for value in 0..270 {
        writeln!(prefix, "Lion430Load(\"pool430_{value}\")").unwrap();
    }
    let source = SOURCE.replace("Lion430Load(\"one\")", &prefix);
    assert_roundtrip_trace(&Workspace::new(), &source, whole_builder);
}

#[test]
fn regressions_430_new_small_pool_numeric_local_closes_completed_record() {
    let workspace = Workspace::new();
    for value in [2, 3, 4, 6, 7, 8, 987654321] {
        let source = SOURCE
            .replace("local count = 1", &format!("local count = {value}"))
            .replace("count == 1", &format!("count == {value}"));
        assert_roundtrip_trace(&workspace, &source, whole_builder);
    }
}

#[test]
fn regressions_430_unseen_large_pool_scalar_does_not_close_a_record() {
    let workspace = Workspace::new();
    let mut prefix = String::new();
    for value in 0..270 {
        writeln!(prefix, "Lion430Load(\"pool430_{value}\")").unwrap();
    }
    let source = SOURCE
        .replace("Lion430Load(\"one\")", &prefix)
        .replace("local count = 1", "local count = 987654321");
    for strip in [true, false] {
        let bytes = compile(&workspace, &source, strip);
        let error = decompile(&bytes, options(NamingMode::Simple))
            .expect_err("new constant has no proved record/scalar boundary");
        assert!(
            error.to_string().contains("residual table-set-list"),
            "{error}"
        );
    }
}

#[test]
fn regressions_430_captured_record_field_scratch_stays_rejected() {
    let workspace = Workspace::new();
    for strip in [true, false] {
        let bytes = compile(&workspace, SOURCE, strip);
        let result = decompile(&bytes, options(NamingMode::Simple)).unwrap();
        let lowered = result.state.lowered.unwrap();
        let index = lowered
            .main
            .children
            .iter()
            .position(|proto| prefix_and_region(proto).is_some())
            .unwrap();
        let proto = &lowered.main.children[index];
        let count_pc = proto.instrs.iter().enumerate().find_map(|(pc, instruction)| {
            let LowInstr::LoadConst(load) = instruction else { return None; };
            (load.dst.index() == 1
                && matches!(proto.instrs.get(pc + 1), Some(LowInstr::NewTable(table)) if table.dst.index() == 2)
                && matches!(proto.instrs.get(pc.wrapping_sub(1)), Some(LowInstr::SetTable(store)) if store.base == AccessBase::Reg(unluac::transformer::Reg(0))))
                .then_some(pc)
        }).expect("captured count initializer after completed record");
        let key = proto.constants.common.literals.iter().position(|constant|
            matches!(constant, RawLiteralConst::String(string) if string.bytes.as_ref() == b"a")).unwrap();
        assert!(key <= 255);
        let raw = result.state.raw_chunk.unwrap();
        let origin = raw.main.common.children[index].common.instructions[count_pc].origin;
        // Replace count's LOADK with record.a = r1. The later closure now
        // captures the preceding nested-field producer in r1, not a scalar
        // declaration. A rounded allocation hint cannot authorize that split.
        let word = 9 | (((key as u32) | 256) << 23) | (1 << 14);
        let error = rejected_record_word(&bytes, origin.span.offset, word);
        assert!(error.contains("residual"), "{error}");
    }
}
