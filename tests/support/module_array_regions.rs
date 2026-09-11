//! 验证模块级连续构造的完整帧、字段调用、异常与 GC 时点；未证明的前后缀严格拒绝。
use super::*;

fn whole_proto(proto: &LoweredProto) -> Option<&[LowInstr]> {
    Some(&proto.instrs)
}

#[test]
fn regressions_417_closed_initializers_and_record_calls() {
    let workspace = Workspace::new();
    assert_roundtrip_trace(
        &workspace,
        include_str!("../regress-case/regress_417_module_record_calls.lua"),
        whole_proto,
    );
}

fn module_source(rows: usize, prefix: usize) -> String {
    let template = include_str!("../regress-case/regress_417_module_record_calls.lua");
    let begin = template.find("-- region417 rows begin").unwrap();
    let end = template.find("-- region417 rows end").unwrap();
    let mut source = template[..begin].to_owned();
    for row in 1..=rows {
        writeln!(
            source,
            "{{label = Module417Text({row}, \"first\"), value = {}}},",
            row + 10
        )
        .unwrap();
    }
    source.push_str(&template[end..]);
    source = source
        .replace(
            "for fail = 1, 4 do",
            &format!(
                "for _, fail in ipairs({{1, 2, {rows}, {}, {}}}) do",
                rows + 1,
                rows + 2
            ),
        )
        .replace("if fail <= 2 then", &format!("if fail <= {rows} then"));
    if prefix > 0 {
        let mut calls = String::new();
        for n in 0..prefix {
            writeln!(calls, "Module417Load(\"pool417_{n}\")").unwrap();
        }
        source = source.replace("Module417Load(\"prefix\")", &calls);
    }
    source
}

#[test]
fn regressions_417_batches_and_call_argument_constant_pool() {
    let workspace = Workspace::new();
    for rows in [49, 50, 51, 100, 101] {
        assert_roundtrip_trace(&workspace, &module_source(rows, 0), whole_proto);
    }
    for prefix in [240, 248, 250, 252, 254, 256, 270] {
        assert_roundtrip_trace(&workspace, &module_source(2, prefix), whole_proto);
    }
}

