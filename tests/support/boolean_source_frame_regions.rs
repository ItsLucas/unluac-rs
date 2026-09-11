//! 布尔物化回归同时验证真假结果、覆盖槽、GC 及完整指令序列。
use super::lua51_roundtrip::{Workspace, compile, tool, with_stdin};
use std::process::Command;
use unluac::decompile::{
    DecompileOptions, GenerateMode, GeneratedChunkKind, NamingMode, decompile,
};
use unluac::parser::RawLiteralConst;
use unluac::transformer::{LowInstr, LoweredProto};

const SOURCE: &str = include_str!("../regress-case/regress_437_boolean_source_frame.lua");

fn options(mode: NamingMode) -> DecompileOptions {
    let mut options = DecompileOptions::default();
    options.generate.mode = GenerateMode::Strict;
    options.naming.mode = mode;
    options
}

fn builder(proto: &LoweredProto) -> &LoweredProto {
    let mut candidates = proto.children.iter().filter(|proto| {
        proto.signature.num_params == 2
            && proto
                .instrs
                .iter()
                .filter(|instruction| matches!(instruction, LowInstr::SetList(_)))
                .count()
                == 2
    });
    let result = candidates.next().expect("one boolean builder");
    assert!(candidates.next().is_none());
    result
}

fn check(source: &str) {
    let workspace = Workspace::new();
    let expected = with_stdin(Command::new(tool("lua")).arg("-"), source).stdout;
    assert!(String::from_utf8_lossy(&expected).contains("use:final:live"));
    for strip in [true, false] {
        let bytes = compile(&workspace, source, strip);
        for mode in [
            NamingMode::Simple,
            NamingMode::DebugLike,
            NamingMode::Heuristic,
        ] {
            let result = decompile(&bytes, options(mode)).expect("strict boolean source frame");
            let hir = result.state.hir.as_ref().unwrap();
            let lowered = result.state.lowered.as_ref().unwrap();
            let original = builder(&lowered.main);
            let index = lowered
                .main
                .children
                .iter()
                .position(|proto| std::ptr::eq(proto.as_ref(), original))
                .unwrap();
            let proto = &hir.protos[hir.protos[hir.entry.index()].children[index].index()];
            let frame = proto
                .source_frame
                .as_ref()
                .expect("complete boolean source-frame metadata");
            assert!(frame.local_slots.values().any(|slot| *slot == 2));
            let generated = result.state.generated.unwrap();
            assert_eq!(generated.kind, GeneratedChunkKind::Source);
            let actual = with_stdin(Command::new(tool("lua")).arg("-"), &generated.source).stdout;
            assert_eq!(
                actual, expected,
                "strip={strip} mode={mode:?}\n{}",
                generated.source
            );
            let recompiled = compile(&workspace, &generated.source, strip);
            let second =
                decompile(&recompiled, options(mode)).expect("boolean source-frame roundtrip");
            let before = result.state.lowered.unwrap();
            let after = second.state.lowered.unwrap();
            let before = builder(&before.main);
            let after = builder(&after.main);
            assert_eq!(
                before.instrs, after.instrs,
                "all materialized boolean writes, control, scope and scratch writes"
            );
            assert_eq!(before.frame.max_stack_size, after.frame.max_stack_size);
            assert_eq!(
                before.constants.common.literals.len(),
                after.constants.common.literals.len()
            );
            for (a, b) in before
                .constants
                .common
                .literals
                .iter()
                .zip(&after.constants.common.literals)
            {
                match (a, b) {
                    (RawLiteralConst::String(a), RawLiteralConst::String(b)) => {
                        assert_eq!(a.bytes, b.bytes)
                    }
                    (RawLiteralConst::Number(a), RawLiteralConst::Number(b)) => {
                        assert_eq!(a.to_bits(), b.to_bits())
                    }
                    _ => assert_eq!(a, b),
                }
            }
        }
    }
}

#[test]
fn regressions_437_comparison_values_preserve_diamond_slots_and_gc() {
    check(SOURCE);
}

#[test]
fn regressions_437_reversed_predicates_keep_their_evaluation_order() {
    check(&SOURCE.replace("left <= right", "left > right"));
    check(&SOURCE.replace(
        "Frame437Read(left) == Frame437Kind.value",
        "Frame437Read(left) ~= Frame437Kind.value",
    ));
}

#[test]
fn regressions_437_unproved_move_before_comparison_cannot_be_inlined() {
    let workspace = Workspace::new();
    let source = SOURCE.replace(
        "local ready = Frame437Read(left) == Frame437Kind.value",
        "local copied = left\n    local ready = copied == Frame437Kind.value",
    );
    for strip in [true, false] {
        let bytes = compile(&workspace, &source, strip);
        for mode in [
            NamingMode::Simple,
            NamingMode::DebugLike,
            NamingMode::Heuristic,
        ] {
            if let Ok(result) = decompile(&bytes, options(mode)) {
                let lowered = result.state.lowered.as_ref().unwrap();
                let original = builder(&lowered.main);
                let index = lowered
                    .main
                    .children
                    .iter()
                    .position(|proto| std::ptr::eq(proto.as_ref(), original))
                    .unwrap();
                let hir = result.state.hir.unwrap();
                let child = hir.protos[hir.entry.index()].children[index];
                assert!(
                    hir.protos[child.index()].source_frame.is_none(),
                    "inlining a bare MOVE would erase an original scratch write"
                );
            }
        }
    }
}
