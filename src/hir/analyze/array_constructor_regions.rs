//! Recover canonical Lua 5.1 array expressions before SSA scratch slots are promoted.
//!
//! This is not the generic SETLIST folding rule. A complete, single-block expression
//! must reproduce the compiler's register stack, fixed CALL results, CONCAT ranges,
//! allocation operands and 50-field flushes. Its only escaping value is installed by
//! the immediately following SETGLOBAL. No seed, scratch assignment or field write
//! may survive independently, and the global store stays after every RHS effect.
//! The entry prefix must also keep the register frame anchored: calls either discard
//! all results or declare locals with multiple later reads. An unread prefix root
//! could otherwise disappear during cleanup and change indirect-GC observations.
//! Record-only children additionally preserve each string-key write and lookup chain.

mod records;

use super::exprs::expr_for_const;
use super::helpers::concat_expr;
use super::instrs::lower_regular_instr;
use super::lower::ProtoLowering;
use crate::ast::DecompileDialect;
use crate::hir::common::{
    HirBinaryOpKind, HirBlock, HirCallExpr, HirExpr, HirLValue, HirStmt, HirTableConstructor,
    HirTableField, TempId,
};
use crate::structure::{BlockRef, Cfg, DataflowFacts};
use crate::transformer::{
    AccessBase, AccessKey, CallKind, GetTableKind, InstrRef, LowInstr, LoweredProto,
    Lua51TableAllocation, Reg, ResultPack, SetTableKind, ValueOperand, ValuePack,
};

const FIELDS_PER_FLUSH: usize = 50;
const MAX_REGION_INSTRS: usize = 4096;
const MAX_DEPTH: usize = 64;

#[cfg(test)]
#[path = "array_constructor_regions/regressions_397.rs"]
mod regressions_397;

#[cfg(test)]
#[path = "array_constructor_regions/regressions_398.rs"]
mod regressions_398;

pub(super) fn recover_global_arrays(body: &mut HirBlock, lowering: &ProtoLowering<'_>) {
    if lowering.target.version != DecompileDialect::Lua51
        || body
            .stmts
            .iter()
            .any(|stmt| matches!(stmt, HirStmt::Goto(_) | HirStmt::Label(_)))
    {
        return;
    }
    let mut index = 0;
    while index < body.stmts.len() {
        let Some((replacement, count)) = global_array_region(&body.stmts[index..], lowering) else {
            index += 1;
            continue;
        };
        body.stmts.splice(index..index + count, [replacement]);
        index += 1;
    }
}

fn global_array_region(
    stmts: &[HirStmt],
    lowering: &ProtoLowering<'_>,
) -> Option<(HirStmt, usize)> {
    let HirStmt::Assign(seed) = stmts.first()? else {
        return None;
    };
    let ([HirLValue::Temp(root)], [HirExpr::TableConstructor(table)], None) = (
        seed.targets.as_slice(),
        seed.values.fixed.as_slice(),
        &seed.values.tail,
    ) else {
        return None;
    };
    if !table.fields.is_empty() || table.trailing_multivalue.is_some() {
        return None;
    }
    let definition = lowering.dataflow.defs.get(root.index())?;
    if lowering.bindings.fixed_temps.get(definition.id.index()) != Some(root) {
        return None;
    }
    let start = definition.instr.index();
    let LowInstr::NewTable(new_table) = lowering.proto.instrs.get(start)? else {
        return None;
    };
    if !canonical_prefix_keeps_frame(
        lowering.proto,
        lowering.cfg,
        lowering.dataflow,
        start,
        new_table.dst,
    ) {
        return None;
    }
    let mut parser = ArrayParser {
        lowering,
        block: definition.block,
        root: new_table.dst,
        start,
        cursor: start,
        has_observable_producer: false,
    };
    let expression = parser.table(0)?;
    if !parser.has_observable_producer {
        return None;
    }
    let end = parser.cursor;
    let LowInstr::SetTable(store) = parser.instruction()? else {
        return None;
    };
    if store.kind != SetTableKind::Normal
        || store.base != AccessBase::Env
        || !matches!(store.key, AccessKey::Const(_))
        || store.value != ValueOperand::Reg(new_table.dst)
    {
        return None;
    }

    let mut expected = Vec::new();
    for pc in start..=end {
        for def in &lowering.dataflow.instr_defs[pc] {
            let temp = TempId(def.index());
            if lowering.bindings.fixed_temps.get(def.index()) != Some(&temp)
                || lowering.bindings.lvalue_for_temp(temp) != HirLValue::Temp(temp)
                || lowering
                    .bindings
                    .reg_is_reference_captured(lowering.dataflow.def_reg(*def))
                || lowering
                    .bindings
                    .temp_debug_locals
                    .get(temp.index())
                    .is_some_and(Option::is_some)
                || !lowering.dataflow.def_phi_uses[def.index()].is_empty()
                || lowering.dataflow.def_uses[def.index()]
                    .iter()
                    .any(|site| !(start..=end).contains(&site.instr.index()))
            {
                return None;
            }
        }
        expected.extend(lower_regular_instr(
            lowering,
            definition.block,
            InstrRef(pc),
            &lowering.proto.instrs[pc],
        )?);
    }
    if stmts.get(..expected.len()) != Some(expected.as_slice()) {
        return None;
    }
    let mut replacement = expected.last()?.clone();
    let HirStmt::Assign(store) = &mut replacement else {
        return None;
    };
    store.values = vec![expression].into();
    Some((replacement, expected.len()))
}

