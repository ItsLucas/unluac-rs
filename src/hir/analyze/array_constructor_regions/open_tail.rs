//! An open CALL consumed immediately by the captured root's final SETLIST.
//!
//! The unknown-width result remains a genuine constructor tail. The restricted
//! closure-installation suffix cannot read or capture any result scratch slot.

use super::*;
use crate::hir::common::HirPackTail;
use crate::transformer::{CallInstr, CaptureSource, NewTableInstr};

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
        if !closure_installation_suffix(self.lowering.proto, self.cursor, seed.dst) {
            return None;
        }
        self.has_observable_producer = true;
        Some(HirExpr::TableConstructor(Box::new(HirTableConstructor {
            fields,
            trailing_multivalue: Some(HirPackTail::open(tail)),
        })))
    }
}

fn closure_installation_suffix(proto: &LoweredProto, mut pc: usize, root: Reg) -> bool {
    let scratch = Reg(root.index() + 1);
    while let Some(LowInstr::Closure(closure)) = proto.instrs.get(pc) {
        if closure.dst != scratch
            || closure.captures.iter().any(|capture| match capture.source {
                CaptureSource::ByReference(reg) | CaptureSource::ByValue(reg) => {
                    reg.index() > root.index()
                }
                _ => false,
            })
        {
            return false;
        }
        pc += 1;
        if !matches!(proto.instrs.get(pc), Some(LowInstr::SetTable(store))
            if store.kind == SetTableKind::Normal && store.base == AccessBase::Env
                && matches!(store.key, AccessKey::Const(_))
                && store.value == ValueOperand::Reg(scratch))
        {
            return false;
        }
        pc += 1;
    }
    pc + 1 == proto.instrs.len()
        && matches!(proto.instrs.get(pc), Some(LowInstr::Return(ret))
            if matches!(ret.values, ValuePack::Fixed(values) if values.len == 0))
}
