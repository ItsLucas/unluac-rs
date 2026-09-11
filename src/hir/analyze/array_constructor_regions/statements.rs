//! 在 SSA 提升前恢复完整 Lua 5.1 源码帧，并原子提交整份 proto 的语句。
//!
//! 表达式栈必须逐条重现原始写槽、调用宽度、控制边及 SETLIST；声明解释还须通过
//! 全部后缀验证。例如单次读取的 `local scene = player.scene()` 仍占原槽，后面的
//! 调用及数组不能提前覆盖它。只证明完整函数，不用 dummy 初始化填补未知槽；
//! 已认证局部与控制流通过 HIR/AST 元数据固定，不能再由 readability 改写。

use super::*;
use crate::hir::common::{
    HirCallStmt, HirIf, HirLocalDecl, HirReturn, HirSourceFrame, HirUnaryExpr, HirUnaryOpKind,
    LocalId, ParamId,
};
use crate::transformer::{BranchCond, BranchSubject, CondOperand, ConstRef};
use std::cell::Cell;
use std::rc::Rc;

#[path = "statements/and_guards.rs"]
mod and_guards;
#[path = "statements/arm_exits.rs"]
mod arm_exits;
#[path = "statements/assignments.rs"]
mod assignments;
#[path = "statements/boolean_values.rs"]
mod boolean_values;
#[path = "statements/breaks.rs"]
mod breaks;
#[path = "statements/condition_order.rs"]
mod condition_order;
#[path = "statements/constant_pool.rs"]
mod constant_pool;
#[path = "statements/declarations.rs"]
mod declarations;
#[path = "statements/expressions.rs"]
mod expressions;
#[path = "statements/fixed_results.rs"]
mod fixed_results;
#[path = "statements/frames.rs"]
mod frames;
#[path = "statements/logical_guards.rs"]
mod logical_guards;
#[path = "statements/loops.rs"]
mod loops;
#[path = "statements/numeric_loops.rs"]
mod numeric_loops;
#[path = "statements/open_packs.rs"]
mod open_packs;
use expressions::ExpressionEnd;
use frames::{Frame, FrameSlot};

pub(super) fn recover_parameter_frame_statements(
    body: &mut HirBlock,
    lowering: &mut ProtoLowering<'_>,
) -> Option<HirSourceFrame> {
    let capture_reader = lowering.proto.children.is_empty()
        && lowering.proto.upvalues.common.count > 1
        && lowering.proto.instrs.len() <= 128;
    if lowering.proto.signature.has_vararg_param_reg
        // This whole-frame expression grammar has no per-evaluation-point RK
        // pool witness yet. A full pool can materialize even an old numeric key.
        || lowering.proto.constants.common.literals.len() > 255
        || lowering.proto.constants.common.literals.iter().any(|value| matches!(value,
            crate::parser::RawLiteralConst::Number(number) if !number.is_finite()))
        || lowering.proto.instrs.len() > MAX_REGION_INSTRS
        || !lowering.bindings.debug_entry_local_decls.is_empty()
        || !lowering.bindings.capture_entry_local_decls.is_empty()
        || !lowering.shared_closure_locals.is_empty()
        || !capture_reader && !lowering
            .proto
            .instrs
            .iter()
            .any(|instruction| matches!(instruction, LowInstr::SetList(_)))
    {
        return None;
    }
    let written_stack = lowering
        .dataflow
        .defs
        .iter()
        .map(|definition| definition.reg.index() + 1)
        .chain(
            lowering
                .proto
                .instrs
                .iter()
                .filter_map(|instruction| match instruction {
                    LowInstr::GenericForCall(call) => Some(call.iterator.index() + 6),
                    _ => None,
                }),
        )
        .max()
        .unwrap_or_default()
        .max(usize::from(lowering.proto.signature.num_params))
        .max(2);
    if written_stack != usize::from(lowering.proto.frame.max_stack_size) {
        return None;
    }
    let mut parser = StatementParser {
        lowering,
        tables: 0,
        locals: Vec::new(),
        attempts: Rc::new(Cell::new(0)),
        loop_exit: None,
        arm_exit: None,
        expression_origin: None,
    };
    let mut active = (0..usize::from(lowering.proto.signature.num_params))
        .map(|index| FrameSlot {
            original: HirExpr::ParamRef(ParamId(index)),
            value: Some(HirExpr::ParamRef(ParamId(index))),
        })
        .collect();
    let replacement = parser.block(0, lowering.proto.instrs.len(), &mut active, 0)?;
    if parser.tables == 0 && !capture_reader {
        return None;
    }
    let locals = parser.locals;
    let frame = HirSourceFrame {
        local_slots: locals.iter().map(|local| (local.id, local.slot)).collect(),
        max_stack_size: lowering.proto.frame.max_stack_size,
    };
    for local in locals {
        lowering.bindings.locals.push(local.id);
        lowering.bindings.local_debug_hints.push(local.hint);
    }
    *body = replacement;
    Some(frame)
}

