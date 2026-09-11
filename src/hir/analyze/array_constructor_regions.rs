//! 在 SSA 临时槽提升前，按 Lua 5.1 编译器的寄存器栈恢复完整建表表达式。
//!
//! 本 pass 消费原始 LIR、CFG 和定义／使用事实，校验槽写入、分配参数、调用结果、
//! CONCAT 与每 50 项的 SETLIST，再原子替换对应 HIR 区间。
//! 例如 `NEWTABLE; 子数组; SETTABLE; SETGLOBAL` 恢复为 `Global = {items = {...}}`；
//! 数组与纯记录表可递归包含彼此，每个记录字段仍按原顺序写入。
//! 整个 proto 可证明时由 statements 保留完整源码帧；否则局部事务要求根紧随全局
//! 安装或保留既有捕获绑定，入口前缀只能沿已证明的表达式、循环和守卫继续。
//! 全局临时区间先待提交，再由保留声明或完整终态锚定；开放结果还要求后缀闭合。
//! 未知局部和控制流不会被跳过，也不移动闭包／RHS 效果或拆写残余 SETLIST。

mod captured;
mod captured_values;
mod global_guards;
mod global_values;
mod loop_regions;
mod open_tail;
mod records;
mod statements;

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
const MAX_REGION_INSTRS: usize = 32768;
const MAX_DEPTH: usize = 64;

#[cfg(test)]
#[path = "array_constructor_regions/regressions_397.rs"]
mod regressions_397;

#[cfg(test)]
#[path = "array_constructor_regions/regressions_398.rs"]
mod regressions_398;

pub(super) fn recover_canonical_arrays(
    body: &mut HirBlock,
    lowering: &mut ProtoLowering<'_>,
) -> Option<crate::hir::common::HirSourceFrame> {
    if lowering.target.version != DecompileDialect::Lua51
        || body
            .stmts
            .iter()
            .any(|stmt| matches!(stmt, HirStmt::Goto(_) | HirStmt::Label(_)))
    {
        return None;
    }
    if let Some(frame) = statements::recover_parameter_frame_statements(body, lowering) {
        return Some(frame);
    }
    let mut index = 0;
    let mut closed_arrays = Vec::new();
    let mut before_open = None;
    let mut before_unanchored = None;
    while index < body.stmts.len() {
        let recovered = global_array_region(&body.stmts[index..], lowering, &closed_arrays)
            .or_else(|| {
                captured::captured_array_region(&body.stmts[index..], lowering, &closed_arrays)
            })
            .or_else(|| {
                global_values::global_value_region(&body.stmts[index..], lowering, &closed_arrays)
            })
            .or_else(|| {
                captured_values::captured_value_region(
                    &body.stmts[index..],
                    lowering,
                    &closed_arrays,
                )
            })
            .or_else(|| loop_regions::loop_region(&body.stmts[index..], lowering, &closed_arrays))
            .or_else(|| {
                global_guards::global_guard_region(&body.stmts[index..], lowering, &closed_arrays)
            });
        let Some((replacement, count, region)) = recovered else {
            if before_unanchored.is_some()
                && scalar_prefix_anchor(&body.stmts[index], lowering, &closed_arrays)
            {
                before_unanchored = None;
            }
            index += 1;
            continue;
        };
        // 全局赋值不能仅凭唯一使用就删掉原槽：更晚的已证明局部声明或终态
        // 必须确认源码栈深度。未知后缀不替尚未闭合的全局序列提供退役证据。
        if !region.retained && before_unanchored.is_none() {
            before_unanchored = Some(body.clone());
        }
        let root_values = match &replacement {
            HirStmt::Assign(assign) => Some(&assign.values),
            HirStmt::LocalDecl(decl) => Some(&decl.values),
            _ => None,
        };
        if before_open.is_none()
            && root_values.is_some_and(|values| {
                matches!(values.fixed.as_slice(),
                [HirExpr::TableConstructor(table)] if table.trailing_multivalue.is_some())
            })
        {
            before_open = Some(before_unanchored.clone().unwrap_or_else(|| body.clone()));
        }
        body.stmts.splice(index..index + count, [replacement]);
        closed_arrays.push(region);
        if closed_arrays.last().is_some_and(|region| region.retained) {
            before_unanchored = None;
        }
        index += 1;
    }
    // 开放结果可写到静态 maxstack 之外；后缀必须也保持同一源码帧的完整语句。
    // 任何未闭合后缀都撤销首个开放区间及依赖它的改写，保留先前已证明的前缀。
    let end = lowering.proto.instrs.len().saturating_sub(1);
    let complete = matches!(lowering.proto.instrs.get(end), Some(LowInstr::Return(ret))
            if matches!(ret.values, ValuePack::Fixed(values) if values.len == 0))
        && certified_prefix_depth(
            lowering.proto,
            lowering.cfg,
            lowering.dataflow,
            end,
            &closed_arrays,
        )
        .is_some();
    if !complete && let Some(before) = before_open.or(before_unanchored) {
        *body = before;
    }
    None
}

