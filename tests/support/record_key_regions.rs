//! 数值记录键恢复同时校验执行、常量顺序和完整原始寄存器写入轨迹。
//! 重复键是有序字段写，不可按最终键集减少分配或删除覆盖前的对象。
use super::*;

const SOURCE: &str = include_str!("../regress-case/regress_419_numeric_record_keys.lua");

fn whole_builder(proto: &LoweredProto) -> Option<&[LowInstr]> {
    Some(&proto.instrs)
}

fn check(workspace: &Workspace, source: &str) {
    assert_roundtrip_trace(workspace, source, whole_builder);
}

fn source_with_prefix(prefix: usize, reuse: bool) -> String {
    let mut loads = String::new();
    if reuse {
        for number in ["10", "20", "-10", "0.25", "1", "2", "3"] {
            writeln!(loads, "Numeric419Load({number})").unwrap();
        }
    }
    for index in 0..prefix {
        writeln!(loads, "Numeric419Load(\"pool419_{index}\")").unwrap();
    }
    SOURCE.replace("Numeric419Load(\"prefix\")", &loads)
}

#[test]
fn regressions_419_numeric_keys_keep_hash_layout_and_nested_calls() {
    check(&Workspace::new(), SOURCE);
}

#[test]
fn regressions_419_numeric_key_pool_crossing_preserves_original_slots() {
    let workspace = Workspace::new();
    for prefix in [240, 248, 250, 252, 254, 255, 256, 270] {
        for reuse in [false, true] {
            check(&workspace, &source_with_prefix(prefix, reuse));
        }
    }
}

#[test]
fn regressions_419_repeated_keys_preserve_all_writes_and_allocations() {
    let workspace = Workspace::new();
    for prefix in [0, 248, 270] {
        let source = source_with_prefix(prefix, true)
            .replace(
                "        [20] =",
                "        [10] = {n = 10, tShow = {{text = false, ids = {Numeric419Nil(), 10, 11}}}},\n        [20] =",
            )
            .replace(
                "assert(Numeric419Root[10].tShow[1].text.index == 1)",
                "assert(Numeric419Root[10].tShow[1].text == false and Numeric419Root[10].n == 10)",
            )
            .replace("        label = \"ready\",", "        label = false,\n        label = \"ready\",");
        check(&workspace, &source);
    }
}

#[test]
fn regressions_419_environment_reads_keep_numeric_key_scratch_lifetimes() {
    let workspace = Workspace::new();
    for prefix in [0, 270] {
        let source = source_with_prefix(prefix, true).replace(
            "\nNumeric419Root = \"old\"",
            r#"
setfenv(Numeric419Build, setmetatable({}, {
    __index = function(_, key)
        collectgarbage("collect")
        if key ~= "Numeric419Load" then
            trace[#trace + 1] = "get:" .. key .. ":" .. (weak[calls] and "live" or "dead")
        end
        return _G[key]
    end,
    __newindex = function(_, key, value)
        collectgarbage("collect")
        trace[#trace + 1] = "store:" .. key .. ":" .. (weak[calls] and "live" or "dead")
        _G[key] = value
    end,
}))
Numeric419Root = "old""#,
        );
        check(&workspace, &source);
    }
}

#[test]
fn regressions_419_only_the_immediate_parent_key_proves_nested_pool_exhaustion() {
    let workspace = Workspace::new();
    for strip in [true, false] {
        let bytes = compile(&workspace, &source_with_prefix(240, false), strip);
        let result = decompile(&bytes, options(NamingMode::Simple)).unwrap();
        let lowered = result.state.lowered.unwrap();
        let index = lowered
            .main
            .children
            .iter()
            .position(|proto| prefix_and_region(proto).is_some())
            .unwrap();
        let proto = &lowered.main.children[index];
        let (pc, store) = proto
            .instrs
            .iter()
            .enumerate()
            .find_map(|(pc, instruction)| match instruction {
                LowInstr::SetTable(store)
                    if matches!(store.key, AccessKey::Const(key) if key.index() == 255) =>
                {
                    Some((pc, store))
                }
                _ => None,
            })
            .unwrap();
        assert!(proto.constants.common.literals.len() > 256);
        let previous_key = proto.instrs[..pc]
            .iter()
            .rev()
            .find_map(|instruction| match instruction {
                LowInstr::SetTable(previous) if previous.base == store.base => {
                    let AccessKey::Const(key) = previous.key else {
                        return None;
                    };
                    Some(key.index())
                }
                _ => None,
            })
            .unwrap();
        assert!(previous_key < 255);
        let raw = result.state.raw_chunk.unwrap();
        let origin = raw.main.common.children[index].common.instructions[pc].origin;
        let word = u32::try_from(origin.raw_word.unwrap()).unwrap();
        let patched = (word & !(0x1ff << 23)) | (((previous_key as u32) | 0x100) << 23);
        assert_rejected_record_word(&bytes, origin.span.offset, patched);
    }
}

#[test]
fn regressions_419_nonfinite_record_values_do_not_reproduce_literal_slot_writes() {
    let workspace = Workspace::new();
    for prefix in [0, 270] {
        for value in ["1e999", "-1e999"] {
            for expression in [value.to_owned(), format!("Numeric419Operand + {value}")] {
                let source = source_with_prefix(prefix, false).replacen(
                    "n = 1,",
                    &format!("n = {expression},"),
                    1,
                );
                for strip in [true, false] {
                    let bytes = compile(&workspace, &source, strip);
                    let error = decompile(&bytes, options(NamingMode::Simple)).expect_err(
                        "nonfinite source spelling adds binary instructions and scratch writes",
                    );
                    assert!(error.to_string().contains("residual"), "{error}");
                }
            }
        }
    }
}

#[test]
fn regressions_419_nonfinite_record_values_cannot_erase_a_live_operand_root() {
    let workspace = Workspace::new();
    let mut loads = String::new();
    for index in 0..270 {
        writeln!(loads, "InfReviewLoad(\"pool_{index}\")").unwrap();
    }
    let source = r#"
local weak = setmetatable({}, {__mode = "v"})
local operand = {__add = function() return 3 end}
InfReviewCatalog = setmetatable({}, {__index = function(_, key)
    local value = setmetatable({}, operand)
    weak[key] = value
    return value
end})
function InfReviewLoad(value) return {}, {} end
function InfReviewProbe() return 9 end
function InfReviewBuild()
    PREFIX
    InfReviewRoot = {
        sum = InfReviewCatalog.left + InfReviewCatalog.right,
        n = 1e999,
        probe = InfReviewProbe(),
        rows = {{1, 2}, {3, 4}},
    }
end
setfenv(InfReviewBuild, setmetatable({}, {
    __index = function(_, key)
        if key == "InfReviewProbe" then
            collectgarbage("collect")
            print(weak.right and "live" or "dead")
        end
        return _G[key]
    end,
    __newindex = function(_, key, value) _G[key] = value end,
}))
InfReviewBuild()
assert(InfReviewRoot.sum == 3 and InfReviewRoot.probe == 9 and InfReviewRoot.n == math.huge)
"#
    .replace("PREFIX", &loads);
    assert_eq!(
        with_stdin(Command::new(tool("lua")).arg("-"), &source).stdout,
        b"live\n",
    );
    for strip in [true, false] {
        let bytes = compile(&workspace, &source, strip);
        decompile(&bytes, options(NamingMode::Simple))
            .expect_err("spelling infinity as division overwrites the live right-operand slot");
    }
}