#[derive(Clone)]
struct FrameLocal {
    id: LocalId,
    slot: usize,
    hint: Option<String>,
}

#[derive(Clone)]
struct StatementParser<'a, 'b> {
    lowering: &'a ProtoLowering<'b>,
    tables: usize,
    locals: Vec<FrameLocal>,
    attempts: Rc<Cell<usize>>,
    loop_exit: Option<breaks::LoopExit>,
    arm_exit: Option<arm_exits::ArmExit>,
    expression_origin: Option<(usize, usize)>,
}

impl StatementParser<'_, '_> {
    fn block(
        &mut self,
        mut pc: usize,
        end: usize,
        active: &mut Frame,
        depth: usize,
    ) -> Option<HirBlock> {
        self.attempts.set(self.attempts.get() + 1);
        if depth > MAX_DEPTH
            || self.attempts.get() > MAX_REGION_INSTRS
            || end > self.lowering.proto.instrs.len()
        {
            return None;
        }
        let mut body = HirBlock::default();
        while pc < end {
            let mut candidate = self.clone();
            let mut after = pc;
            let mut assigned_frame = active.clone();
            if let Some(ExpressionEnd::Assigned(assign)) =
                candidate.expression(&mut after, end, active)
                && let Some(mut suffix) =
                    candidate.block(after, end, &mut assigned_frame, depth + 1)
            {
                body.stmts.push(HirStmt::Assign(Box::new(assign)));
                body.stmts.append(&mut suffix.stmts);
                *self = candidate;
                *active = assigned_frame;
                return Some(body);
            }
            for call_prefix in [false, true] {
                let mut candidate = self.clone();
                if let Some((value, definition, next)) =
                    candidate.local_initializer(pc, end, active, call_prefix)
                {
                    let mut declared = active.clone();
                    if let Some(declaration) =
                        candidate.declaration(definition, value, &mut declared)
                        && let Some(mut suffix) =
                            candidate.block(next, end, &mut declared, depth + 1)
                    {
                        body.stmts.push(declaration);
                        body.stmts.append(&mut suffix.stmts);
                        *self = candidate;
                        *active = declared;
                        return Some(body);
                    }
                }
            }
            match self.lowering.proto.instrs.get(pc)?.clone() {
                LowInstr::Jump(_) if self.is_loop_break(pc) && pc + 1 == end => {
                    body.stmts.push(HirStmt::Break);
                    pc += 1;
                }
                LowInstr::Closure(_) => {
                    body.stmts.push(self.closure_install(&mut pc, end, active)?);
                }
                LowInstr::LoadNil(load) if load.dst.start.index() == active.len() => {
                    body.stmts
                        .push(self.nil_declaration(pc, load.dst.len, active)?);
                    pc += 1;
                }
                LowInstr::Branch(_) => {
                    if let Some(statement) =
                        self.materialized_boolean_statement(&mut pc, end, active)
                    {
                        body.stmts.push(statement);
                        continue;
                    }
                    let statement = self.branch(&mut pc, end, active, None, depth)?;
                    body.stmts.push(statement);
                }
                LowInstr::NewTable(seed) if seed.dst.index() == active.len() => {
                    let start = pc;
                    let expression = self.table(&mut pc, active)?;
                    if pc > end {
                        return None;
                    }
                    body.stmts
                        .push(self.declaration(start, expression, active)?);
                }
                LowInstr::Return(ret) => {
                    let ValuePack::Fixed(values) = ret.values else {
                        return None;
                    };
                    if values.len > 1 {
                        return None;
                    }
                    let implicit = values.len == 0 && pc + 1 == self.lowering.proto.instrs.len();
                    let values = if values.len == 0 {
                        Vec::new()
                    } else {
                        vec![active.get(values.start.index())?.value.clone()?]
                    };
                    if !implicit {
                        body.stmts.push(HirStmt::Return(Box::new(HirReturn {
                            values: values.into(),
                        })));
                    }
                    pc += 1;
                    if pc + 1 == end
                        && matches!(self.lowering.proto.instrs.get(pc),
                        Some(LowInstr::Return(ret)) if matches!(ret.values, ValuePack::Fixed(values) if values.len == 0))
                    {
                        pc += 1;
                    }
                    if pc != end {
                        return None;
                    }
                }
                _ => match self.expression(&mut pc, end, active)? {
                    ExpressionEnd::Assigned(assign) => {
                        body.stmts.push(HirStmt::Assign(Box::new(assign)))
                    }
                    ExpressionEnd::Iterator(call) => {
                        let mut candidate = self.clone();
                        let mut next = pc;
                        if let Some(statement) =
                            candidate.generic_for(&mut next, end, active, call.clone(), depth)
                        {
                            *self = candidate;
                            pc = next;
                            body.stmts.push(statement);
                        } else {
                            body.stmts.push(self.fixed_results(
                                pc.checked_sub(1)?,
                                call,
                                3,
                                active,
                            )?);
                        }
                    }
                    ExpressionEnd::FixedResults {
                        call,
                        definition,
                        width,
                    } => {
                        body.stmts
                            .push(self.fixed_results(definition, call, width, active)?);
                    }
                    ExpressionEnd::NumericHeader(header) => {
                        body.stmts
                            .push(self.numeric_for(&mut pc, end, active, header, depth)?);
                    }
                    ExpressionEnd::ConditionValues(values) => {
                        body.stmts
                            .push(self.branch(&mut pc, end, active, Some(values), depth)?);
                    }
                    ExpressionEnd::Ignored(call) => body
                        .stmts
                        .push(HirStmt::CallStmt(Box::new(HirCallStmt { call }))),
                    ExpressionEnd::Value { value, definition } => {
                        // A fixed call result may be either a source declaration or
                        // temporary guard input. Only a complete suffix can distinguish
                        // a single-use local from an otherwise identical SSA temporary.
                        let mut candidate = self.clone();
                        let mut declared_frame = active.clone();
                        if let Some(declaration) =
                            candidate.declaration(definition, value.clone(), &mut declared_frame)
                            && let Some(mut suffix) =
                                candidate.block(pc, end, &mut declared_frame, depth + 1)
                        {
                            body.stmts.push(declaration);
                            body.stmts.append(&mut suffix.stmts);
                            *self = candidate;
                            *active = declared_frame;
                            return Some(body);
                        }
                        if !expressions::is_materialized_expression(&value) {
                            return None;
                        }
                        let statement =
                            self.branch(&mut pc, end, active, Some(vec![value]), depth)?;
                        body.stmts.push(statement);
                    }
                },
            }
        }
        Some(body)
    }

