//! 保留由根数组末次 SETLIST 唯一消费的开放 CALL 结果。
//!
//! 未知宽度结果保持真正的构造器尾项；本模块只证明调用与消费协议，不单独提交改写。
//! 外层帧事务还须证明整个后缀，包括后续局部声明和闭包安装。任何未闭合后缀都会
//! 撤销该开放区间，不能让未知返回槽被普通内联或人为局部改变存活期。

use super::*;
use crate::hir::common::HirPackTail;
use crate::transformer::{CallInstr, NewTableInstr};

impl ArrayParser<'_, '_> {
    pub(super) fn open_tail(
        &mut self,
        seed: NewTableInstr,
        mut fields: Vec<HirTableField>,
        mut pending: Vec<HirExpr>,
        call: CallInstr,
    ) -> Option<HirExpr> {
        let ValuePack::Fixed(args) = call.args else {
            return None;
        };
        if call.results != ResultPack::Open(call.callee) {
            return None;
        }
        let first = call.callee.index().checked_sub(seed.dst.index() + 1)?;
        if first >= FIELDS_PER_FLUSH
            || args.start.index() != call.callee.index() + 1
            || first + args.len + 1 != pending.len()
        {
            return None;
        }
        let producer = InstrRef(self.cursor);
        self.cursor += 1;
        let LowInstr::SetList(batch) = *self.instruction()? else {
            return None;
        };
        let sources = self
            .lowering
            .dataflow
            .open_use_sources_at(InstrRef(self.cursor));
        if batch.base != seed.dst
            || batch.values != ValuePack::Open(Reg(seed.dst.index() + 1))
            || usize::try_from(batch.start_index).ok()? != fields.len() + 1
            || sources.has_entry()
            || sources.defs().len() != 1
        {
            return None;
        }
        let def = *sources.defs().iter().next()?;
        if self.lowering.dataflow.open_defs[def.index()].instr != producer
            || !self.lowering.owns_open_pack(def, InstrRef(self.cursor))
        {
            return None;
        }
        // Lua 5.1 excludes the open expression from NEWTABLE's array hint.
        if seed.lua51_allocation?
            != Lua51TableAllocation::from_field_counts(fields.len() + first, 0)
        {
            return None;
        }
        let mut values = pending.split_off(first).into_iter();
        let callee = values.next()?;
        let tail = HirExpr::Call(Box::new(HirCallExpr {
            callee,
            args: values.collect::<Vec<_>>().into(),
            method: false,
            fastcall: None,
            method_name: None,
        }));
        fields.extend(pending.into_iter().map(HirTableField::Array));
        self.cursor += 1;
        self.has_observable_producer = true;
        Some(HirExpr::TableConstructor(Box::new(HirTableConstructor {
            fields,
            trailing_multivalue: Some(HirPackTail::open(tail)),
        })))
    }
}
