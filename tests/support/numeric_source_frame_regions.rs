//! 数值循环回归贯通官方工具链并比较完整槽协议、GC 和循环路径。
use super::lua51_roundtrip::{Workspace, compile, tool, with_stdin};
use std::process::Command;
use unluac::decompile::{
    DecompileOptions, GenerateMode, GeneratedChunkKind, NamingMode, decompile,
};
use unluac::parser::RawLiteralConst;
use unluac::transformer::{LowInstr, LoweredProto};

const SOURCE: &str = include_str!("../regress-case/regress_432_numeric_source_frame.lua");

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
                .filter(|instruction| matches!(instruction, LowInstr::NumericForInit(_)))
                .count()
                == 2
    });
    let result = candidates.next().expect("one numeric-loop builder");
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
            let result = decompile(&bytes, options(mode)).expect("strict numeric source frame");
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
                .expect("complete numeric source-frame metadata");
            assert!(frame.local_slots.values().any(|slot| *slot == 7));
            assert!(frame.local_slots.values().any(|slot| *slot == 8));
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
                decompile(&recompiled, options(mode)).expect("numeric source-frame roundtrip");
            let before = result.state.lowered.unwrap();
            let after = second.state.lowered.unwrap();
            let before = builder(&before.main);
            let after = builder(&after.main);
            assert_eq!(
                before.instrs, after.instrs,
                "all numeric control, scope and scratch writes"
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
fn regressions_432_numeric_protocol_preserves_hidden_slots_and_gc() {
    check(SOURCE);
}

#[test]
fn regressions_432_numeric_start_limit_step_and_empty_iterations() {
    for header in ["4, 1, -1", "1, 7, 2", "4, 1, 1", "1, 4, 1"] {
        check(&SOURCE.replacen(
            "for index = 1, 4 do",
            &format!("for index = {header} do"),
            1,
        ));
    }
}

#[test]
fn regressions_432_two_computed_conditions_preserve_preheader_order() {
    check(&SOURCE.replace("scene.kind == 1", "scene.kind == Frame432Kind.ACTIVE"));
}

#[test]
fn regressions_432_numeric_header_keeps_parameter_moves_and_limit_calls() {
    check(&SOURCE.replacen("for index = 1, 4 do", "for index = left, 4, 1 do", 1));
    let source = SOURCE
        .replace(
            "Frame432Kind =",
            "function Frame432Limit() event(\"limit\"); return 4 end\nFrame432Kind =",
        )
        .replacen(
            "for index = 1, 4 do",
            "for index = 1, Frame432Limit(), 1 do",
            1,
        );
    check(&source);
}

#[test]
fn regressions_432_malformed_numeric_protocol_and_hidden_reads_stay_rejected() {
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
        let (init_pc, init) = proto
            .instrs
            .iter()
            .enumerate()
            .find_map(|(pc, instruction)| {
                if let LowInstr::NumericForInit(init) = instruction {
                    Some((pc, *init))
                } else {
                    None
                }
            })
            .unwrap();
        let arithmetic = proto
            .instrs
            .iter()
            .enumerate()
            .find_map(|(pc, instruction)| {
                if let LowInstr::BinaryOp(binary) = instruction {
                    (pc > init_pc
                        && binary.rhs == unluac::transformer::ValueOperand::Reg(init.binding))
                    .then_some(pc)
                } else {
                    None
                }
            })
            .unwrap();
        let raw = result.state.raw_chunk.unwrap();
        for (pc, shift, width, value) in [
            (init_pc, 6, 8, init.index.index() as u32 + 1),
            (arithmetic, 14, 9, init.index.index() as u32),
        ] {
            let [raw_pc] = proto.lowering_map.low_to_raw[pc].as_slice() else {
                panic!("one source instruction");
            };
            let origin = raw.main.common.children[index].common.instructions[raw_pc.index()].origin;
            let word = u32::try_from(origin.raw_word.unwrap()).unwrap();
            let changed_word = (word & !(((1 << width) - 1) << shift)) | (value << shift);
            let mut changed = bytes.clone();
            let encoded = if bytes[6] == 1 {
                changed_word.to_le_bytes()
            } else {
                changed_word.to_be_bytes()
            };
            changed[origin.span.offset..origin.span.offset + 4].copy_from_slice(&encoded);
            assert!(
                decompile(&changed, options(NamingMode::Simple)).is_err(),
                "unproved numeric state at {pc}"
            );
        }
    }
}

#[test]
#[ignore = "requires local original/recompiled archive paths; never executes game Lua"]
fn regressions_432_archive_numeric_builder_trace() {
    let original = std::fs::read(std::env::var("UNLUAC_432_ARCHIVE").unwrap()).unwrap();
    let recompiled = std::fs::read(std::env::var("UNLUAC_432_RECOMPILED").unwrap()).unwrap();
    let parameter_count: u8 = std::env::var("UNLUAC_432_PARAMS")
        .unwrap_or_else(|_| "2".to_owned())
        .parse()
        .unwrap();
    let mut opts = options(NamingMode::Simple);
    opts.parse.string_encoding = "gbk".parse().unwrap();
    let before = decompile(&original, opts.clone())
        .unwrap()
        .state
        .lowered
        .unwrap();
    let after = decompile(&recompiled, opts).unwrap().state.lowered.unwrap();
    let select = |root: &LoweredProto| {
        let indexes: Vec<_> = root
            .children
            .iter()
            .enumerate()
            .filter_map(|(index, child)| {
                (child.signature.num_params == parameter_count
                    && child
                        .instrs
                        .iter()
                        .filter(|instruction| matches!(instruction, LowInstr::NumericForInit(_)))
                        .count()
                        == 2
                    && child
                        .instrs
                        .iter()
                        .any(|instruction| matches!(instruction, LowInstr::SetList(_))))
                .then_some(index)
            })
            .collect();
        assert_eq!(indexes.len(), 1, "one numeric archive builder");
        indexes[0]
    };
    let a = &before.main.children[select(&before.main)];
    let b = &after.main.children[select(&after.main)];
    assert_eq!(a.instrs, b.instrs);
    assert_eq!(a.frame.max_stack_size, b.frame.max_stack_size);
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