    fn declaration(
        &mut self,
        pc: usize,
        expression: HirExpr,
        active: &mut Frame,
    ) -> Option<HirStmt> {
        self.initializer_constants_closed(&expression)?;
        if active.len() >= crate::SOURCE_LOCAL_LIMIT {
            return None;
        }
        if !matches!(
            self.lowering.proto.instrs.get(pc),
            Some(
                LowInstr::Call(_)
                    | LowInstr::NewTable(_)
                    | LowInstr::LoadConst(_)
                    | LowInstr::LoadBool(_)
                    | LowInstr::Concat(_)
                    | LowInstr::GetTable(_)
                    | LowInstr::GetUpvalue(_)
                    | LowInstr::UnaryOp(_)
                    | LowInstr::BinaryOp(_)
            )
        ) {
            return None;
        }
        let [def] = self.lowering.dataflow.instr_defs.get(pc)?.as_slice() else {
            return None;
        };
        let temp = *self.lowering.bindings.fixed_temps.get(def.index())?;
        if self.lowering.dataflow.def_reg(*def).index() != active.len() {
            return None;
        }
        let id = LocalId(self.lowering.bindings.locals.len() + self.locals.len());
        let original = self.lowering.bindings.expr_for_temp(temp);
        let hint = match &original {
            HirExpr::LocalRef(local) => self
                .lowering
                .bindings
                .local_debug_hints
                .get(local.index())
                .cloned()
                .flatten(),
            _ => self
                .lowering
                .bindings
                .temp_debug_locals
                .get(temp.index())
                .cloned()
                .flatten(),
        };
        self.locals.push(FrameLocal {
            id,
            slot: active.len(),
            hint,
        });
        active.push(FrameSlot {
            original,
            value: Some(HirExpr::LocalRef(id)),
        });
        Some(HirStmt::LocalDecl(Box::new(HirLocalDecl {
            bindings: vec![id],
            values: vec![expression].into(),
        })))
    }

