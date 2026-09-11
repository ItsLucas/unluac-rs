//! 在完整源码帧内保留开放调用参数，逐段验证唯一且紧邻的 pack 消费。
//!
//! 输入如 `CALL inner -> open; CALL outer args=open -> ignore`，输出为
//! `outer(inner())`；固定前置参数仍占原槽，开放结果不物化成 local 或定宽元组。
//! 这里只收完一条调用语句或其最终固定结果；帧、后续写入和 maxstack 仍由父事务验证。
//! 分支合流、入口 pack、非紧邻消费、开放 RETURN 与中途出现普通指令都保持拒绝。

use super::*;
use crate::hir::common::{HirPackTail, HirValuePack};
use crate::transformer::CallInstr;

pub(super) fn open_call_statement(
    parser: &StatementParser<'_, '_>,
    pc: &mut usize,
    end: usize,
    active: &Frame,
    pending: &mut Vec<HirExpr>,
    call: CallInstr,
) -> Option<expressions::ExpressionEnd> {
    let base = active.len();
    let first = call.callee.index().checked_sub(base)?;
    let ValuePack::Fixed(args) = call.args else {
        return None;
    };
    if call.kind != CallKind::Normal
        || call.method_name.is_some()
        || call.results != ResultPack::Open(call.callee)
        || first == 0
        || args.start.index() != call.callee.index() + 1
        || first.checked_add(args.len)?.checked_add(1)? != pending.len()
    {
        return None;
    }
    let block = parser.lowering.cfg.instr_to_block[*pc];
    let mut values = pending.clone();
    let mut arguments = values.split_off(first).into_iter();
    let mut tail = HirCallExpr {
        callee: arguments.next()?,
        args: arguments.collect::<Vec<_>>().into(),
        method: false,
        fastcall: None,
        method_name: None,
    };
    let mut producer = *pc;
    let mut result_start = call.callee;
    for _ in 0..MAX_DEPTH {
        let consumer = producer.checked_add(1)?;
        if consumer >= end || parser.lowering.cfg.instr_to_block.get(consumer) != Some(&block) {
            return None;
        }
        let LowInstr::Call(call) = *parser.lowering.proto.instrs.get(consumer)? else {
            return None;
        };
        if call.kind != CallKind::Normal
            || call.method_name.is_some()
            || call.args != ValuePack::Open(Reg(call.callee.index() + 1))
            || base.checked_add(values.len())? != result_start.index()
            || call.callee.index() >= result_start.index()
        {
            return None;
        }
        let sources = parser
            .lowering
            .dataflow
            .open_use_sources_at(InstrRef(consumer));
        if sources.has_entry() || sources.defs().len() != 1 {
            return None;
        }
        let source = *sources.defs().iter().next()?;
        let definition = &parser.lowering.dataflow.open_defs[source.index()];
        if definition.instr != InstrRef(producer)
            || definition.start_reg != result_start
            || definition.block != block
            || !parser.lowering.owns_open_pack(source, InstrRef(consumer))
        {
            return None;
        }
        let first = call.callee.index().checked_sub(base)?;
        if first >= values.len() {
            return None;
        }
        // 剩余槽必须恰好止于开放 producer 的首结果槽。outer 的 callee 和固定
        // 参数从这同一栈弹出，不能重排参数或把未知结果扩为静态寄存器数量。
        let mut arguments = values.split_off(first).into_iter();
        let expression = HirCallExpr {
            callee: arguments.next()?,
            args: HirValuePack::expanding(
                arguments.collect(),
                HirPackTail::open(HirExpr::Call(Box::new(tail))),
            ),
            method: false,
            fastcall: None,
            method_name: None,
        };
        let outcome = match call.results {
            ResultPack::Ignore if first == 0 => expressions::ExpressionEnd::Ignored(expression),
            ResultPack::Fixed(results)
                if first == 0 && results.start == call.callee && results.len == 1 =>
            {
                expressions::ExpressionEnd::Value {
                    value: HirExpr::Call(Box::new(expression)),
                    definition: consumer,
                }
            }
            ResultPack::Fixed(results)
                if first == 0 && results.start == call.callee && results.len == 3 =>
            {
                expressions::ExpressionEnd::Iterator(expression)
            }
            ResultPack::Fixed(results)
                if first == 0 && results.start == call.callee && results.len > 1 =>
            {
                expressions::ExpressionEnd::FixedResults {
                    call: expression,
                    definition: consumer,
                    width: results.len,
                }
            }
            ResultPack::Open(start) if start == call.callee => {
                producer = consumer;
                result_start = start;
                tail = expression;
                continue;
            }
            _ => return None,
        };
        *pc = consumer + 1;
        *pending = values;
        return Some(outcome);
    }
    None
}