fn canonical_prefix_keeps_frame(
    proto: &LoweredProto,
    cfg: &Cfg,
    dataflow: &DataflowFacts,
    end: usize,
    root: Reg,
) -> bool {
    if proto.signature.has_vararg_param_reg
        || cfg.instr_to_block[..=end]
            .iter()
            .any(|block| *block != cfg.entry_block)
    {
        return false;
    }
    let mut slot = usize::from(proto.signature.num_params);
    let mut pc = 0;
    while pc < end {
        if !matches!(proto.instrs.get(pc),
            Some(LowInstr::GetTable(get))
                if get.dst.index() == slot
                    && get.base == AccessBase::Env
                    && get.kind == GetTableKind::Normal
                    && matches!(get.key, AccessKey::Const(_)))
        {
            return false;
        }
        pc += 1;
        let mut args = 0;
        while pc < end {
            let next = slot + args + 1;
            match proto.instrs.get(pc) {
                Some(LowInstr::Move(mov)) if mov.dst.index() == next && mov.src.index() < slot => {}
                Some(LowInstr::LoadConst(load)) if load.dst.index() == next => {}
                _ => break,
            }
            args += 1;
            pc += 1;
        }
        let Some(LowInstr::Call(call)) = proto.instrs.get(pc) else {
            return false;
        };
        let ValuePack::Fixed(arguments) = call.args else {
            return false;
        };
        if pc >= end
            || call.kind != CallKind::Normal
            || call.method_name.is_some()
            || call.callee.index() != slot
            || arguments.start.index() != slot + 1
            || arguments.len != args
        {
            return false;
        }
        if call.results == ResultPack::Ignore {
            pc += 1;
            continue;
        }
        let ResultPack::Fixed(results) = call.results else {
            return false;
        };
        if results.start.index() != slot || results.len != 1 {
            return false;
        }
        let [def] = dataflow.instr_defs[pc].as_slice() else {
            return false;
        };
        if dataflow.def_uses[def.index()]
            .iter()
            .filter(|site| site.instr.index() >= end)
            .take(2)
            .count()
            < 2
        {
            return false;
        }
        slot += 1;
        pc += 1;
    }
    pc == end && slot == root.index()
}

struct ArrayParser<'a, 'b> {
    lowering: &'a ProtoLowering<'b>,
    block: BlockRef,
    root: Reg,
    start: usize,
    cursor: usize,
    has_observable_producer: bool,
}

impl ArrayParser<'_, '_> {
    fn instruction(&self) -> Option<&LowInstr> {
        if self.cursor - self.start > MAX_REGION_INSTRS
            || self.lowering.cfg.instr_to_block.get(self.cursor) != Some(&self.block)
        {
            return None;
        }
        self.lowering.proto.instrs.get(self.cursor)
    }

    fn scalar_value(&self) -> Option<HirExpr> {
        let stmts = lower_regular_instr(
            self.lowering,
            self.block,
            InstrRef(self.cursor),
            self.instruction()?,
        )?;
        let [HirStmt::Assign(assign)] = stmts.as_slice() else {
            return None;
        };
        let ([HirLValue::Temp(_)], [value], None) = (
            assign.targets.as_slice(),
            assign.values.fixed.as_slice(),
            &assign.values.tail,
        ) else {
            return None;
        };
        Some(value.clone())
    }