    fn branch(
        &mut self,
        pc: &mut usize,
        end: usize,
        active: &Frame,
        temporary: Option<Vec<HirExpr>>,
        depth: usize,
    ) -> Option<HirStmt> {
        let mut candidate = self.clone();
        if let Some((statement, next)) =
            candidate.and_branch(*pc, end, active, temporary.as_deref(), depth)
        {
            *self = candidate;
            *pc = next;
            return Some(statement);
        }
        let mut candidate = self.clone();
        if let Some((statement, next)) =
            candidate.or_branch(*pc, end, active, temporary.as_deref(), depth)
        {
            *self = candidate;
            *pc = next;
            return Some(statement);
        }
        let LowInstr::Branch(branch) = *self.lowering.proto.instrs.get(*pc)? else {
            return None;
        };
        let (other, invert) = if branch.then_target.index() == *pc + 1 {
            (branch.else_target.index(), false)
        } else if branch.else_target.index() == *pc + 1 {
            (branch.then_target.index(), true)
        } else {
            return None;
        };
        let other = self.arm_target(other, end)?;
        if other <= *pc + 1 {
            return None;
        }
        let (arm_end, merge, arm_exit) = match self.lowering.proto.instrs.get(other - 1)? {
            LowInstr::Jump(jump)
                if jump.target.index() >= other && !self.is_loop_break(other - 1) =>
            {
                (
                    other - 1,
                    self.arm_target(jump.target.index(), end)?,
                    Some(jump.target.index()),
                )
            }
            _ => (other, other, None),
        };
        let mut condition = self.condition(*pc, branch.cond, active, temporary.as_deref(), None)?;
        if invert {
            condition = negate(condition);
        }
        let then_block =
            self.arm_block(*pc + 1, arm_end, &mut active.clone(), depth + 1, arm_exit)?;
        let else_block = if merge > other {
            Some(self.block(other, merge, &mut active.clone(), depth + 1)?)
        } else {
            None
        };
        *pc = merge;
        Some(HirStmt::If(Box::new(HirIf {
            cond: condition,
            then_block,
            else_block,
        })))
    }