#[test]
fn regressions_417_nil_results_and_environment_effects() {
    let workspace = Workspace::new();
    let source = include_str!("../regress-case/regress_417_module_record_calls.lua");
    let no_arguments = source
        .replace(
            "function Module417Build()",
            "function Module417Zero() return Module417Text(1, \"first\") end\nfunction Module417Build()",
        )
        .replace(
            "label = Module417Text(1, \"first\")",
            "label = Module417Zero()",
        );
    assert_roundtrip_trace(&workspace, &no_arguments, whole_proto);
    let nil_result = source
        .replace(
            "return result, \"discarded\", nil",
            "if index == 2 then return nil, \"discarded\" end\n    return result, \"discarded\", nil",
        )
        .replace(
            "assert(Module417Groups",
            "assert(Module417Rows[2].label == nil)\nassert(Module417Groups",
        );
    assert_roundtrip_trace(&workspace, &nil_result, whole_proto);
    let environment = source.replace(
        "\nModule417Rows = \"old\"",
        r#"
setfenv(Module417Build, setmetatable({}, {
    __index = function(_, key)
        collectgarbage("collect")
        trace[#trace + 1] = "get:" .. key
        return _G[key]
    end,
    __newindex = function(_, key, value)
        trace[#trace + 1] = "store:" .. key
        _G[key] = value
    end,
}))
Module417Rows = "old""#,
    );
    assert_roundtrip_trace(&workspace, &environment, whole_proto);
}

#[test]
fn regressions_417_unused_local_prefixes_keep_their_slots_and_gc_lifetimes() {
    let workspace = Workspace::new();
    let source = include_str!("../regress-case/regress_417_module_record_calls.lua");
    for changed in [
        source
            .replace("Module417Seed = ", "local previous = ")
            .replace(
                "assert(#Module417Seed == 2 and Module417Seed[2].b == 3)",
                "assert(Module417Seed == nil)",
            ),
        source.replace(
            "Module417Rows = {",
            "local previous = Module417Unknown\n    Module417Rows = {",
        ),
    ] {
        assert_roundtrip_trace(&workspace, &changed, whole_proto);
    }
}

#[test]
fn regressions_417_unproved_prefixes_and_calls_stay_rejected() {
    let workspace = Workspace::new();
    let source = include_str!("../regress-case/regress_417_module_record_calls.lua");
    for changed in [
        source.replace(
            "Module417Text(1, \"first\")",
            "Module417Text(Module417More())",
        ),
        source.replace(
            "Module417Text(1, \"first\")",
            "Module417Text(nil, \"first\")",
        ),
    ] {
        for strip in [true, false] {
            let bytes = compile(&workspace, &changed, strip);
            let Err(error) = decompile(&bytes, options(NamingMode::Simple)) else {
                panic!("unproved prefix or noncanonical field call must stay rejected");
            };
            assert!(
                error.to_string().contains("residual table-set-list"),
                "{error}"
            );
        }
    }
}

#[test]
fn regressions_433_module_field_calls_preserve_chains_arguments_and_arithmetic() {
    let workspace = Workspace::new();
    let source = include_str!("../regress-case/regress_417_module_record_calls.lua");
    for replacement in [
        "Module417Text(Module417Unknown, \"first\")",
        "Module417Text(1, \"first\") + 1",
        "Module417Library.text(1, \"first\")",
    ] {
        let changed = source
            .replace("Module417Text(1, \"first\")", replacement)
            .replace("function Module417Build()", "Module417Library = {text = Module417Text}\nfunction Module417Build()")
            .replace("local result = {index = index, label = label}",
                "local result = setmetatable({index = index, label = label}, {__add = function(left, right) collectgarbage('collect'); trace[#trace + 1] = 'add:' .. right; return left end})");
        assert_roundtrip_trace(&workspace, &changed, whole_proto);
    }
}

#[test]
fn regressions_417_malformed_calls_and_closed_prefixes_stay_rejected() {
    let workspace = Workspace::new();
    for strip in [true, false] {
        let bytes = compile(
            &workspace,
            include_str!("../regress-case/regress_417_module_record_calls.lua"),
            strip,
        );
        let result = decompile(&bytes, options(NamingMode::Simple)).unwrap();
        let lowered = result.state.lowered.unwrap();
        let index = lowered
            .main
            .children
            .iter()
            .position(|proto| prefix_and_region(proto).is_some())
            .unwrap();
        let proto = &lowered.main.children[index];
        let call = proto.instrs.iter().position(|instr| matches!(instr,
            LowInstr::Call(call) if matches!(call.results, unluac::transformer::ResultPack::Fixed(_))))
            .unwrap();
        let seed = proto
            .instrs
            .iter()
            .position(|instr| matches!(instr, LowInstr::NewTable(_)))
            .unwrap();
        let store = proto
            .instrs
            .iter()
            .position(|instr| {
                matches!(instr,
            LowInstr::SetTable(store) if store.base == AccessBase::Env)
            })
            .unwrap();
        let raw = result.state.raw_chunk.unwrap();
        let instructions = &raw.main.common.children[index].common.instructions;
        for (pc, shift, mask, value) in [
            (call, 14, 0x1ff, 0), // open results
            (call, 14, 0x1ff, 3), // two results
            (call, 23, 0x1ff, 0), // open arguments
            (seed, 14, 0x1ff, 1), // wrong prefix allocation
            (store, 6, 0xff, 1),  // the prefix stores a child instead of its root
        ] {
            let origin = instructions[pc].origin;
            let word = u32::try_from(origin.raw_word.unwrap()).unwrap();
            let changed = (word & !(mask << shift)) | (value << shift);
            let error = rejected_record_word(&bytes, origin.span.offset, changed);
            assert!(error.contains("residual"), "pc={pc}: {error}");
        }
    }
}
