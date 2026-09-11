//! 完整源码帧回归覆盖局部变量保活、短路、异常、GC 和原始 builder 指令。
use super::lua51_roundtrip::{Workspace, compile, tool, with_stdin};
use std::process::Command;
use unluac::decompile::{DecompileOptions, GeneratedChunkKind, NamingMode, decompile};
use unluac::parser::RawLiteralConst;
use unluac::transformer::{LowInstr, LoweredProto};

const SOURCE: &str = include_str!("../regress-case/regress_424_retained_source_frame.lua");
fn options(mode: NamingMode) -> DecompileOptions {
    let mut options = DecompileOptions::default();
    options.naming.mode = mode;
    options
}
fn builder(proto: &LoweredProto) -> &LoweredProto {
    let mut candidates = proto.children.iter().filter(|proto| {
        proto.signature.num_params == 10
            && proto
                .instrs
                .iter()
                .filter(|instruction| matches!(instruction, LowInstr::SetList(_)))
                .count()
                > 10
    });
    let result = candidates
        .next()
        .expect("one complete retained-frame builder");
    assert!(candidates.next().is_none());
    result
}
fn check(source: &str) {
    check_with_slots(source, &[10, 11, 12, 13, 14]);
}
fn check_with_slots(source: &str, slots: &[usize]) {
    let workspace = Workspace::new();
    let expected = with_stdin(Command::new(tool("lua")).arg("-"), source).stdout;
    for strip in [true, false] {
        let bytes = compile(&workspace, source, strip);
        for mode in [
            NamingMode::Simple,
            NamingMode::DebugLike,
            NamingMode::Heuristic,
        ] {
            let result = decompile(&bytes, options(mode)).expect("strict source-frame recovery");
            let hir = result.state.hir.as_ref().unwrap();
            let proto = hir
                .protos
                .iter()
                .find(|proto| proto.signature.num_params == 10)
                .unwrap();
            let frame = proto
                .source_frame
                .as_ref()
                .expect("full source frame metadata");
            assert_eq!(
                frame.local_slots.values().copied().collect::<Vec<_>>(),
                slots
            );
            let generated = result.state.generated.unwrap();
            assert_eq!(generated.kind, GeneratedChunkKind::Source);
            let actual = with_stdin(Command::new(tool("lua")).arg("-"), &generated.source).stdout;
            assert_eq!(
                actual, expected,
                "strip={strip} mode={mode:?}\n{}",
                generated.source
            );
            let recompiled = compile(&workspace, &generated.source, strip);
            let second = decompile(&recompiled, options(mode)).expect("source-frame roundtrip");
            let before = result.state.lowered.unwrap();
            let after = second.state.lowered.unwrap();
            let before = builder(&before.main);
            let after = builder(&after.main);
            assert_eq!(
                before.instrs, after.instrs,
                "all branch, call and allocation register operations"
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
#[ignore = "requires original and separately recompiled local game archive paths; never executes Lua"]
fn regressions_424_archive_complete_builder_trace() {
    let original = std::fs::read(std::env::var("UNLUAC_424_ARCHIVE").unwrap()).unwrap();
    let recompiled = std::fs::read(std::env::var("UNLUAC_424_RECOMPILED").unwrap()).unwrap();
    let mut opts = options(NamingMode::Simple);
    opts.parse.string_encoding = "gbk".parse().unwrap();
    let original = decompile(&original, opts.clone())
        .unwrap()
        .state
        .lowered
        .unwrap();
    let recompiled = decompile(&recompiled, opts).unwrap().state.lowered.unwrap();
    let original = builder(&original.main);
    let recompiled = builder(&recompiled.main);
    assert_eq!(original.instrs, recompiled.instrs);
    assert_eq!(
        original.frame.max_stack_size,
        recompiled.frame.max_stack_size
    );
    assert_eq!(
        original.constants.common.literals.len(),
        recompiled.constants.common.literals.len()
    );
    for (a, b) in original
        .constants
        .common
        .literals
        .iter()
        .zip(&recompiled.constants.common.literals)
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
#[test]
fn regressions_424_single_use_guard_results_keep_source_slots_and_gc_roots() {
    check(SOURCE);
}

#[test]
fn regressions_424_environment_gc_observes_all_original_guard_and_argument_writes() {
    check(&SOURCE.replace(
        "collectgarbage(\"stop\")",
        r#"setfenv(Frame424Build, setmetatable({}, {
    __index = function(_, key)
        collectgarbage("collect")
        collectgarbage("collect")
        trace[#trace + 1] = "lookup:" .. key .. ":"
            .. (weak.scene and "scene-live" or "scene-dead") .. ":"
            .. (weak.scratch and "scratch-live" or "scratch-dead")
        return _G[key]
    end,
}))
collectgarbage("stop")"#,
    ));
}

#[test]
fn regressions_424_certified_control_flow_is_not_folded_by_later_passes() {
    check(&SOURCE.replace(
        "    if not player then return end",
        "    if not player then return end\n    if not player then return end",
    ));
}

#[test]
fn regressions_424_short_circuit_or_keeps_both_branches_and_call_writes() {
    for expected in [0, 1468] {
        let source = SOURCE.replace(
            "if Frame424Library.correct(player) then",
            &format!("if Frame424Library.correct(player) or player.dynamic() == {expected} then"),
        ).replace(
            "            player.setquest(quest, 2, 0)\n        end",
            "            player.setquest(quest, 2, 0)\n        else\n            Frame424Library.remove(player, 999)\n        end",
        );
        check(&source);
    }
}

#[test]
fn regressions_424_ordered_lookup_arithmetic_and_direct_stores_keep_the_frame() {
    let source = SOURCE
        .replace(
            "            Frame424Library.observe(player, 802, rows)",
            r#"            Frame424Library.observe(player, 802, rows)
            Frame424Center = (rows[1][1] + rows[2][1] + rows[3][1]) / 3
            Frame424Saved = player
            player.center = quest
            Frame424Consume(Frame424Center, Frame424Saved.center)"#,
        )
        .replace(
            "        assert(weak.scene ~= nil and weak.scratch == nil)",
            r#"        assert(weak.scene ~= nil and weak.scratch == nil)
        for i = 1, 3 do
            local row = rows[i]
            local number = row[1]
            row[1] = nil
            setmetatable(row, {__index = function(_, key)
                collectgarbage("collect")
                collectgarbage("collect")
                trace[#trace + 1] = "term:" .. number .. ":" .. key
                    .. ":" .. (weak.scene and "scene-live" or "scene-dead")
                    .. ":" .. (weak.scratch and "scratch-live" or "scratch-dead")
                if failure == 3 and number == 83 then error("fixture-stop", 0) end
                return number
            end})
        end"#,
        )
        .replace("for f = 1, 2 do", "for f = 1, 3 do");
    check(&source);
}

#[test]
fn regressions_424_global_constructor_key_order_distinguishes_a_retained_local() {
    for (statement, slots) in [
        (
            "Frame424Tail = {{311, true}, {312, true}}",
            &[10, 11, 12, 13, 14][..],
        ),
        (
            "local tail = {{311, true}, {312, true}}; Frame424Tail = tail",
            &[10, 11, 12, 13, 14, 15][..],
        ),
    ] {
        check_with_slots(
            &SOURCE.replace(
                "            player.setquest(quest, 2, 0)",
                &format!("            player.setquest(quest, 2, 0)\n            {statement}"),
            ),
            slots,
        );
    }
}

#[test]
fn regressions_424_boolean_local_and_scalar_writebacks_keep_their_slots() {
    check_with_slots(
        &SOURCE.replace(
            "    local phase = player.phase(token)",
            "    local shifted = id - 1\n    local phase = player.phase(token)",
        ),
        &[10, 11, 12, 13, 14, 15],
    );
    check_with_slots(&SOURCE.replace(
        "    local phase = player.phase(token)",
        "    local phase = player.phase(token)\n    local enabled = true\n    if enabled then Frame424Consume() end",
    ), &[10, 11, 12, 13, 14, 15]);
    check(&SOURCE.replace(
        "        if Frame424Library.correct(player) then",
        r#"        phase = true
        if phase then Frame424Consume() end
        phase = 2
        phase = player.id
        phase = player.dynamic() * 2
        phase = quest
        if Frame424Library.correct(player) then"#,
    ));
}

#[test]
fn regressions_424_call_led_concat_keeps_moves_literals_upvalues_and_read_order() {
    for expression in [
        "player.dynamic() .. id .. player.dynamic() .. player.id",
        "player.dynamic() .. ':' .. token",
    ] {
        check_with_slots(&SOURCE.replace(
            "    local phase = player.phase(token)",
            &format!("    local phase = player.phase(token)\n    local label = {expression}\n    Frame424Consume(label)"),
        ), &[10, 11, 12, 13, 14, 15]);
    }
}

#[test]
fn regressions_424_reversed_computed_operands_cannot_reorder_lookups() {
    use unluac::transformer::ValueOperand;
    let workspace = Workspace::new();
    let source = SOURCE.replace(
        "            player.setquest(quest, 2, 0)",
        "            player.setquest(quest, 2, 0)\n            Frame424Center = rows[1][1] + rows[2][1]",
    );
    for strip in [true, false] {
        let bytes = compile(&workspace, &source, strip);
        let result = decompile(&bytes, options(NamingMode::Simple)).unwrap();
        let lowered = result.state.lowered.unwrap();
        let proto = builder(&lowered.main);
        let child = lowered
            .main
            .children
            .iter()
            .position(|candidate| std::ptr::eq(candidate.as_ref(), proto))
            .unwrap();
        let pc = proto.instrs.iter().position(|instruction| matches!(instruction,
            LowInstr::BinaryOp(binary) if matches!((binary.lhs,binary.rhs),
                (ValueOperand::Reg(lhs),ValueOperand::Reg(rhs)) if lhs.index() >= 14 && lhs==binary.dst && rhs.index()==lhs.index()+1))).unwrap();
        let [raw_pc] = proto.lowering_map.low_to_raw[pc].as_slice() else {
            panic!("one arithmetic instruction");
        };
        let raw = result.state.raw_chunk.unwrap();
        let origin = raw.main.common.children[child].common.instructions[raw_pc.index()].origin;
        let word = u32::try_from(origin.raw_word.unwrap()).unwrap();
        let lhs = (word >> 23) & 0x1ff;
        let rhs = (word >> 14) & 0x1ff;
        let changed_word = (word & !((0x1ff << 23) | (0x1ff << 14))) | (lhs << 14) | (rhs << 23);
        let encoded = if bytes[6] == 1 {
            changed_word.to_le_bytes()
        } else {
            changed_word.to_be_bytes()
        };
        let mut changed = bytes.clone();
        changed[origin.span.offset..origin.span.offset + 4].copy_from_slice(&encoded);
        // A preserved declaration can keep the first lookup in its original
        // register before a reversed arithmetic use. Require its complete trace;
        // accepting a reordered combined expression would fail this comparison.
        if let Ok(result) = decompile(&changed, options(NamingMode::Simple)) {
            let generated = result.state.generated.unwrap();
            let rebuilt = compile(&workspace, &generated.source, strip);
            let roundtrip = decompile(&rebuilt, options(NamingMode::Simple)).unwrap();
            let before = result.state.lowered.unwrap();
            let after = roundtrip.state.lowered.unwrap();
            assert_eq!(builder(&before.main).instrs, builder(&after.main).instrs);
            assert_eq!(
                builder(&before.main).frame.max_stack_size,
                builder(&after.main).frame.max_stack_size
            );
        }
    }
}

#[test]
fn regressions_424_nonfinite_condition_constants_cannot_certify_a_frame() {
    let workspace = Workspace::new();
    let source = SOURCE.replace(
        "    local player = Frame424Get(id)",
        "    if id < 1e999 then Frame424Consume() end\n    local player = Frame424Get(id)",
    );
    with_stdin(Command::new(tool("lua")).arg("-"), &source);
    for strip in [true, false] {
        let bytes = compile(&workspace, &source, strip);
        assert!(decompile(&bytes, options(NamingMode::Simple)).is_err());
        let infinity = if bytes[6] == 1 {
            f64::INFINITY.to_le_bytes()
        } else {
            f64::INFINITY.to_be_bytes()
        };
        let offsets = bytes
            .windows(8)
            .enumerate()
            .filter_map(|(offset, value)| (value == infinity).then_some(offset))
            .collect::<Vec<_>>();
        assert_eq!(offsets.len(), 1);
        let mut changed = bytes.clone();
        let nan = if bytes[6] == 1 {
            f64::NAN.to_le_bytes()
        } else {
            f64::NAN.to_be_bytes()
        };
        changed[offsets[0]..offsets[0] + 8].copy_from_slice(&nan);
        assert!(decompile(&changed, options(NamingMode::Simple)).is_err());
    }
}

#[test]
fn regressions_424_boolean_guards_cannot_erase_retired_scope_scratch_writes() {
    let workspace = Workspace::new();
    for expression in ["not id", "id < 0"] {
        let source = SOURCE.replace("    local player = Frame424Get(id)",
            &format!("    do local retired = Frame424Produce(1, 2) end\n    do local inverse = {expression}; if inverse then return end end\n    local player = Frame424Get(id)"));
        with_stdin(Command::new(tool("lua")).arg("-"), &source);
        for strip in [true, false] {
            let bytes = compile(&workspace, &source, strip);
            assert!(decompile(&bytes, options(NamingMode::Simple)).is_err());
        }
    }
}

#[test]
fn regressions_424_changed_local_and_guard_scratch_slots_stay_rejected() {
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
        let pc = proto
            .instrs
            .iter()
            .position(|instruction| {
                matches!(instruction,
            LowInstr::LoadBool(load) if load.dst.index() == 25)
            })
            .unwrap();
        let [raw_pc] = proto.lowering_map.low_to_raw[pc].as_slice() else {
            panic!("one bool write");
        };
        let origin = raw.main.common.children[index].common.instructions[raw_pc.index()].origin;
        let word = u32::try_from(origin.raw_word.unwrap()).unwrap();
        for destination in [24, 26] {
            let changed_word = (word & !(0xff << 6)) | (destination << 6);
            let encoded = if bytes[6] == 1 {
                changed_word.to_le_bytes()
            } else {
                changed_word.to_be_bytes()
            };
            let mut changed = bytes.clone();
            changed[origin.span.offset..origin.span.offset + 4].copy_from_slice(&encoded);
            let error = decompile(&changed, options(NamingMode::Simple))
                .expect_err("unproved late overwrite must remain rejected");
            assert!(error.to_string().contains("residual"), "{error}");
        }
        let pc = proto
            .instrs
            .iter()
            .position(|instruction| {
                matches!(instruction,
            LowInstr::BinaryOp(binary) if binary.dst.index() == 19)
            })
            .unwrap();
        let [raw_pc] = proto.lowering_map.low_to_raw[pc].as_slice() else {
            panic!("one binary write");
        };
        let origin = raw.main.common.children[index].common.instructions[raw_pc.index()].origin;
        let word = u32::try_from(origin.raw_word.unwrap()).unwrap();
        let duplicated = (word & !(0x1ff << 14)) | (19 << 14);
        let encoded = if bytes[6] == 1 {
            duplicated.to_le_bytes()
        } else {
            duplicated.to_be_bytes()
        };
        let mut changed = bytes.clone();
        changed[origin.span.offset..origin.span.offset + 4].copy_from_slice(&encoded);
        assert!(
            decompile(&changed, options(NamingMode::Simple)).is_err(),
            "temporary lookup cannot be emitted twice"
        );

        let first = raw.main.common.children[index].common.instructions[0]
            .origin
            .span
            .offset;
        let max_stack_offset = first - usize::from(bytes[7]) - 1;
        assert_eq!(bytes[max_stack_offset], proto.frame.max_stack_size);
        let mut changed = bytes.clone();
        changed[max_stack_offset] += 1;
        assert!(
            decompile(&changed, options(NamingMode::Simple)).is_err(),
            "unreproduced physical stack cells are not certified"
        );
    }
}