    fn condition(
        &self,
        pc: usize,
        condition: BranchCond,
        active: &Frame,
        temporary: Option<&[HirExpr]>,
        reserved_key: Option<ConstRef>,
    ) -> Option<HirExpr> {
        if let Some(values) = temporary {
            // Lua's conditional compiler can consume `not x` by reversing the
            // branch and omitting OP_NOT. Its original scratch overwrite must
            // instead belong to a preserved declaration, never this inline path.
            if let BranchSubject::Truthy(CondOperand::Reg(reg)) = condition.subject
                && reg.index() >= active.len()
                && (matches!(values.get(reg.index() - active.len()), Some(HirExpr::Unary(unary))
                    if unary.op == HirUnaryOpKind::Not)
                    || matches!(values.get(reg.index() - active.len()), Some(HirExpr::Binary(binary))
                        if matches!(binary.op,HirBinaryOpKind::Eq|HirBinaryOpKind::Lt|HirBinaryOpKind::Le)))
            {
                return None;
            }
            for index in 0..values.len() {
                let reads = |operand| matches!(operand, CondOperand::Reg(reg) if reg.index() == active.len() + index);
                let count = match condition.subject {
                    BranchSubject::Truthy(value) => usize::from(reads(value)),
                    BranchSubject::Compare { lhs, rhs, .. } => {
                        usize::from(reads(lhs)) + usize::from(reads(rhs))
                    }
                };
                if count != 1 {
                    return None;
                }
            }
        }
        let operand = |value: CondOperand| match value {
            CondOperand::Reg(reg) if reg.index() < active.len() => {
                active[reg.index()].value.clone()
            }
            CondOperand::Reg(reg) if reg.index() >= active.len() => {
                temporary?.get(reg.index() - active.len()).cloned()
            }
            CondOperand::Const(key) => Some(expr_for_const(self.lowering.proto, key)),
            _ => None,
        };
        let expression = match condition.subject {
            BranchSubject::Truthy(value) => operand(value)?,
            BranchSubject::Compare {
                predicate,
                lhs,
                rhs,
            } => {
                use crate::transformer::BranchPredicate;
                let op = match predicate {
                    BranchPredicate::Eq => HirBinaryOpKind::Eq,
                    BranchPredicate::Lt => HirBinaryOpKind::Lt,
                    BranchPredicate::Le => HirBinaryOpKind::Le,
                };
                HirExpr::Binary(Box::new(crate::hir::common::HirBinaryExpr {
                    operand_order: Some(self.relational_order(
                        pc,
                        condition.subject,
                        active,
                        temporary,
                        reserved_key,
                    )?),
                    op,
                    lhs: operand(lhs)?,
                    rhs: operand(rhs)?,
                }))
            }
        };
        Some(if condition.negated {
            negate(expression)
        } else {
            expression
        })
    }

    fn table(&mut self, pc: &mut usize, active: &Frame) -> Option<HirExpr> {
        let start = *pc;
        let mut parser = ArrayParser {
            lowering: self.lowering,
            block: self.lowering.cfg.instr_to_block[*pc],
            root: Reg(active.len()),
            start: *pc,
            cursor: *pc,
            has_observable_producer: false,
            allow_open_tail: false,
            rk_pool_full: false,
        };
        parser.allow_open_tail = parser.has_later_list_batch(
            start,
            match self.lowering.proto.instrs[start] {
                LowInstr::NewTable(seed) => seed.dst,
                _ => return None,
            },
        );
        let mut expression = parser.table(0)?;
        let mut aliases = active.clone();
        for instruction in start..parser.cursor {
            for (reg, _) in self
                .lowering
                .dataflow
                .use_values_at(InstrRef(instruction))
                .iter()
            {
                if let Some(slot) = active.get(reg.index()) {
                    aliases.push(FrameSlot {
                        original: crate::hir::analyze::exprs::expr_for_reg_use(
                            self.lowering,
                            self.lowering.cfg.instr_to_block[instruction],
                            InstrRef(instruction),
                            reg,
                        ),
                        value: slot.value.clone(),
                    });
                }
            }
        }
        frames::remap_expression(&mut expression, &aliases)?;
        *pc = parser.cursor;
        self.expression_origin = Some((start, *pc));
        self.tables += 1;
        Some(expression)
    }
}

fn negate(expression: HirExpr) -> HirExpr {
    if let HirExpr::Unary(unary) = &expression
        && unary.op == HirUnaryOpKind::Not
    {
        return unary.expr.clone();
    }
    HirExpr::Unary(Box::new(HirUnaryExpr {
        op: HirUnaryOpKind::Not,
        expr: expression,
    }))
}
