//! 完整语句恢复必须同时通过源码执行、重编译槽轨迹和未证明输入拒绝。
use std::process::Command;

use super::lua51_roundtrip::{Workspace, compile, tool, with_stdin};
use unluac::decompile::{DecompileOptions, GeneratedChunkKind, NamingMode, decompile};
use unluac::parser::RawLiteralConst;
use unluac::transformer::{LowInstr, LoweredProto};

const SOURCE: &str = include_str!("../regress-case/regress_422_statement_frame.lua");

fn options(mode: NamingMode) -> DecompileOptions {
    let mut options = DecompileOptions::default();
    options.naming.mode = mode;
    options
}

fn builder(proto: &LoweredProto) -> &LoweredProto {
    let mut candidates = proto.children.iter().filter(|proto| {
        proto
            .instrs
            .iter()
            .filter(|instruction| matches!(instruction, LowInstr::SetList(_)))
            .count()
            >= 4
    });
    let builder = candidates
        .next()
        .expect("one statement constructor builder");
    assert!(candidates.next().is_none());
    builder
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
            let result = decompile(&bytes, options(mode)).expect("strict statement recovery");
            let generated = result.state.generated.unwrap();
            assert_eq!(generated.kind, GeneratedChunkKind::Source);
            let actual = with_stdin(Command::new(tool("lua")).arg("-"), &generated.source);
            assert_eq!(
                actual.stdout, expected,
                "strip={strip}, mode={mode:?}\n{}",
                generated.source
            );
            let recompiled = compile(&workspace, &generated.source, strip);
            let second = decompile(&recompiled, options(mode)).expect("strict roundtrip");
            let before = result.state.lowered.unwrap();
            let after = second.state.lowered.unwrap();
            let before = builder(&before.main);
            let after = builder(&after.main);
            assert_eq!(
                before.instrs, after.instrs,
                "complete builder register trace\n{}",
                generated.source
            );
            assert_eq!(before.frame.max_stack_size, after.frame.max_stack_size);
            let before = &before.constants.common.literals;
            let after = &after.constants.common.literals;
            assert_eq!(before.len(), after.len());
            for (left, right) in before.iter().zip(after) {
                match (left, right) {
                    (RawLiteralConst::String(left), RawLiteralConst::String(right)) => {
                        assert_eq!(left.bytes, right.bytes);
                    }
                    _ => assert_eq!(left, right),
                }
            }
        }
    }
}

#[test]
fn regressions_422_complete_call_and_array_keep_scratch_overwrites() {
    check(SOURCE);
}

const JOIN: &str = r#"
local weak = setmetatable({}, {__mode = "v"})
local events = {}
function Statement422Produce(a, b)
    local object = {}
    weak.object = object
    return object
end
function Statement422Consume(...) end
function Statement422Build(flag)
    if flag then
        Statement422Consume(1, 2, 3, 4, (Statement422Produce(3, 4)))
    else
        Statement422Consume(1, 2, 3, 4, 99)
    end
    local rows = {{Statement422EdgeProbe, true}, {2, true}, {3, true}, {4, true}}
    Statement422Consume(Statement422EdgeAfter)
    return rows
end
setfenv(Statement422Build, setmetatable({}, {
    __index = function(_, key)
        if key == "Statement422EdgeProbe" or key == "Statement422EdgeAfter" then
            collectgarbage("collect")
            collectgarbage("collect")
            events[#events + 1] = key .. ":" .. (weak.object and "live" or "dead")
            return 1
        end
        return _G[key]
    end,
}))
collectgarbage("stop")
Statement422Build(true)
Statement422Build(false)
print(table.concat(events, ","))
"#;

#[test]
fn regressions_422_parameter_diamond_keeps_both_incoming_scratch_states() {
    check(JOIN);
}

#[test]
fn regressions_422_parameter_early_return_keeps_the_same_frame() {
    check(&SOURCE.replace(
        "    if flag then\n        Statement422Consume",
        "    if not flag then return end\n    do\n        Statement422Consume",
    ));
}

#[test]
fn regressions_422_constructor_declarations_keep_their_source_slots() {
    check(&SOURCE.replace(
        "        Statement422Consume(keep, 3, 4, 5, 6, {0, 0, 0}, false, true,\n            {{value = Statement422Produce(1, 2), font = 13}}, false)\n",
        "",
    ));
    check(&SOURCE.replace(
        "        local rows =",
        "        local retained = {{21, true}, {22, true}, {23, true}, {24, true}, {25, true}, {26, true}, {27, true}, {28, true}, {29, true}, {30, true}, {31, true}}\n        Statement422Observe(retained, keep)\n        local rows =",
    ).replace(
        "dead:11:2:true:42,dead:11:2:true:43",
        "dead:11:2:true:42,dead:11:2:true:42,dead:11:2:true:43,dead:11:2:true:43",
    ));
}

#[test]
fn regressions_422_unproved_while_scope_stays_rejected() {
    let workspace = Workspace::new();
    for source in [SOURCE.replace(
        "function Statement422Build(flag, keep)\n",
        "function Statement422Build(flag, keep)\n    while keep < 0 do keep = keep + 1 end\n",
    )] {
        for strip in [true, false] {
            let bytes = compile(&workspace, &source, strip);
            let error = decompile(&bytes, options(NamingMode::Simple))
                .expect_err("unproved frame or open pack remains rejected");
            assert!(error.to_string().contains("residual"), "{error}");
        }
    }
}

#[test]
fn regressions_422_complete_open_argument_call_keeps_its_parameter_frame() {
    check(&JOIN.replace("(Statement422Produce(3, 4))", "Statement422Produce(3, 4)"));
    check(&JOIN.replace("    if flag then", "    if not flag then"));
    check(&SOURCE.replace("function Statement422Build(flag, keep)\n", "function Statement422Build(flag, keep)\n    local prior = Statement422Produce(1, 2)\n    prior = Statement422Produce(2, 3)\n"));
}

#[test]
fn regressions_422_wrong_scratch_writes_and_call_widths_stay_rejected() {
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
            proto
                .instrs
                .iter()
                .position(|instruction| {
                    matches!(instruction,
                    LowInstr::LoadBool(load) if load.dst.index() == 13)
                })
                .map(|pc| (pc, 6, 8, 14)),
            proto
                .instrs
                .iter()
                .position(|instruction| {
                    matches!(instruction,
                    LowInstr::Call(call) if matches!(call.args,
                        unluac::transformer::ValuePack::Fixed(args) if args.len == 10))
                })
                .map(|pc| (pc, 23, 9, 10)),
        ];
        for target in targets {
            let (pc, shift, width, value) = target.expect("the complete statement has the target");
            let [raw_pc] = proto.lowering_map.low_to_raw[pc].as_slice() else {
                panic!("target must be one raw instruction");
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
            let error = decompile(&changed, options(NamingMode::Simple))
                .expect_err("changed scratch destination or call width is not certified");
            assert!(error.to_string().contains("residual"), "{error}");
        }
    }
}
