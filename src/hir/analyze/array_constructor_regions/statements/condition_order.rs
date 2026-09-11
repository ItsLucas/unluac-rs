//! 为规范化 LT/LE 选择有原始槽与常量池证据的源码操作数顺序。
//!
//! 例如先 `GETTABLE GetLevel; CALL` 后比较新常量 0，须打印 `GetLevel() > 0`；
//! `0 < FreshCall()` 则须先插入 0。依赖完整表达式 origin、已证低于 RK 容量的池、
//! 前序实际常量引用，以及 computed 操作数原始相邻槽。不能用打印复杂度猜顺序，
//! EQ 也不能交换两个有计算的操作数来修补顺序。
//! 完整赋值的父 global／索引 key 可在表达式前保留，不能把 RHS 对该 key 的重读
//! 当成本次新增常量。已存在 local 与旧 literal 本身无求值写槽，EQ 固定语义两端。

use super::*;
use crate::hir::RelationalOperandOrder;
use crate::transformer::BranchPredicate;

impl StatementParser<'_, '_> {
    pub(super) fn relational_order(
        &self,
        pc: usize,
        subject: BranchSubject,
        active: &Frame,
        temporary: Option<&[HirExpr]>,
        reserved_key: Option<ConstRef>,
    ) -> Option<RelationalOperandOrder> {
        use RelationalOperandOrder::{LeftFirst, RightFirst};
        let BranchSubject::Compare {
            predicate,
            lhs,
            rhs,
        } = subject
        else {
            return None;
        };
        let count = temporary.map_or(0, <[HirExpr]>::len);
        let temp = |operand| match operand {
            CondOperand::Reg(reg) if reg.index() >= active.len() => {
                Some(reg.index() - active.len())
            }
            _ => None,
        };
        let mut strict_order = false;
        let order = match (count, temp(lhs), temp(rhs)) {
            (2, Some(0), Some(1)) => LeftFirst,
            (2, Some(1), Some(0)) => {
                strict_order = true;
                RightFirst
            }
            (0, None, None) => {
                let previous = self.lowering.proto.instrs[..pc]
                    .iter()
                    .flat_map(assignments::constant_indices)
                    .max();
                match (lhs, rhs) {
                    (CondOperand::Const(left), CondOperand::Const(right))
                        if previous.is_none_or(|previous| {
                            left.index() > previous && right.index() > previous
                        }) && right.index() < left.index() =>
                    {
                        strict_order = true;
                        RightFirst
                    }
                    _ => LeftFirst,
                }
            }
            (1, Some(0), None) | (1, None, Some(0)) => {
                let (start, end) = self.expression_origin?;
                if end != pc || start >= end {
                    return None;
                }
                let left_is_temp = temp(lhs).is_some();
                let other = if left_is_temp { rhs } else { lhs };
                let mut constant_first = false;
                if let CondOperand::Const(constant) = other {
                    let previous = self.lowering.proto.instrs[..start]
                        .iter()
                        .flat_map(assignments::constant_indices)
                        .max();
                    if Some(constant) != reserved_key
                        && previous.is_none_or(|previous| constant.index() > previous)
                    {
                        let first_new_other = self.lowering.proto.instrs[start..end]
                            .iter()
                            .flat_map(assignments::constant_indices)
                            .filter(|index| {
                                *index != constant.index()
                                    && reserved_key.is_none_or(|key| *index != key.index())
                                    && previous.is_none_or(|previous| *index > previous)
                            })
                            .min();
                        strict_order = first_new_other.is_some();
                        constant_first =
                            first_new_other.is_some_and(|other| constant.index() < other);
                    }
                }
                if left_is_temp != constant_first {
                    LeftFirst
                } else {
                    RightFirst
                }
            }
            _ => return None,
        };
        if predicate == BranchPredicate::Eq {
            if strict_order && order == RightFirst {
                return None;
            }
            return Some(LeftFirst);
        }
        Some(order)
    }
}