struct ClosedArrayRegion {
    start: usize,
    end: usize,
    root: Reg,
    retained: bool,
}

fn scalar_prefix_anchor(
    stmt: &HirStmt,
    lowering: &ProtoLowering<'_>,
    closed: &[ClosedArrayRegion],
) -> bool {
    let target = match stmt {
        HirStmt::Assign(assign) if assign.targets.len() == 1 => assign.targets[0].clone(),
        HirStmt::LocalDecl(decl) if decl.bindings.len() == 1 => HirLValue::Local(decl.bindings[0]),
        _ => return false,
    };
    let candidates = match target {
        HirLValue::Temp(temp) => vec![temp],
        HirLValue::Local(_) => lowering
            .bindings
            .fixed_temps
            .iter()
            .copied()
            .filter(|temp| lowering.bindings.lvalue_for_temp(*temp) == target)
            .collect(),
        _ => return false,
    };
    candidates.into_iter().any(|temp| {
        let Some(def) = lowering.dataflow.defs.get(temp.index()) else {
            return false;
        };
        let pc = def.instr.index();
        let Some(instruction) = lowering.proto.instrs.get(pc) else {
            return false;
        };
        if !matches!(instruction, LowInstr::LoadConst(_) | LowInstr::Call(_)) {
            return false;
        }
        // 原始多次使用的固定调用结果、确切被捕获的常量声明，由同一前缀证明
        // 确认追加了一个源码槽。不能把临时CALL参数或会被删掉的单读结果当成锚。
        certified_prefix_state(
            lowering.proto,
            lowering.cfg,
            lowering.dataflow,
            pc + 1,
            closed,
        )
        .is_some_and(|state| state.depth == def.reg.index() + 1 && state.block == def.block)
            && lower_regular_instr(lowering, def.block, def.instr, instruction)
                .is_some_and(|expected| expected.as_slice() == std::slice::from_ref(stmt))
    })
}

fn global_array_region(
    stmts: &[HirStmt],
    lowering: &ProtoLowering<'_>,
    closed_arrays: &[ClosedArrayRegion],
) -> Option<(HirStmt, usize, ClosedArrayRegion)> {
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
    if !prefix_with_closed_arrays(
        lowering.proto,
        lowering.cfg,
        lowering.dataflow,
        start,
        new_table.dst,
        closed_arrays,
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
        allow_open_tail: false,
        rk_pool_full: false,
    };
    let expression = parser.table(0)?;
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
                || definition_is_captured(lowering, *def)
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
    Some((
        replacement,
        expected.len(),
        ClosedArrayRegion {
            start,
            end: end + 1,
            root: new_table.dst,
            retained: false,
        },
    ))
}

fn prefix_with_closed_arrays(
    proto: &LoweredProto,
    cfg: &Cfg,
    dataflow: &DataflowFacts,
    end: usize,
    root: Reg,
    closed_arrays: &[ClosedArrayRegion],
) -> bool {
    certified_prefix_depth(proto, cfg, dataflow, end, closed_arrays) == Some(root.index())
}

fn certified_prefix_depth(
    proto: &LoweredProto,
    cfg: &Cfg,
    dataflow: &DataflowFacts,
    end: usize,
    closed_arrays: &[ClosedArrayRegion],
) -> Option<usize> {
    let state = certified_prefix_state(proto, cfg, dataflow, end, closed_arrays)?;
    (cfg.instr_to_block.get(end) == Some(&state.block)).then_some(state.depth)
}

struct PrefixState {
    depth: usize,
    block: BlockRef,
}

