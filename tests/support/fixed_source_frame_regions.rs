//! 固定多结果回归保留未读槽和返回宽度，并对照 GC、异常与完整 LIR。
use super::lua51_roundtrip::{Workspace, compile, tool, with_stdin};
use std::process::Command;
use unluac::decompile::{
    DecompileOptions, GenerateMode, GeneratedChunkKind, NamingMode, decompile,
};
use unluac::parser::RawLiteralConst;
use unluac::transformer::{LowInstr, LoweredProto};

const SOURCE: &str = include_str!("../regress-case/regress_434_fixed_source_frame.lua");

fn options(mode: NamingMode) -> DecompileOptions {
    let mut options = DecompileOptions::default();
    options.generate.mode = GenerateMode::Strict;
    options.naming.mode = mode;
    options
}

fn builder(proto: &LoweredProto) -> &LoweredProto {
    let mut candidates = proto.children.iter().filter(|proto| {
        proto.signature.num_params == 2
            && proto.instrs.iter().any(|instruction| {
                matches!(instruction,
                LowInstr::Call(call) if matches!(call.results,
                    unluac::transformer::ResultPack::Fixed(results) if results.len == 4))
            })
    });
    let result = candidates.next().expect("one fixed-result builder");
    assert!(candidates.next().is_none());
    result
}

fn check(source: &str) {
    let workspace = Workspace::new();
    let expected = with_stdin(Command::new(tool("lua")).arg("-"), source).stdout;
    assert!(String::from_utf8_lossy(&expected).contains("sink:pair-live:four-live:three-live"));
    for strip in [true, false] {
        let bytes = compile(&workspace, source, strip);
        for mode in [
            NamingMode::Simple,
            NamingMode::DebugLike,
            NamingMode::Heuristic,
        ] {
            let result =
                decompile(&bytes, options(mode)).expect("strict fixed-result source frame");
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
                .expect("complete fixed-result source-frame metadata");
            for slot in [4, 5, 7, 8, 9, 10, 11, 12, 13] {
                assert!(
                    frame.local_slots.values().any(|found| *found == slot),
                    "fixed result slot {slot} remains pinned"
                );
            }
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
                decompile(&recompiled, options(mode)).expect("fixed-result source-frame roundtrip");
            let before = result.state.lowered.unwrap();
            let after = second.state.lowered.unwrap();
            let before = builder(&before.main);
            let after = builder(&after.main);
            assert_eq!(
                before.instrs, after.instrs,
                "all fixed result widths, control, scope and scratch writes"
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
fn regressions_434_fixed_groups_preserve_padding_unused_results_and_gc() {
    check(SOURCE);
}

#[test]
fn regressions_434_open_arguments_preserve_fixed_result_widths() {
    check(&SOURCE.replace(
        "Frame434Coords(player)\n    local unusedThree",
        "Frame434Coords(Frame434Args(player))\n    local unusedThree",
    ));
    let source = SOURCE
        .replace(
            "local unusedPair, owner = player.owner()",
            "local unusedPair, owner = player.owner(Frame434Args(player))",
        )
        .replace(
            "local unusedThree, a, b = Frame434Triple()",
            "local unusedThree, a, b = Frame434Triple(Frame434Args(player))",
        );
    check(&source);
}

#[test]
fn regressions_434_unproved_result_slots_stay_rejected() {
    let workspace = Workspace::new();
    for strip in [true, false] {
        let bytes = compile(&workspace, SOURCE, strip);
        let result = decompile(&bytes, options(NamingMode::Simple)).unwrap();
        let lowered = result.state.lowered.as_ref().unwrap();
        let proto = builder(&lowered.main);
        let index = lowered
            .main
            .children
            .iter()
            .position(|candidate| std::ptr::eq(candidate.as_ref(), proto))
            .unwrap();
        let (pc, call) = proto
            .instrs
            .iter()
            .enumerate()
            .find_map(|(pc, instruction)| {
                if let LowInstr::Call(call) = instruction {
                    matches!(call.results, unluac::transformer::ResultPack::Fixed(results)
                    if results.len == 4)
                    .then_some((pc, call))
                } else {
                    None
                }
            })
            .unwrap();
        let [raw_pc] = proto.lowering_map.low_to_raw[pc].as_slice() else {
            panic!("one original CALL");
        };
        let origin = result
            .state
            .raw_chunk
            .as_ref()
            .unwrap()
            .main
            .common
            .children[index]
            .common
            .instructions[raw_pc.index()]
        .origin;
        let word = u32::try_from(origin.raw_word.unwrap()).unwrap();
        for (shift, width, value) in [(14, 9, 3), (6, 8, call.callee.index() as u32 + 1)] {
            let changed_word = (word & !(((1 << width) - 1) << shift)) | (value << shift);
            let mut changed = bytes.clone();
            let encoded = if bytes[6] == 1 {
                changed_word.to_le_bytes()
            } else {
                changed_word.to_be_bytes()
            };
            changed[origin.span.offset..origin.span.offset + 4].copy_from_slice(&encoded);
            if let Ok(result) = decompile(&changed, options(NamingMode::Simple)) {
                let hir = result.state.hir.unwrap();
                let child = hir.protos[hir.entry.index()].children[index];
                assert!(
                    hir.protos[child.index()].source_frame.is_none(),
                    "unknown result/producer slot must not gain a frame certificate"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires local original/recompiled archive paths; never executes game Lua"]
fn regressions_434_archive_fixed_builder_trace() {
    let original = std::fs::read(std::env::var("UNLUAC_434_ARCHIVE").unwrap()).unwrap();
    let recompiled = std::fs::read(std::env::var("UNLUAC_434_RECOMPILED").unwrap()).unwrap();
    let mut opts = options(NamingMode::Simple);
    opts.parse.string_encoding = "gbk".parse().unwrap();
    opts.target_stage = unluac::decompile::DecompileStage::Transformer;
    let before = decompile(&original, opts.clone())
        .unwrap()
        .state
        .lowered
        .unwrap();
    let after = decompile(&recompiled, opts).unwrap().state.lowered.unwrap();
    let a = builder(&before.main);
    let b = builder(&after.main);
    assert_eq!(
        a.instrs.len(),
        b.instrs.len(),
        "complete archive instruction count"
    );
    for (pc, (before, after)) in a.instrs.iter().zip(&b.instrs).enumerate() {
        assert_eq!(before, after, "complete archive instruction {pc}");
    }
    assert_eq!(a.frame.max_stack_size, b.frame.max_stack_size);
    assert_eq!(a.upvalues.common.count, b.upvalues.common.count);
    assert_eq!(
        a.constants.common.literals.len(),
        b.constants.common.literals.len()
    );
    for (a, b) in a
        .constants
        .common
        .literals
        .iter()
        .zip(&b.constants.common.literals)
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
