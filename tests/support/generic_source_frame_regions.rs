//! 泛型循环回归检查隐藏 iterator 槽、可见绑定、赋值与数组 RHS 的完整轨迹。
use super::lua51_roundtrip::{Workspace, compile, tool, with_stdin};
use std::process::Command;
use unluac::decompile::{DecompileOptions, NamingMode, decompile};
use unluac::parser::RawLiteralConst;
use unluac::transformer::{LowInstr, LoweredProto};
const SOURCE: &str = include_str!("../regress-case/regress_428_generic_source_frame.lua");
fn options(mode: NamingMode) -> DecompileOptions {
    let mut options = DecompileOptions::default();
    options.naming.mode = mode;
    options
}
fn builder(proto: &LoweredProto) -> &LoweredProto {
    let mut candidates = proto.children.iter().filter(|proto| {
        proto.signature.num_params == 2
            && proto
                .instrs
                .iter()
                .filter(|instruction| matches!(instruction, LowInstr::GenericForLoop(_)))
                .count()
                == 2
    });
    let result = candidates.next().unwrap();
    assert!(candidates.next().is_none());
    result
}
fn check(source: &str) {
    let workspace = Workspace::new();
    let expected = with_stdin(Command::new(tool("lua")).arg("-"), source).stdout;
    for strip in [true, false] {
        let bytes = compile(&workspace, source, strip);
        for mode in [
            NamingMode::Simple,
            NamingMode::DebugLike,
            NamingMode::Heuristic,
        ] {
            let result = decompile(&bytes, options(mode)).expect("strict generic source frame");
            let generated = result.state.generated.unwrap();
            let actual = with_stdin(Command::new(tool("lua")).arg("-"), &generated.source).stdout;
            assert_eq!(
                actual, expected,
                "strip={strip} mode={mode:?}\n{}",
                generated.source
            );
            let recompiled = compile(&workspace, &generated.source, strip);
            let second = decompile(&recompiled, options(mode)).expect("strict generic roundtrip");
            let before = result.state.lowered.unwrap();
            let after = second.state.lowered.unwrap();
            let before = builder(&before.main);
            let after = builder(&after.main);
            assert_eq!(
                before.instrs, after.instrs,
                "all loop protocol and scratch instructions"
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
fn regressions_428_generic_loop_cells_assignments_and_indexed_arrays() {
    check(SOURCE);
}

#[test]
fn regressions_428_generic_binding_width_keeps_hidden_iterator_slots() {
    for variables in ["key", "key, value, unused"] {
        check(
            &SOURCE
                .replace(
                    "for key, value in Frame428Pairs(Frame428Defs)",
                    &format!("for {variables} in Frame428Pairs(Frame428Defs)"),
                )
                .replace("    assert(weak.iterator ~= nil)\n", ""),
        );
    }
}

#[test]
fn regressions_428_malformed_loop_protocol_and_hidden_slot_reads_are_rejected() {
    let workspace = Workspace::new();
    for strip in [true, false] {
        let bytes = compile(&workspace, SOURCE, strip);
        let result = decompile(&bytes, options(NamingMode::Simple)).unwrap();
        let lowered = result.state.lowered.unwrap();
        let proto = builder(&lowered.main);
        let index = lowered
            .main
            .children
            .iter()
            .position(|candidate| std::ptr::eq(candidate.as_ref(), proto))
            .unwrap();
        let raw = result.state.raw_chunk.unwrap();
        let targets = [
            proto.instrs.iter().position(|instruction| matches!(instruction,LowInstr::GenericForCall(_))).map(|pc|(pc,14,9,3)),
            proto.instrs.iter().position(|instruction| matches!(instruction,LowInstr::Move(mov) if mov.dst.index()==13 && mov.src.index()==9)).map(|pc|(pc,23,9,6)),
            proto.instrs.iter().position(|instruction| matches!(instruction,LowInstr::UnaryOp(unary) if unary.dst.index()==10)).map(|pc|(pc,6,8,11)),
        ];
        for target in targets {
            let (pc, shift, width, value) = target.unwrap();
            let [raw_pc] = proto.lowering_map.low_to_raw[pc].as_slice() else {
                panic!("one original instruction");
            };
            let origin = raw.main.common.children[index].common.instructions[raw_pc.index()].origin;
            let word = u32::try_from(origin.raw_word.unwrap()).unwrap();
            let changed_word = (word & !(((1 << width) - 1) << shift)) | (value << shift);
            let encoded = if bytes[6] == 1 {
                changed_word.to_le_bytes()
            } else {
                changed_word.to_be_bytes()
            };
            let mut changed = bytes.clone();
            changed[origin.span.offset..origin.span.offset + 4].copy_from_slice(&encoded);
            assert!(
                decompile(&changed, options(NamingMode::Simple)).is_err(),
                "uncertified protocol/write at {pc}"
            );
        }
    }
}
