//! 布尔物化回归同时验证真假结果、覆盖槽、GC 及完整指令序列。
//! 验证显式比较求值方向的常量池、临时槽与运行时事件。
//!
//! 每份原创源码覆盖 strip/调试信息与三种命名，要求获得完整 SourceFrame，
//! 回编译后的全部 LIR、常量字节/数值位模式和 maxstack 相同；运行时另外比较
//! 环境读取、字段读取、调用、GC 与 __eq 参数顺序。例：`0 < oldCall(newString)`
//! 中 0 仍须早于新字符串入池。此模块不读取或执行任何游戏归档。

use super::lua51_roundtrip::{Workspace, compile, tool, with_stdin};
use std::process::Command;
use unluac::decompile::{
    DecompileOptions, GenerateMode, GeneratedChunkKind, NamingMode, decompile,
};
use unluac::parser::RawLiteralConst;
use unluac::transformer::{LowInstr, LoweredProto};

const SOURCE: &str = include_str!("../regress-case/regress_440_relational_source_order.lua");

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
    let result = candidates.next().expect("one relational builder");
    assert!(candidates.next().is_none());
    result
}

fn check(source: &str) {
    let workspace = Workspace::new();
    let expected = with_stdin(Command::new(tool("lua")).arg("-"), source).stdout;
    assert!(String::from_utf8_lossy(&expected).contains("use:live"));
    for strip in [true, false] {
        let bytes = compile(&workspace, source, strip);
        for mode in [
            NamingMode::Simple,
            NamingMode::DebugLike,
            NamingMode::Heuristic,
        ] {
            let result = decompile(&bytes, options(mode)).unwrap_or_else(|error| {
                panic!("strict relational source frame: {error}\n{source}")
            });
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
                .expect("complete relational source-frame metadata");
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
                decompile(&recompiled, options(mode)).expect("relational source-frame roundtrip");
            let before = result.state.lowered.unwrap();
            let after = second.state.lowered.unwrap();
            let before = builder(&before.main);
            let after = builder(&after.main);
            assert_eq!(
                before.instrs, after.instrs,
                "all relational producer order, control, scope and scratch writes"
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
fn regressions_440_literal_and_fresh_call_preserve_constant_order() {
    check(SOURCE);
    for condition in [
        "Frame440Call(left) > 17",
        "Frame440Call(0, \"fresh string\") > 0",
    ] {
        check(&SOURCE.replace("17 < Frame440Call(left)", condition));
    }
    let source = SOURCE.replace(
        "    if 17 < Frame440Call(left) then",
        "    Frame440Call(left)\n    if 0 < Frame440Call(left, \"fresh string\") then",
    );
    check(&source);
}

#[test]
fn regressions_440_two_computed_operands_keep_both_source_directions() {
    for condition in [
        "Frame440Call(left) > Frame440Field.value",
        "Frame440Field.value < Frame440Call(left)",
        "Frame440Call(left) >= Frame440Field.value",
        "Frame440Field.value <= Frame440Call(left)",
    ] {
        check(&SOURCE.replace("17 < Frame440Call(left)", condition));
    }
}

#[test]
fn regressions_440_equality_keeps_metamethod_argument_order() {
    let start = SOURCE.find("current = 18\nFrame440Build(18, 1)").unwrap();
    let source = format!(
        "{}{}",
        &SOURCE[..start],
        r#"
local equality = {
    __eq = function(left, right)
        event("eq:" .. left.role .. ":" .. right.role)
        return left.id == right.id
    end,
    __tostring = function(value) return value.role end,
}
Frame440Build(setmetatable({role = "left", id = 1}, equality), setmetatable({role = "right", id = 1}, equality))
weak = setmetatable({}, {__mode = "v"})
Frame440Build(setmetatable({role = "left", id = 1}, equality), setmetatable({role = "right", id = 2}, equality))
print(table.concat(trace, "|"))
"#
    );
    for condition in [
        "left == Frame440EqCall(right)",
        "Frame440EqCall(left) == right",
        "right == Frame440EqCall(left)",
    ] {
        check(&source.replace("17 < Frame440Call(left)", condition));
    }
}