fn certified_prefix_state(
    proto: &LoweredProto,
    cfg: &Cfg,
    dataflow: &DataflowFacts,
    end: usize,
    closed_arrays: &[ClosedArrayRegion],
) -> Option<PrefixState> {
    if proto.signature.has_vararg_param_reg || end >= proto.instrs.len() {
        return None;
    }
    let mut slot = usize::from(proto.signature.num_params);
    let mut pc = 0;
    let mut block = cfg.entry_block;
    while pc < end {
        if cfg.instr_to_block.get(pc) != Some(&block) {
            return None;
        }
        // Only a previously replaced, whole NEWTABLE..SETGLOBAL transaction can
        // bridge a prefix. Unknown statements and surviving locals are not skipped.
        if let Some(region) = closed_arrays.iter().find(|region| region.start == pc) {
            if region.root.index() != slot || region.end > end {
                return None;
            }
            pc = region.end;
            block = *cfg.instr_to_block.get(pc)?;
            if region.retained {
                slot += 1;
            }
            continue;
        }
        // A primitive local captured by a later closure must retain a stack slot.
        // Do not admit unread constants: cleanup could erase their declaration.
        if let Some(LowInstr::LoadConst(load)) = proto.instrs.get(pc)
            && load.dst.index() == slot
            && dataflow.instr_defs[pc].len() == 1
            && dataflow.def_uses[dataflow.instr_defs[pc][0].index()].iter().any(|site| {
                matches!(&proto.instrs[site.instr.index()], LowInstr::Closure(closure)
                        if closure.captures.iter().any(|capture|
                            capture.source == crate::transformer::CaptureSource::ByReference(load.dst)))
            })
        {
            slot += 1;
            pc += 1;
            continue;
        }
        if !matches!(proto.instrs.get(pc),
            Some(LowInstr::GetTable(get))
                if get.dst.index() == slot
                    && get.base == AccessBase::Env
                    && get.kind == GetTableKind::Normal
                    && matches!(get.key, AccessKey::Const(_)))
        {
            return None;
        }
        pc += 1;
        let mut args = 0;
        while pc < end {
            if cfg.instr_to_block.get(pc) != Some(&block) {
                return None;
            }
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
            return None;
        };
        let ValuePack::Fixed(arguments) = call.args else {
            return None;
        };
        if pc >= end
            || cfg.instr_to_block.get(pc) != Some(&block)
            || call.kind != CallKind::Normal
            || call.method_name.is_some()
            || call.callee.index() != slot
            || arguments.start.index() != slot + 1
            || arguments.len != args
        {
            return None;
        }
        if call.results == ResultPack::Ignore {
            pc += 1;
            continue;
        }
        let ResultPack::Fixed(results) = call.results else {
            return None;
        };
        if results.start.index() != slot || results.len != 1 {
            return None;
        }
        let [def] = dataflow.instr_defs[pc].as_slice() else {
            return None;
        };
        if dataflow.def_uses[def.index()]
            .iter()
            .filter(|site| site.instr.index() > pc)
            .take(2)
            .count()
            < 2
        {
            return None;
        }
        slot += 1;
        pc += 1;
    }
    (pc == end).then_some(PrefixState { depth: slot, block })
}

fn definition_is_captured(lowering: &ProtoLowering<'_>, def: crate::structure::DefId) -> bool {
    let reg = lowering.dataflow.def_reg(def);
    lowering.dataflow.def_uses[def.index()].iter().any(|site| {
        matches!(&lowering.proto.instrs[site.instr.index()], LowInstr::Closure(closure)
            if closure.captures.iter().any(|capture| matches!(capture.source,
                crate::transformer::CaptureSource::ByReference(source)
                    | crate::transformer::CaptureSource::ByValue(source) if source == reg)))
    })
}

