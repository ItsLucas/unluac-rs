//! 字段栈扩展验证完整寄存器轨迹、六种元方法、回调更新低槽和GC／异常时点。
use super::*;

const SOURCE: &str = include_str!("../regress-case/regress_433_record_call_arithmetic.lua");
const UPVALUE_ITEMS: &str = include_str!("../regress-case/regress_433_upvalue_array_items.lua");

fn whole_builder(proto: &LoweredProto) -> Option<&[LowInstr]> {
    Some(&proto.instrs)
}

#[test]
fn regressions_433_upvalue_array_items_keep_each_read_and_gc_lifetime() {
    assert_roundtrip_trace(&Workspace::new(), UPVALUE_ITEMS, whole_builder);
}

#[test]
fn regressions_433_record_calls_and_low_slot_arithmetic_preserve_gc_and_mutation() {
    let workspace = Workspace::new();
    for prefix in [0, 248, 270] {
        let mut loads = String::new();
        for index in 0..prefix {
            writeln!(loads, "Fields433Load(\"pool433_{index}\")").unwrap();
        }
        let source = SOURCE.replace("Fields433Load(\"prefix\")", &loads);
        assert_roundtrip_trace(&workspace, &source, whole_builder);
    }
}

#[test]
fn regressions_433_record_expression_slots_and_call_width_stay_proven() {
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
        let root = proto
            .instrs
            .iter()
            .find_map(|instruction| match instruction {
                LowInstr::NewTable(table) => Some(table.dst),
                _ => None,
            })
            .unwrap();
        let (binary_pc, _) = proto
            .instrs
            .iter()
            .enumerate()
            .find(|(_, instruction)| {
                matches!(instruction, LowInstr::BinaryOp(binary)
                if matches!(binary.lhs, ValueOperand::Reg(reg) if reg.index() < root.index())
                    && matches!(binary.rhs, ValueOperand::Reg(reg) if reg.index() < root.index()))
            })
            .unwrap();
        let (call_pc, call) = proto
            .instrs
            .iter()
            .enumerate()
            .skip(binary_pc)
            .find_map(|(pc, instruction)| match instruction {
                LowInstr::Call(call) => Some((pc, call)),
                _ => None,
            })
            .unwrap();
        let (copied_call_pc, copied_call) = proto
            .instrs
            .windows(3)
            .enumerate()
            .find_map(|(pc, instructions)| match instructions {
                [
                    LowInstr::Move(callee),
                    LowInstr::Move(argument),
                    LowInstr::Call(call),
                ] if call.callee == callee.dst
                    && argument.dst.index() == callee.dst.index() + 1 =>
                {
                    Some((pc + 2, call))
                }
                _ => None,
            })
            .unwrap();
        let raw = result.state.raw_chunk.unwrap();
        for (pc, shift, mask, value) in [
            (binary_pc, 23, 0x1ff, root.index() as u32),
            (call_pc, 14, 0x1ff, 0),
            (call_pc, 14, 0x1ff, 3),
        ] {
            let raw_index = proto.lowering_map.low_to_raw[pc][0].index();
            let origin = raw.main.common.children[index].common.instructions[raw_index].origin;
            let word = u32::try_from(origin.raw_word.unwrap()).unwrap();
            let patched = (word & !(mask << shift)) | (value << shift);
            let error = rejected_record_word(&bytes, origin.span.offset, patched);
            assert!(error.contains("residual"), "{error}");
        }
        assert!(call.callee.index() > root.index());
        // MOVE callee; MOVE arg; CALL 不能猜成 callee + arg：后者由低槽直接读取，
        // 会删除这两次原始物理写，即使目标寄存器和最终 SETTABLE 相同也不等价。
        let raw_index = proto.lowering_map.low_to_raw[copied_call_pc][0].index();
        let origin = raw.main.common.children[index].common.instructions[raw_index].origin;
        let callee = copied_call.callee.index() as u32;
        let patched = 12 | (callee << 6) | ((callee + 1) << 14) | (callee << 23);
        let error = rejected_record_word(&bytes, origin.span.offset, patched);
        assert!(error.contains("residual"), "{error}");
    }
}
