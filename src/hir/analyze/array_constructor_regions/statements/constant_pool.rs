//! 声明必须按源码求值顺序重现常量首次插入，不能只看最终常量集合。
//!
//! 例如 `31415 < Fresh(31415, Key)` 的调用区间也引用全部常量，但若把调用单独
//! 声明，Fresh 会抢先插入。这里消费完整 HIR 初始化式及原始前缀池状态，重放
//! Lua 5.1 的字符串即时插入、数值延迟物化、recfield 键先值后与比较顺序。
//! 未覆盖的表达式拒绝成为独立声明，不改变原指令、物理槽或控制流。

use super::*;
use crate::hir::RelationalOperandOrder;

#[derive(Clone, Copy)]
enum Pending {
    Value,
    Number(f64),
    Nil,
    Boolean(bool),
}

struct Pool {
    constants: Vec<HirExpr>,
    next: usize,
}

impl StatementParser<'_, '_> {
    pub(super) fn initializer_constants_closed(&self, expression: &HirExpr) -> Option<()> {
        let (start, end) = self.expression_origin?;
        if start >= end || end > self.lowering.proto.instrs.len() {
            return None;
        }
        let next = self.lowering.proto.instrs[..start]
            .iter()
            .flat_map(assignments::constant_indices)
            .max()
            .map_or(0, |index| index + 1);
        let expected = self.lowering.proto.instrs[start..end]
            .iter()
            .flat_map(assignments::constant_indices)
            .max()
            .map_or(next, |index| next.max(index + 1));
        let mut pool = Pool {
            constants: (0..self.lowering.proto.constants.common.literals.len())
                .map(|index| {
                    expr_for_const(self.lowering.proto, crate::transformer::ConstRef(index))
                })
                .collect(),
            next,
        };
        let value = pool.expression(expression)?;
        pool.register(value)?;
        (pool.next == expected).then_some(())
    }
}

impl Pool {
    fn index(&mut self, index: usize) -> Option<()> {
        if index > self.next {
            return None;
        }
        if index == self.next {
            self.next += 1;
        }
        Some(())
    }

    fn literal(&mut self, value: &HirExpr) -> Option<()> {
        let index = self
            .constants
            .iter()
            .position(|constant| match (constant, value) {
                (HirExpr::Number(a), HirExpr::Number(b)) => a.to_bits() == b.to_bits(),
                _ => constant == value,
            })?;
        self.index(index)
    }

    fn name(&mut self, name: &str) -> Option<()> {
        let index = self.constants.iter().position(|constant| matches!(constant,
            HirExpr::String(value) if value.decoded_text() == Some(name) || value.as_utf8() == Some(name)))?;
        self.index(index)
    }

    fn register(&mut self, value: Pending) -> Option<()> {
        if let Pending::Number(number) = value {
            self.literal(&HirExpr::Number(number))?;
        }
        Some(())
    }

    fn rk(&mut self, value: Pending) -> Option<()> {
        match value {
            Pending::Nil => self.literal(&HirExpr::Nil),
            Pending::Boolean(value) => self.literal(&HirExpr::Boolean(value)),
            _ => self.register(value),
        }
    }

    fn expression(&mut self, expression: &HirExpr) -> Option<Pending> {
        match expression {
            HirExpr::Number(value) if value.is_finite() => return Some(Pending::Number(*value)),
            HirExpr::Integer(value) => return Some(Pending::Number(*value as f64)),
            HirExpr::Nil => return Some(Pending::Nil),
            HirExpr::Boolean(value) => return Some(Pending::Boolean(*value)),
            HirExpr::String(_) => self.literal(expression)?,
            HirExpr::LocalRef(_)
            | HirExpr::ParamRef(_)
            | HirExpr::TempRef(_)
            | HirExpr::UpvalueRef(_) => {}
            HirExpr::GlobalRef(global) => self.name(&global.name)?,
            HirExpr::TableAccess(access) => {
                let base = self.expression(&access.base)?;
                self.register(base)?;
                let key = self.expression(&access.key)?;
                self.rk(key)?;
            }
            HirExpr::Call(call) => {
                if call.method || call.method_name.is_some() || call.fastcall.is_some() {
                    return None;
                }
                let callee = self.expression(&call.callee)?;
                self.register(callee)?;
                for value in call.args.iter() {
                    let value = self.expression(value)?;
                    self.register(value)?;
                }
            }
            HirExpr::TableConstructor(table) => {
                for field in &table.fields {
                    match field {
                        HirTableField::Array(value) => {
                            let value = self.expression(value)?;
                            self.register(value)?;
                        }
                        HirTableField::Record(field) => {
                            match &field.key {
                                crate::hir::common::HirTableKey::Name(name) => self.name(name)?,
                                crate::hir::common::HirTableKey::Expr(key) => {
                                    let key = self.expression(key)?;
                                    self.rk(key)?;
                                }
                            }
                            let value = self.expression(&field.value)?;
                            self.rk(value)?;
                        }
                    }
                }
                if let Some(tail) = &table.trailing_multivalue {
                    let value = self.expression(tail.as_expr())?;
                    self.register(value)?;
                }
            }
            HirExpr::Closure(_) => {} // 子函数常量池独立，捕获操作不插入父池常量。
            HirExpr::Unary(unary) => {
                let value = self.expression(&unary.expr)?;
                if matches!(
                    value,
                    Pending::Number(_) | Pending::Nil | Pending::Boolean(_)
                ) {
                    // 这些形状可能常量折叠；完整帧不以额外 unary 指令冒充原始物化。
                    return None;
                }
                self.register(value)?;
            }
            HirExpr::Binary(binary) => match binary.op {
                HirBinaryOpKind::Eq | HirBinaryOpKind::Lt | HirBinaryOpKind::Le => {
                    let (first, second) = match binary.operand_order? {
                        RelationalOperandOrder::LeftFirst => (&binary.lhs, &binary.rhs),
                        RelationalOperandOrder::RightFirst => (&binary.rhs, &binary.lhs),
                    };
                    let first = self.expression(first)?;
                    self.rk(first)?;
                    let second = self.expression(second)?;
                    self.rk(second)?;
                }
                HirBinaryOpKind::Concat => {
                    let first = self.expression(&binary.lhs)?;
                    self.register(first)?;
                    let second = self.expression(&binary.rhs)?;
                    self.register(second)?;
                }
                HirBinaryOpKind::Add
                | HirBinaryOpKind::Sub
                | HirBinaryOpKind::Mul
                | HirBinaryOpKind::Div
                | HirBinaryOpKind::Mod
                | HirBinaryOpKind::Pow => {
                    let first = self.expression(&binary.lhs)?;
                    if !matches!(first, Pending::Number(_)) {
                        self.rk(first)?;
                    }
                    let second = self.expression(&binary.rhs)?;
                    if matches!((first, second), (Pending::Number(_), Pending::Number(_))) {
                        return None;
                    }
                    self.rk(second)?;
                    self.rk(first)?;
                }
                _ => return None,
            },
            _ => return None,
        }
        Some(Pending::Value)
    }
}