#[derive(Clone)]
struct ArrayParser<'a, 'b> {
    lowering: &'a ProtoLowering<'b>,
    block: BlockRef,
    root: Reg,
    start: usize,
    cursor: usize,
    has_observable_producer: bool,
    allow_open_tail: bool,
    rk_pool_full: bool,
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
        let seed_pc = self.cursor;
        let allocation = seed.lua51_allocation?;
        if allocation.hash_hint != 0 {
            if allocation.array_hint != 0 {
                return None;
            }
            return self.record_table(seed, depth);
        }
        let base = seed.dst.index();
        self.cursor += 1;
        // 空表不产生 SETLIST；它的消费位置仍由父字段、全局安装或捕获根校验。
        // 若后面还有填表指令，父解析器不会把它们当作已完成子项的消费。
        if allocation.array_hint == 0
            && !(depth == 0 && self.allow_open_tail && self.has_later_list_batch(seed_pc, seed.dst))
        {
            return Some(HirExpr::TableConstructor(Box::default()));
        }
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
                    ) || matches!(value, HirExpr::Number(number) if !number.is_finite())
                    {
                        return None;
                    }
                    pending.push(value);
                }
                LowInstr::LoadBool(load) if load.dst.index() == next => {
                    pending.push(HirExpr::Boolean(load.value));
                }
                LowInstr::GetUpvalue(get) if get.dst.index() == next => {
                    // 列表项需要在当前位置物化 upvalue；后续调用即使更新同一
                    // upvalue，也不能替换已经写入此临时槽的旧值。
                    pending.push(self.scalar_value()?);
                }
                LowInstr::Move(mov)
                    if mov.dst.index() == next && mov.src.index() < self.root.index() =>
                {
                    // 数组项必须写入下一个列表槽，即使 SSA 已把 MOVE 归并为别名。
                    // 外层帧事务负责保证低槽仍对应真实参数／局部变量。
                    pending.push(super::exprs::expr_for_reg_use(
                        self.lowering,
                        self.block,
                        InstrRef(self.cursor),
                        mov.src,
                    ));
                }
                LowInstr::LoadNil(load)
                    if load.dst.start.index() == next
                        && load.dst.len > 0
                        && pending.len() + load.dst.len <= FIELDS_PER_FLUSH =>
                {
                    pending.extend(std::iter::repeat_n(HirExpr::Nil, load.dst.len));
                }
                LowInstr::GetTable(get) if get.kind == GetTableKind::Normal => {
                    let (base, key) = match get.key {
                        AccessKey::Const(_)
                            if get.dst.index() == next
                                && match get.base {
                                    AccessBase::Env => true,
                                    AccessBase::Reg(reg) => reg.index() < self.root.index(),
                                    _ => false,
                                } =>
                        {
                            pending.push(self.scalar_value()?);
                            self.has_observable_producer = true;
                            self.cursor += 1;
                            continue;
                        }
                        AccessKey::Const(key)
                            if !pending.is_empty()
                                && get.dst.index() + 1 == next
                                && get.base == AccessBase::Reg(get.dst)
                                && matches!(
                                    expr_for_const(self.lowering.proto, key),
                                    HirExpr::String(_)
                                ) =>
                        {
                            (pending.pop()?, expr_for_const(self.lowering.proto, key))
                        }
                        AccessKey::Reg(key)
                            if key.index() < self.root.index()
                                && !pending.is_empty()
                                && get.dst.index() + 1 == next
                                && get.base == AccessBase::Reg(get.dst) =>
                        {
                            (
                                pending.pop()?,
                                super::exprs::expr_for_reg_use(
                                    self.lowering,
                                    self.block,
                                    InstrRef(self.cursor),
                                    key,
                                ),
                            )
                        }
                        AccessKey::Reg(key) if key.index() + 1 == next => {
                            let crate::structure::SsaValue::Def(def) =
                                self.lowering.dataflow.use_values[self.cursor]
                                    .fixed
                                    .get(key)?
                            else {
                                return None;
                            };
                            let LowInstr::LoadConst(load) = &self.lowering.proto.instrs
                                [self.lowering.dataflow.defs[def.index()].instr.index()]
                            else {
                                return None;
                            };
                            if load.value.index() <= 255
                                || !matches!(pending.last(), Some(HirExpr::String(_)))
                            {
                                return None;
                            }
                            let key_expr = pending.pop()?;
                            let base = match get.base {
                                AccessBase::Reg(base)
                                    if base == get.dst && base.index() + 2 == next =>
                                {
                                    pending.pop()?
                                }
                                AccessBase::Reg(base)
                                    if base.index() < self.root.index() && get.dst == key =>
                                {
                                    let HirExpr::TableAccess(access) = self.scalar_value()? else {
                                        return None;
                                    };
                                    access.base
                                }
                                _ => return None,
                            };
                            (base, key_expr)
                        }
                        _ => return None,
                    };
                    pending.push(HirExpr::TableAccess(Box::new(
                        crate::hir::common::HirTableAccess { base, key },
                    )));
                    self.has_observable_producer = true;
                }
                LowInstr::Call(call)
                    if call.kind == CallKind::Normal && call.method_name.is_none() =>
                {
                    if depth == 0
                        && self.allow_open_tail
                        && matches!(call.results, ResultPack::Open(_))
                    {
                        return self.open_tail(seed, fields, pending, call);
                    }
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
                    // 圆整 hint 不能区分 50 与 51 项；原 SSA 的后续 SETLIST 使用
                    // 决定是否还有批次。没有剩余批次才把边界交给外层消费者验证。
                    if allocation == Lua51TableAllocation::from_field_counts(fields.len(), 0)
                        && !self.has_later_list_batch(seed_pc, seed.dst)
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

    fn has_later_list_batch(&self, seed_pc: usize, root: Reg) -> bool {
        let [def] = self.lowering.dataflow.instr_defs[seed_pc].as_slice() else {
            return true;
        };
        self.lowering.dataflow.def_uses[def.index()]
            .iter()
            .any(|site| {
                site.instr.index() >= self.cursor
                    && matches!(&self.lowering.proto.instrs[site.instr.index()],
                    LowInstr::SetList(batch) if batch.base == root)
            })
    }

    fn enclosing_record_store(&self, value: Reg) -> bool {
        matches!(self.instruction(), Some(LowInstr::SetTable(store))
            if store.kind == SetTableKind::Normal
                && matches!(store.base, AccessBase::Reg(owner) if owner.index() < value.index())
                && store.value == ValueOperand::Reg(value))
    }
}