    fn table(&mut self, depth: usize) -> Option<HirExpr> {
        if depth > MAX_DEPTH {
            return None;
        }
        let LowInstr::NewTable(seed) = *self.instruction()? else {
            return None;
        };
        let allocation = seed.lua51_allocation?;
        if allocation.hash_hint != 0 {
            if depth == 0 || allocation.array_hint != 0 {
                return None;
            }
            return self.record_table(seed);
        }
        let base = seed.dst.index();
        self.cursor += 1;
        let mut fields = Vec::new();
        let mut pending = Vec::new();
        loop {
            let instruction = self.instruction()?.clone();
            let next = base + pending.len() + 1;
            match instruction {
                LowInstr::NewTable(child) if child.dst.index() == next => {
                    pending.push(self.table(depth + 1)?);
                    continue;
                }
                LowInstr::LoadConst(load) if load.dst.index() == next => {
                    let value = self.scalar_value()?;
                    if !matches!(
                        value,
                        HirExpr::Nil
                            | HirExpr::Boolean(_)
                            | HirExpr::Integer(_)
                            | HirExpr::Number(_)
                            | HirExpr::String(_)
                    ) {
                        return None;
                    }
                    pending.push(value);
                }
                LowInstr::GetTable(get)
                    if get.dst.index() == next
                        && get.kind == GetTableKind::Normal
                        && matches!(get.key, AccessKey::Const(_))
                        && match get.base {
                            AccessBase::Env => true,
                            AccessBase::Reg(reg) => reg.index() < self.root.index(),
                            _ => false,
                        } =>
                {
                    pending.push(self.scalar_value()?);
                }
                LowInstr::Call(call)
                    if call.kind == CallKind::Normal && call.method_name.is_none() =>
                {
                    let ValuePack::Fixed(args) = call.args else {
                        return None;
                    };
                    let ResultPack::Fixed(results) = call.results else {
                        return None;
                    };
                    let callee = call.callee.index();
                    let first = callee.checked_sub(base + 1)?;
                    if results.start != call.callee
                        || results.len != 1
                        || args.start.index() != callee + 1
                        || first + args.len + 1 != pending.len()
                    {
                        return None;
                    }
                    let mut values = pending.split_off(first).into_iter();
                    let callee = values.next()?;
                    pending.push(HirExpr::Call(Box::new(HirCallExpr {
                        callee,
                        args: values.collect::<Vec<_>>().into(),
                        method: false,
                        fastcall: None,
                        method_name: None,
                    })));
                    self.has_observable_producer = true;
                }
                LowInstr::Concat(concat) if concat.dst == concat.src.start => {
                    let first = concat.dst.index().checked_sub(base + 1)?;
                    if concat.src.len < 2 || first + concat.src.len != pending.len() {
                        return None;
                    }
                    // A right-hand CONCAT would be merged by Lua's compiler, losing an
                    // original intermediate write and potentially a metamethod boundary.
                    if matches!(pending.last(), Some(HirExpr::Binary(binary))
                        if binary.op == HirBinaryOpKind::Concat)
                    {
                        return None;
                    }
                    let values = pending.split_off(first);
                    pending.push(concat_expr(values));
                }
                LowInstr::SetList(batch) if batch.base == seed.dst => {
                    let ValuePack::Fixed(values) = batch.values else {
                        return None;
                    };
                    if pending.is_empty()
                        || pending.len() > FIELDS_PER_FLUSH
                        || values.start.index() != base + 1
                        || values.len != pending.len()
                        || usize::try_from(batch.start_index).ok()? != fields.len() + 1
                    {
                        return None;
                    }
                    let complete = pending.len() < FIELDS_PER_FLUSH;
                    fields.extend(pending.drain(..).map(HirTableField::Array));
                    self.cursor += 1;
                    if complete {
                        if allocation != Lua51TableAllocation::from_field_counts(fields.len(), 0) {
                            return None;
                        }
                        return Some(HirExpr::TableConstructor(Box::new(HirTableConstructor {
                            fields,
                            trailing_multivalue: None,
                        })));
                    }
                    // Exact multiples of 50 can end at the enclosing flush or global
                    // installation, never at a guessed rounded-allocation boundary.
                    if allocation == Lua51TableAllocation::from_field_counts(fields.len(), 0)
                        && matches!(self.instruction()?,
                            LowInstr::SetList(outer) if outer.base.index() < base)
                    {
                        return Some(HirExpr::TableConstructor(Box::new(HirTableConstructor {
                            fields,
                            trailing_multivalue: None,
                        })));
                    }
                    if depth == 0
                        && allocation == Lua51TableAllocation::from_field_counts(fields.len(), 0)
                        && matches!(self.instruction()?, LowInstr::SetTable(store)
                            if store.base == AccessBase::Env
                                && store.value == ValueOperand::Reg(seed.dst))
                    {
                        return Some(HirExpr::TableConstructor(Box::new(HirTableConstructor {
                            fields,
                            trailing_multivalue: None,
                        })));
                    }
                    continue;
                }
                _ => return None,
            }
            self.cursor += 1;
        }
    }
}
