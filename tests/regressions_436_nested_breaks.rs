//! 贯通官方 Lua 工具链，检验嵌套 numeric/generic break 的完整控制边和物理槽。
#[path = "support/lua51_roundtrip.rs"]
mod lua51_roundtrip;

use lua51_roundtrip::{Workspace, compile, tool, with_stdin};
use std::process::Command;
use unluac::decompile::{DecompileOptions, NamingMode, decompile};
use unluac::parser::RawLiteralConst;
use unluac::transformer::{LowInstr, LoweredProto};

const SOURCE: &str = include_str!("regress-case/regress_436_nested_source_frame_breaks.lua");

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
                .any(|instruction| matches!(instruction, LowInstr::SetList(_)))
    });
    let result = candidates.next().expect("one break builder");
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
            let result = decompile(&bytes, options(mode)).expect("strict nested break frame");
            let lowered = result.state.lowered.as_ref().unwrap();
            let original = builder(&lowered.main);
            let index = lowered
                .main
                .children
                .iter()
                .position(|child| std::ptr::eq(child.as_ref(), original))
                .unwrap();
            let hir = result.state.hir.as_ref().unwrap();
            let child = hir.protos[hir.entry.index()].children[index];
            assert!(
                hir.protos[child.index()].source_frame.is_some(),
                "the whole control-flow frame must be certified"
            );
            let generated = result.state.generated.unwrap().source;
            let actual = with_stdin(Command::new(tool("lua")).arg("-"), &generated).stdout;
            assert_eq!(actual, expected, "strip={strip}, mode={mode:?}");
            let second = decompile(&compile(&workspace, &generated, strip), options(mode))
                .expect("strict nested break roundtrip");
            let before = result.state.lowered.unwrap();
            let after = second.state.lowered.unwrap();
            let before = builder(&before.main);
            let after = builder(&after.main);
            assert_eq!(
                before.instrs, after.instrs,
                "preserve every break edge and slot write"
            );
            assert_eq!(before.frame, after.frame);
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
fn regressions_436_nested_for_breaks_preserve_scope_and_gc() {
    check(SOURCE);
    check(&SOURCE.replace(
        "for index = 1, 3 do",
        "for index, unused in ipairs({1, 2, 3}) do",
    ));
}

#[test]
fn regressions_436_foreign_exit_is_not_a_break_and_latch_keeps_its_trace() {
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
            .position(|child| std::ptr::eq(child.as_ref(), proto))
            .unwrap();
        let (latch, inner_exit) = proto
            .instrs
            .iter()
            .enumerate()
            .find_map(|(pc, instruction)| {
                let LowInstr::NumericForLoop(step) = instruction else {
                    return None;
                };
                Some((pc, step.exit_target.index()))
            })
            .unwrap();
        let outer_exit = proto
            .instrs
            .iter()
            .find_map(|instruction| {
                let LowInstr::GenericForLoop(step) = instruction else {
                    return None;
                };
                Some(step.exit_target.index())
            })
            .unwrap();
        let jump = proto
            .instrs
            .iter()
            .position(|instruction| {
                matches!(instruction,
            LowInstr::Jump(jump) if jump.target.index() == inner_exit)
            })
            .unwrap();
        let [raw_pc] = proto.lowering_map.low_to_raw[jump].as_slice() else {
            panic!("one physical break jump");
        };
        let raw = result.state.raw_chunk.unwrap();
        let origin = raw.main.common.children[index].common.instructions[raw_pc.index()].origin;
        let word = u32::try_from(origin.raw_word.unwrap()).unwrap();
        for target in [outer_exit, latch] {
            let target_raw = proto.lowering_map.low_to_raw[target]
                .first()
                .unwrap()
                .index();
            let offset =
                i32::try_from(target_raw).unwrap() - i32::try_from(raw_pc.index()).unwrap() - 1;
            let changed_word =
                (word & ((1 << 14) - 1)) | (u32::try_from(offset + 131071).unwrap() << 14);
            let encoded = if bytes[6] == 1 {
                changed_word.to_le_bytes()
            } else {
                changed_word.to_be_bytes()
            };
            let mut changed = bytes.clone();
            changed[origin.span.offset..origin.span.offset + 4].copy_from_slice(&encoded);
            if let Ok(result) = decompile(&changed, options(NamingMode::Simple)) {
                let hir = result.state.hir.as_ref().unwrap();
                let child = hir.protos[hir.entry.index()].children[index];
                if target == outer_exit {
                    assert!(
                        hir.protos[child.index()].source_frame.is_none(),
                        "foreign-loop jump is not the current break"
                    );
                } else if hir.protos[child.index()].source_frame.is_some() {
                    // 跳到 latch 可独立构成合法的条件臂；它不能被错误改成 break。
                    let generated = result.state.generated.unwrap().source;
                    let recompiled = compile(&workspace, &generated, strip);
                    let roundtrip = decompile(&recompiled, options(NamingMode::Simple)).unwrap();
                    assert_eq!(
                        builder(&result.state.lowered.unwrap().main).instrs,
                        builder(&roundtrip.state.lowered.unwrap().main).instrs,
                        "a certified latch jump must preserve the original edge"
                    );
                }
            }
        }
    }
}

#[test]
fn regressions_438_nested_branch_arms_preserve_shared_continuations() {
    let source = include_str!("regress-case/regress_438_shared_arm_continuation.lua");
    check(source);
    check(&source.replace(
        "Arm438Test(flags, 1) and Arm438Test(flags, 2)",
        "Arm438Test(flags, 1) or Arm438Test(flags, 2)",
    ));
    check(&source.replace("        end\n    elseif Arm438Test(flags, 5)",
        "        end\n        if Arm438Test(flags, 1) then Arm438Observe(rows, 'sibling') end\n        for index = 1, 2 do Arm438Observe(rows, index) end\n    elseif Arm438Test(flags, 5)")
        .replace("    Arm438Observe(rows, tag)",
            "    if Arm438Test(flags, 2) then Arm438Observe(rows, 'after-branch') end\n    Arm438Observe(rows, tag)"));
}

#[test]
fn regressions_439_relational_printing_preserves_calls_and_field_read_order() {
    check(include_str!(
        "regress-case/regress_439_relational_evaluation_order.lua"
    ));
}

#[test]
fn regressions_441_initializers_and_parent_keys_keep_constant_insertion_order() {
    let source = include_str!("regress-case/regress_441_initializer_constant_order.lua");
    check(source);
    for (statement, assertion) in [
        (
            "object.method = 31415 < object.method(Pool441Fresh)",
            "assert(object.method == true)",
        ),
        (
            "if 31415 < Pool441Sink(Pool441Fresh) then return rows end",
            "assert(type(Pool441Sink) == 'function')",
        ),
        (
            "local ready = 31415 < Pool441Sink(Pool441Fresh)",
            "assert(type(Pool441Sink) == 'function')",
        ),
        (
            "if 31415 < Pool441Sink(31415, Pool441Fresh) then return rows end",
            "assert(type(Pool441Sink) == 'function')",
        ),
        (
            "local ready = 31415 < Pool441Sink(Pool441Fresh)\n    Pool441Sink = ready",
            "assert(Pool441Sink == true)",
        ),
    ] {
        check(
            &source
                .replace("Pool441Sink = 31415 < Pool441Sink(Pool441Fresh)", statement)
                .replace("assert(Pool441Sink == true)", assertion),
        );
    }
}
