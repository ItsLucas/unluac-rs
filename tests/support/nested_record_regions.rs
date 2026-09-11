//! 嵌套 record 的回归同时检查运行结果、完整寄存器轨迹和常量池顺序。
use super::*;

const SOURCE: &str = include_str!("../regress-case/regress_418_nested_record_tables.lua");

fn whole_builder(proto: &LoweredProto) -> Option<&[LowInstr]> {
    Some(&proto.instrs)
}

fn check(workspace: &Workspace, source: &str) {
    assert_roundtrip_trace(workspace, source, whole_builder);
}

#[test]
fn regressions_418_nested_records_preserve_gc_and_exception_order() {
    check(&Workspace::new(), SOURCE);
}

fn source_with_batch(items: usize) -> String {
    let mut batch = "batch = {\n".to_owned();
    for item in 1..=items {
        writeln!(
            batch,
            "{{first = {{value = {item}}}, second = {{{item}, {}}}}},",
            item + 1,
        )
        .unwrap();
    }
    batch.push_str("},\n        rows = {");
    SOURCE.replace("rows = {", &batch).replace(
        "assert(next(Nested418Root.empty)",
        &format!(
            "assert(#Nested418Root.batch == {items})\n\
             for i, item in ipairs(Nested418Root.batch) do\n\
             assert(item.first.value == i and item.second[2] == i + 1)\nend\n\
             assert(next(Nested418Root.empty)"
        ),
    )
}

#[test]
fn regressions_418_nested_batches_close_at_parent_record_stores() {
    let workspace = Workspace::new();
    for items in [0, 1, 49, 50, 51, 100, 101] {
        check(&workspace, &source_with_batch(items));
    }
}

#[test]
fn regressions_418_root_and_nested_rounded_record_allocations() {
    let workspace = Workspace::new();
    for fields in [17, 18] {
        let mut root = "Nested418Root = {\n".to_owned();
        for field in 3..fields {
            writeln!(root, "root{field} = {field},").unwrap();
        }
        let mut nested = "record = {\n".to_owned();
        for field in 2..fields {
            writeln!(nested, "nested{field} = {field},").unwrap();
        }
        check(
            &workspace,
            &SOURCE
                .replace("Nested418Root = {", &root)
                .replace("record = {", &nested),
        );
    }
}

#[test]
fn regressions_418_large_nested_keys_and_call_arguments() {
    let workspace = Workspace::new();
    for prefix in [240, 248, 256, 270] {
        let mut loads = String::new();
        for value in 0..prefix {
            writeln!(loads, "Nested418Load(\"pool418_{value}\")").unwrap();
        }
        check(
            &workspace,
            &SOURCE.replace("Nested418Load(\"prefix\")", &loads),
        );
    }
}

