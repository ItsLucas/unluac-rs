//! 这个文件集中放 AST 输出层共享的“源码形态偏好”启发式。
//!
//! AST 本身只保存稳定语义，不会专门为 `elseif`、`>` / `>=` 之类的打印糖再扩一层
//! 语法节点。这里提供跨 debug / generate 共享的轻量规则，让不同输出入口在不改变
//! AST 语义的前提下，尽量收敛到更接近源码的文本形状。
//! 例如：默认步长的 numeric-for 可以在输出层省略第三个参数。
//! 有证关系式先遵从 HIR 传入的操作数顺序；例如 `call() > 0` 保留先调用的方向。
//! 无证表达式只允许标量字面量与已有局部绑定交换，字段、上值、全局读取及调用
//! 不按显示复杂度排序，避免改变求值顺序、常量入池时点或临时槽覆盖。

use super::common::{
    AstBinaryExpr, AstBinaryOpKind, AstExpr, AstNameRef, AstUnaryExpr, AstUnaryOpKind,
};
use crate::decompile::DecompileDialect;
use crate::hir::RelationalOperandOrder;

pub(crate) struct PreferredRelationalRender<'a> {
    pub(crate) lhs: &'a AstExpr,
    pub(crate) op_text: &'static str,
    pub(crate) rhs: &'a AstExpr,
}

pub(crate) fn preferred_relational_render(
    binary: &AstBinaryExpr,
) -> Option<PreferredRelationalRender<'_>> {
    let flipped = match binary.operand_order {
        Some(RelationalOperandOrder::LeftFirst) => false,
        Some(RelationalOperandOrder::RightFirst) => true,
        None => should_flip_relational_operands(&binary.lhs, &binary.rhs),
    };
    match (binary.op, flipped) {
        (AstBinaryOpKind::Lt, false) => Some(PreferredRelationalRender {
            lhs: &binary.lhs,
            op_text: "<",
            rhs: &binary.rhs,
        }),
        (AstBinaryOpKind::Lt, true) => Some(PreferredRelationalRender {
            lhs: &binary.rhs,
            op_text: ">",
            rhs: &binary.lhs,
        }),
        (AstBinaryOpKind::Le, false) => Some(PreferredRelationalRender {
            lhs: &binary.lhs,
            op_text: "<=",
            rhs: &binary.rhs,
        }),
        (AstBinaryOpKind::Le, true) => Some(PreferredRelationalRender {
            lhs: &binary.rhs,
            op_text: ">=",
            rhs: &binary.lhs,
        }),
        _ => None,
    }
}

pub(crate) fn preferred_negated_relational_render(
    unary: &AstUnaryExpr,
) -> Option<PreferredRelationalRender<'_>> {
    if unary.op != AstUnaryOpKind::Not {
        return None;
    }
    let AstExpr::Binary(binary) = &unary.expr else {
        return None;
    };
    if binary.op != AstBinaryOpKind::Eq {
        return None;
    }

    Some(PreferredRelationalRender {
        lhs: &binary.lhs,
        op_text: "~=",
        rhs: &binary.rhs,
    })
}

pub(crate) fn is_default_numeric_for_step(step: &AstExpr) -> bool {
    match step {
        AstExpr::Integer(1) => true,
        AstExpr::Number(value) => *value == 1.0,
        _ => false,
    }
}

pub(crate) fn is_default_numeric_for_step_for_target(
    step: &AstExpr,
    dialect: DecompileDialect,
) -> bool {
    match step {
        AstExpr::Integer(1) => true,
        AstExpr::Number(value) => {
            *value == 1.0
                && !matches!(
                    dialect,
                    DecompileDialect::Lua53 | DecompileDialect::Lua54 | DecompileDialect::Lua55
                )
        }
        _ => false,
    }
}

fn should_flip_relational_operands(lhs: &AstExpr, rhs: &AstExpr) -> bool {
    let scalar = matches!(
        lhs,
        AstExpr::Nil | AstExpr::Boolean(_) | AstExpr::Integer(_) | AstExpr::String(_)
    ) || matches!(lhs, AstExpr::Number(value) if value.is_finite());
    // `0 < local_value` 与 `local_value > 0` 编译时只需同一个常量操作数。
    // 其它表达式即使单独看无副作用，也可能物化到不同槽或插入新的常量。
    scalar
        && matches!(
            rhs,
            AstExpr::Var(
                AstNameRef::Local(_) | AstNameRef::Param(_) | AstNameRef::SyntheticLocal(_)
            )
        )
}
