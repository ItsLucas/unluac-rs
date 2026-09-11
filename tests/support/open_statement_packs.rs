//! 开放结果必须保持真实宽度、空洞、GC根和调用栈，不能只比较合法源码。
use std::process::Command;

use super::lua51_roundtrip::{Workspace, compile, tool, with_stdin};
use unluac::decompile::{
    DecompileOptions, GenerateMode, GeneratedChunkKind, NamingMode, decompile,
};
use unluac::parser::RawLiteralConst;
use unluac::transformer::{LowInstr, LoweredProto, ResultPack, ValuePack};

const SOURCE: &str = include_str!("../regress-case/regress_431_open_statement_packs.lua");

fn builders(proto: &LoweredProto) -> Vec<&LoweredProto> {
    let mut result: Vec<_> = proto
        .children
        .iter()
        .filter(|proto| {
            proto
                .instrs
                .iter()
                .any(|instruction| matches!(instruction, LowInstr::SetList(_)))
        })
        .map(|proto| proto.as_ref())
        .collect();
    result.sort_by_key(|proto| proto.signature.num_params);
    assert!(result.len() >= 2, "return and dispatch builders");
    result
}

fn options(mode: NamingMode) -> DecompileOptions {
    let mut options = DecompileOptions::default();
    options.generate.mode = GenerateMode::Strict;
    options.naming.mode = mode;
    options
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
            let result = decompile(&bytes, options(mode)).expect("strict open statement frame");
            let generated = result.state.generated.unwrap();
            assert_eq!(generated.kind, GeneratedChunkKind::Source);
            let actual = with_stdin(Command::new(tool("lua")).arg("-"), &generated.source);
            assert_eq!(actual.stdout, expected, "strip={strip}, mode={mode:?}");
            let recompiled = compile(&workspace, &generated.source, strip);
            let second = decompile(&recompiled, options(mode)).expect("strict open-pack roundtrip");
            let before = result.state.lowered.unwrap();
            let after = second.state.lowered.unwrap();
            let before = builders(&before.main);
            let after = builders(&after.main);
            assert_eq!(before.len(), after.len());
            for (before, after) in before.into_iter().zip(after) {
                assert_eq!(
                    before.instrs, after.instrs,
                    "complete open-pack instruction trace"
                );
                assert_eq!(
                    before.frame, after.frame,
                    "open results depend on exact maxstack"
                );
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
}

#[test]
fn regressions_431_open_table_returns_and_open_call_arguments_preserve_the_frame() {
    check(SOURCE);
}

#[test]
fn regressions_431_open_call_and_table_owners_also_accept_constant_index_consumers() {
    check(&SOURCE.replace("values[actor.force]", "values[1]"));
}

#[test]
fn regressions_431_nested_open_calls_preserve_fixed_argument_prefixes() {
    let source = SOURCE
        .replace("values[actor.force]", "values[1]")
        .replace(
            "function Open431Dispatch(actor, unused1, unused2, unused3)",
            "function Open431Relay(tag, ...)\n    trace[#trace + 1] = \"relay:\" .. tag .. \":\" .. select(\"#\", ...)\n    return ...\nend\nfunction Open431Dispatch(actor, unused1, unused2, unused3)",
        )
        .replace(
            "actor.message(Open431Text(1, 0))",
            "actor.message(\"prefix\", Open431Relay(\"tag\", Open431Text(1, 0)))",
        );
    check(&source);
}

#[test]
fn regressions_431_open_argument_consumer_can_preserve_one_declared_result() {
    let source = SOURCE
        .replace("values[actor.force]", "values[1]")
        .replace(
            "function Open431Dispatch(actor, unused1, unused2, unused3)",
            "function Open431Observe(value)\n    trace[#trace + 1] = \"observe:\" .. tostring(value)\nend\nfunction Open431Dispatch(actor, unused1, unused2, unused3)",
        )
        .replace(
            "actor.message(Open431Text(1, 0))",
            "local result = actor.message(Open431Text(1, 0))\n        Open431Observe(result)",
        );
    check(&source);
}

#[test]
fn regressions_431_open_and_empty_table_arguments_keep_scalar_call_width() {
    for argument in ["{Open431Text(1, 0)}", "{}"] {
        let source = SOURCE.replace("values[actor.force]", "values[1]").replace(
            "actor.message(Open431Text(1, 0))",
            &format!("actor.message({argument})"),
        );
        check(&source);
    }
}

#[test]
fn regressions_431_open_result_widths_cross_the_declared_frame_and_list_batch() {
    check(&SOURCE.replace("for count = 0, 12 do", "for count = 45, 105, 5 do"));
}

#[test]
fn regressions_431_changed_open_call_owners_and_argument_protocol_stay_rejected() {
    let workspace = Workspace::new();
    let source = SOURCE.replace("values[actor.force]", "values[1]");
    for strip in [true, false] {
        let bytes = compile(&workspace, &source, strip);
        let result = decompile(&bytes, options(NamingMode::Simple)).unwrap();
        let lowered = result.state.lowered.unwrap();
        let (index, proto) = lowered
            .main
            .children
            .iter()
            .enumerate()
            .find(|(_, proto)| proto.signature.num_params == 4)
            .unwrap();
        let (pc, producer) = proto
            .instrs
            .windows(2)
            .enumerate()
            .find_map(|(pc, pair)| match pair {
                [LowInstr::Call(producer), LowInstr::Call(consumer)]
                    if matches!(producer.results, ResultPack::Open(_))
                        && matches!(consumer.args, ValuePack::Open(_)) =>
                {
                    Some((pc, producer))
                }
                _ => None,
            })
            .unwrap();
        let raw = result.state.raw_chunk.unwrap();
        for (pc, shift, mask, value) in [
            (pc, 14, 0x1ff, 2),
            (pc + 1, 23, 0x1ff, 1),
            (pc + 1, 6, 0xff, producer.callee.index() as u32),
        ] {
            let raw_pc = proto.lowering_map.low_to_raw[pc][0].index();
            let origin = raw.main.common.children[index].common.instructions[raw_pc].origin;
            let word = u32::try_from(origin.raw_word.unwrap()).unwrap();
            let word = (word & !(mask << shift)) | (value << shift);
            let mut changed = bytes.clone();
            let encoded = if bytes[6] == 1 {
                word.to_le_bytes()
            } else {
                word.to_be_bytes()
            };
            changed[origin.span.offset..origin.span.offset + 4].copy_from_slice(&encoded);
            decompile(&changed, options(NamingMode::Simple))
                .expect_err("changed producer/consumer pack cannot certify the source frame");
        }
    }
}

#[test]
fn regressions_431_captured_open_sequences_keep_unused_prefix_slots_and_upvalues() {
    let source = format!(
        "{}\n{CAPTURED}",
        SOURCE.replace("index == Open431TailIndex", "index % 3 == 0"),
    );
    check(&source);
}

const CAPTURED: &str = r#"
function Open431Captured()
    local tag = 77
    local unused = 99
    local sizes = {1, 2, 3, 4, 5, 6, 7, 8, 9}
    local start = 3
    local minimum = 0
    local maximum = #sizes - 1
    function Open431Prefix() return tag, sizes, start, minimum, maximum end
    local first = {Open431Text(33, 1), Open431Text(33, 2), Open431Text(33, 3)}
    local second = {Open431Text(33, 4), Open431Text(33, 5), Open431Text(33, 6)}
    local third = {Open431Text(33, 7), Open431Text(33, 8), Open431Text(33, 9)}
    function Open431Plain() return 7 end
    function Open431UseFirst() return first end
    function Open431UseRest() return second, third end
end
Open431Fail = nil
for count = 0, 80, 5 do
    Open431Count = count
    trace = {}
    weak = setmetatable({}, {__mode = "v"})
    Open431Captured()
    local tag, sizes, start, minimum, maximum = Open431Prefix()
    assert(tag == 77 and #sizes == 9 and start == 3 and minimum == 0 and maximum == 8)
    local first = Open431UseFirst()
    local second, third = Open431UseRest()
    assert(first[1].index == 1 and first[2] == nil)
    assert(second[1] == nil and second[2].index == 5)
    assert(third[1].index == 7 and third[2] == nil)
    for index = 1, count do
        if index % 3 == 0 then
            assert(first[index + 2] == nil and second[index + 2] == nil and third[index + 2] == nil)
        else
            assert(first[index + 2].index == index and second[index + 2].index == index and third[index + 2].index == index)
            assert(first[index + 2] ~= second[index + 2] and second[index + 2] ~= third[index + 2])
        end
    end
    print("captured", count, #first, #second, #third, table.concat(trace, "|"))
end
"#;