#[test]
fn regressions_418_captured_record_and_empty_global_roots() {
    let workspace = Workspace::new();
    let captured = SOURCE
        .replace("    Nested418Seed = {empty = {}, data = {0, 1}}\n", "")
        .replace("assert(next(Nested418Seed.empty) == nil and Nested418Seed.data[2] == 1)\n", "")
        .replace("Nested418Root = {", "local root = {")
        .replace(
            "    Nested418After = {field = {items = {1, 2}, empty = {}}, value = Nested418Call(3, \"third\")}",
            "    function Nested418Read() return root end\n    Nested418Root = root",
        )
        .replace(
            "assert(Nested418After.value.index == 3 and Nested418After.field.items[2] == 2)",
            "assert(Nested418Read() == Nested418Root)",
        )
        .replace("for fail = 1, 3 do", "for fail = 1, 2 do");
    check(&workspace, &captured);
    let empty = SOURCE
        .replace(
            "Nested418Seed = {empty = {}, data = {0, 1}}",
            "Nested418Seed = {}",
        )
        .replace(
            "assert(next(Nested418Seed.empty) == nil and Nested418Seed.data[2] == 1)",
            "assert(next(Nested418Seed) == nil)",
        );
    check(&workspace, &empty);
    check(
        &workspace,
        r#"
function Nested418Load() return {}, {} end
function Nested418Build()
    Nested418Load()
    local root = {}
    function Nested418Read() return root end
    Nested418Root = root
end
Nested418Build()
assert(Nested418Read() == Nested418Root and next(Nested418Root) == nil)
print(type(Nested418Read()), #Nested418Root)
"#,
    );
}

#[test]
fn regressions_418_environment_lookups_preserve_scratch_lifetimes() {
    let source = SOURCE.replace(
        "\nNested418Root = \"old\"",
        r#"
setfenv(Nested418Build, setmetatable({}, {
    __index = function(_, key)
        collectgarbage("collect")
        trace[#trace + 1] = "get:" .. key .. ":" .. (weak.scratch and "live" or "dead")
        return _G[key]
    end,
    __newindex = function(_, key, value)
        collectgarbage("collect")
        trace[#trace + 1] = "store:" .. key .. ":" .. (weak.scratch and "live" or "dead")
        _G[key] = value
    end,
}))
Nested418Root = "old""#,
    );
    check(&Workspace::new(), &source);
}

#[test]
fn regressions_418_lookup_arithmetic_and_nested_sibling_boundaries() {
    let source = SOURCE
        .replace(
            "{info = {name = Nested418Call(1, \"first\"), code = 11}",
            "{info = {name = Nested418Call(1, \"first\"), code = 11}, ids = {{1, 2, 3}, {4, 5, 6}}",
        )
        .replace(
            "first = Nested418Catalog.group.a",
            "first = 2 * Nested418Catalog.group.a * 3",
        )
        .replace(
            "Nested418Root.record.first == 1",
            "Nested418Root.record.first == 6",
        );
    check(&Workspace::new(), &source);
}

#[test]
fn regressions_418_binary_rhs_roots_survive_only_original_stack_writes() {
    let workspace = Workspace::new();
    for prefix in [0, 270] {
        check(&workspace, &binary_rhs_source(prefix));
    }
}

#[test]
fn regressions_418_materialized_numeric_rhs_preserves_gc_roots() {
    let source = binary_rhs_source(270).replace(
        "last = Nested418Catalog.last + Nested418Catalog.last",
        "last = Nested418Catalog.last + 1",
    );
    check(&Workspace::new(), &source);
}

fn binary_rhs_source(prefix: usize) -> String {
    let mut loads = String::new();
    for value in 0..prefix {
        writeln!(loads, "Nested418Load(\"rhs418_{value}\")").unwrap();
    }
    r#"
local weak = setmetatable({}, {__mode = "v"})
local trace = {}
local operand_mt = {__add = function(left, right) return 3 end}
Nested418Catalog = setmetatable({}, {__index = function(_, key)
    collectgarbage("collect")
    trace[#trace + 1] = "lookup:" .. key .. ":" .. (weak.right and "live" or "dead")
    local value = setmetatable({}, operand_mt)
    weak[key] = value
    return value
end})
function Nested418Load(label) return {}, {} end
function Nested418Probe()
    collectgarbage("collect")
    trace[#trace + 1] = "call:" .. (weak.right and "live" or "dead")
    return 7
end
function Nested418Build()
    Nested418Load("prefix")
    Nested418Root = {
        value = Nested418Catalog.left + Nested418Catalog.right,
        empty = {},
        child = {value = Nested418Probe()},
        last = Nested418Catalog.last + Nested418Catalog.last,
    }
end
setfenv(Nested418Build, setmetatable({}, {
    __index = function(_, key)
        collectgarbage("collect")
        if key ~= "Nested418Load" then
            trace[#trace + 1] = "env:" .. key .. ":" .. (weak.right and "live" or "dead")
        end
        return _G[key]
    end,
    __newindex = function(_, key, value) _G[key] = value end,
}))
Nested418Build()
assert(Nested418Root.value == 3 and Nested418Root.child.value == 7)
assert(next(Nested418Root.empty) == nil and Nested418Root.last == 3)
print(table.concat(trace, "|"))
"#
    .replace("Nested418Load(\"prefix\")", &loads)
}

fn numeric_arithmetic_source(prefix: usize, value: &str, reused: bool) -> String {
    let mut source = include_str!("../regress-case/regress_412_record_arithmetic.lua").to_owned();
    for (before, operand, operator) in [
        ("2 * Record412Catalog.a * 3", "a", "+"),
        ("(Record412Catalog.b + 4) - 5", "b", "-"),
        ("6 / (Record412Catalog.c % 7)", "c", "*"),
        ("(Record412Catalog.d ^ 2) ^ 3", "d", "/"),
        ("8 - (9 + Record412Catalog.e)", "e", "%"),
        ("2 ^ (Record412Catalog.f / 3)", "f", "^"),
    ] {
        source = source.replace(
            before,
            &format!("Record412Catalog.{operand} {operator} {value}"),
        );
    }
    let mut loads = String::new();
    if reused {
        writeln!(loads, "Nested418Load({value})").unwrap();
    }
    for index in 0..prefix {
        writeln!(loads, "Nested418Load(\"numeric418_{index}\")").unwrap();
    }
    source
        .replace("for stop = 0, 12 do", "for stop = 0, 6 do")
        .replace(
            "trace[#trace + 1] = op .. \":\" .. a .. \":\" .. b",
            "trace[#trace + 1] = op .. \":\" .. a .. \":\" .. b .. \":\" .. (b == 0 and 1 / b or b)",
        )
        .replace(
            "function Record412Build()",
            &format!("function Nested418Load(value) return value end\nfunction Record412Build()\n{loads}"),
        )
}

#[test]
fn regressions_418_numeric_rhs_operators_and_literal_bits() {
    let workspace = Workspace::new();
    for value in ["1", "-0.0", "0.25"] {
        for reused in [false, true] {
            check(&workspace, &numeric_arithmetic_source(270, value, reused));
        }
    }
}

#[test]
fn regressions_418_wrong_numeric_rhs_slots_and_order_stay_rejected() {
    let workspace = Workspace::new();
    for strip in [true, false] {
        let bytes = compile(
            &workspace,
            &numeric_arithmetic_source(270, "1", true),
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
        let (load_pc, load, binary) = proto
            .instrs
            .windows(2)
            .enumerate()
            .find_map(|(pc, pair)| match pair {
                [LowInstr::LoadConst(load), LowInstr::BinaryOp(binary)]
                    if binary.rhs == ValueOperand::Reg(load.dst) =>
                {
                    Some((pc, load, binary))
                }
                _ => None,
            })
            .unwrap();
        let raw = result.state.raw_chunk.unwrap();
        let instructions = &raw.main.common.children[index].common.instructions;
        let load_origin = instructions[load_pc].origin;
        let load_word = u32::try_from(load_origin.raw_word.unwrap()).unwrap();
        let binary_origin = instructions[load_pc + 1].origin;
        let binary_word = u32::try_from(binary_origin.raw_word.unwrap()).unwrap();
        let swapped = (binary_word & !((0x1ff << 23) | (0x1ff << 14)))
            | ((load.dst.index() as u32) << 23)
            | ((binary.dst.index() as u32) << 14);
        let wrong_dst = (binary_word & !(0xff << 6)) | ((load.dst.index() as u32) << 6);
        for (origin, word) in [
            (binary_origin, swapped),
            (binary_origin, wrong_dst),
            (load_origin, load_word + (1 << 6)),
        ] {
            assert_rejected_record_word(&bytes, origin.span.offset, word);
        }
    }
}

#[test]
fn regressions_418_small_pool_does_not_prove_numeric_materialization() {
    let workspace = Workspace::new();
    let source = numeric_arithmetic_source(0, "1", true).replace(
        "Record412Catalog.a + 1",
        "Record412Catalog.a + Nested418Right",
    );
    for strip in [true, false] {
        let bytes = compile(&workspace, &source, strip);
        let result = decompile(&bytes, options(NamingMode::Simple)).unwrap();
        let lowered = result.state.lowered.unwrap();
        let index = lowered
            .main
            .children
            .iter()
            .position(|proto| prefix_and_region(proto).is_some())
            .unwrap();
        let proto = &lowered.main.children[index];
        let constants = &proto.constants.common.literals;
        assert!(constants.len() < 255);
        let number = constants
            .iter()
            .position(|value| matches!(value, RawLiteralConst::Number(value) if *value == 1.0))
            .unwrap();
        let (pc, dst) = proto
            .instrs
            .iter()
            .enumerate()
            .find_map(|(pc, instr)| match instr {
                LowInstr::GetTable(get) if get.base == AccessBase::Env => {
                    let AccessKey::Const(key) = get.key else {
                        return None;
                    };
                    matches!(&constants[key.index()], RawLiteralConst::String(value)
                    if value.bytes.as_ref() == b"Nested418Right")
                    .then_some((pc, get.dst))
                }
                _ => None,
            })
            .unwrap();
        let raw = result.state.raw_chunk.unwrap();
        let origin = raw.main.common.children[index].common.instructions[pc].origin;
        let loadk = 1 | ((dst.index() as u32) << 6) | ((number as u32) << 14);
        assert_rejected_record_word(&bytes, origin.span.offset, loadk);
    }
}

#[test]
fn regressions_418_nonfinite_materialized_rhs_stays_unproved() {
    let workspace = Workspace::new();
    for strip in [true, false] {
        let bytes = compile(
            &workspace,
            &numeric_arithmetic_source(270, "12345.25", false),
            strip,
        );
        let finite = if bytes[6] == 1 {
            12345.25f64.to_le_bytes()
        } else {
            12345.25f64.to_be_bytes()
        };
        let offsets = bytes
            .windows(8)
            .enumerate()
            .filter_map(|(offset, value)| (value == finite).then_some(offset))
            .collect::<Vec<_>>();
        assert_eq!(offsets.len(), 1, "unique numeric constant payload");
        for value in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            let mut changed = bytes.clone();
            let encoded = if bytes[6] == 1 {
                value.to_le_bytes()
            } else {
                value.to_be_bytes()
            };
            changed[offsets[0]..offsets[0] + 8].copy_from_slice(&encoded);
            let error = decompile(&changed, options(NamingMode::Simple))
                .expect_err("nonfinite runtime RHS must not prove the parent array region");
            assert!(error.to_string().contains("residual"), "{error}");
        }
    }
}

#[test]
fn regressions_419_nested_numeric_records_keep_hash_allocation() {
    check(
        &Workspace::new(),
        &SOURCE.replace(
            "numbers = {Nested418Nil(), 2, 3}",
            "numbers = {[1] = 1, [3] = 3}",
        ),
    );
}

#[test]
fn regressions_418_unused_local_prefix_keeps_the_complete_nested_frame() {
    check(
        &Workspace::new(),
        &SOURCE.replace(
            "Nested418Root = {",
            "local previous = Nested418Unknown\n    Nested418Root = {",
        ),
    );
}

#[test]
fn regressions_418_unproved_nested_shapes_stay_rejected() {
    let workspace = Workspace::new();
    for (case, source) in [
        SOURCE.replace(
            "numbers = {Nested418Nil(), 2, 3}",
            "numbers = {1, Nested418Nil()}",
        ),
        SOURCE.replace(
            "numbers = {Nested418Nil(), 2, 3}",
            "numbers = {1, 2, flag = 3}",
        ),
        SOURCE.replace(
            "numbers = {Nested418Nil(), 2, 3}",
            "numbers = {[Nested418Unknown] = 1}",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        for strip in [true, false] {
            let bytes = compile(&workspace, &source, strip);
            let error = decompile(&bytes, options(NamingMode::Simple))
                .err()
                .unwrap_or_else(|| panic!("unproved nested shape {case} must stay rejected"));
            assert!(error.to_string().contains("residual"), "{error}");
        }
    }
}

#[test]
fn regressions_433_nested_field_calls_preserve_runtime_arguments_and_arithmetic() {
    let workspace = Workspace::new();
    for replacement in [
        "name = Nested418Call(1, \"first\") + 1",
        "name = Nested418Call(Nested418Unknown, \"first\")",
    ] {
        let changed = SOURCE
            .replace("name = Nested418Call(1, \"first\")", replacement)
            .replace("local value = {index = index, label = label}",
                "local value = setmetatable({index = index, label = label}, {__add = function(left, right) collectgarbage('collect'); trace[#trace + 1] = 'add:' .. right; return left end})");
        check(&workspace, &changed);
    }
}

#[test]
fn regressions_418_wrong_child_destinations_and_allocations_stay_rejected() {
    let workspace = Workspace::new();
    for strip in [true, false] {
        let bytes = compile(&workspace, SOURCE, strip);
        let result = decompile(&bytes, options(NamingMode::Simple)).unwrap();
        let lowered = result.state.lowered.unwrap();
        let index = lowered
            .main
            .children
            .iter()
            .position(|proto| prefix_and_region(proto).is_some())
            .unwrap();
        let proto = &lowered.main.children[index];
        let (child_pc, child) = proto
            .instrs
            .iter()
            .enumerate()
            .find_map(|(pc, instr)| match instr {
                LowInstr::NewTable(table)
                    if table
                        .lua51_allocation
                        .is_some_and(|hint| hint.array_hint > 0 && hint.hash_hint == 0) =>
                {
                    Some((pc, table))
                }
                _ => None,
            })
            .unwrap();
        let (store_pc, store) = proto.instrs[child_pc + 1..]
            .iter()
            .enumerate()
            .find_map(|(pc, instr)| match instr {
                LowInstr::SetTable(store)
                    if store.value == ValueOperand::Reg(child.dst)
                        && matches!(store.base, AccessBase::Reg(_)) =>
                {
                    Some((pc + child_pc + 1, store))
                }
                _ => None,
            })
            .unwrap();
        let AccessBase::Reg(parent) = store.base else {
            unreachable!()
        };
        let raw = result.state.raw_chunk.unwrap();
        let instructions = &raw.main.common.children[index].common.instructions;
        for (pc, shift, mask, value) in [
            (child_pc, 23, 0x1ff, 3), // 数组分配与两个固定项不符。
            (child_pc, 14, 0x1ff, 1), // 数组子表不能声明 hash 分配。
            (child_pc, 6, 0xff, child.dst.index() as u32 + 1),
            (store_pc, 14, 0x1ff, parent.index() as u32), // 写回值误用父表。
            (store_pc, 6, 0xff, child.dst.index() as u32), // 写回目标误用子表。
        ] {
            let origin = instructions[pc].origin;
            let word = u32::try_from(origin.raw_word.unwrap()).unwrap();
            let changed = (word & !(mask << shift)) | (value << shift);
            let error = rejected_record_word(&bytes, origin.span.offset, changed);
            assert!(error.contains("residual"), "pc={pc}: {error}");
        }
    }
}
