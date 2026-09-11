//! 完整 AND 条件回归比较所有分支路径、builder LIR、常量池和帧宽。
use super::lua51_roundtrip::{Workspace, compile, tool, with_stdin};
use std::process::Command;
use unluac::decompile::{
    DecompileOptions, GenerateMode, GeneratedChunkKind, NamingMode, decompile,
};
use unluac::parser::RawLiteralConst;
use unluac::transformer::{LowInstr, LoweredProto};

const SOURCE: &str = include_str!("../regress-case/regress_435_and_source_frame.lua");

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
                == 4
    });
    let result = candidates.next().expect("one AND builder");
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
            let result = decompile(&bytes, options(mode)).expect("strict AND source frame");
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
                .expect("complete AND source-frame metadata");
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
            let second = decompile(&recompiled, options(mode)).expect("AND source-frame roundtrip");
            let before = result.state.lowered.unwrap();
            let after = second.state.lowered.unwrap();
            let before = builder(&before.main);
            let after = builder(&after.main);
            assert_eq!(
                before.instrs, after.instrs,
                "all shared false edges, control, scope and scratch writes"
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
fn regressions_435_shared_false_edges_keep_and_and_elseif() {
    check(SOURCE);
}

#[test]
fn regressions_435_two_conditions_and_computed_first_preserve_order() {
    check(&SOURCE.replace(" and Frame435Kind.value == 1", ""));
    check(&SOURCE.replace(
        "left == 1 and Frame435Check(right) == 1",
        "Frame435Check(left) == 1 and right == 1",
    ));
}
