//! branch-control 的词法 continuation 传播：只消费 HIR 已存在的顺序与 label。
//!
//! `if a then if b then copies; goto L end; tail end; ::L::` 的 arm 正常完成点
//! 就是 L，因此复用前向分支折叠把它收成 `if b then copies else tail end`。
//! continuation 只穿过尾部 if/do，不能穿过循环、资源边界或有正文的后继；
//! 不回扫 CFG、不发明标签，也不改变 edge copy 的原位求值顺序。

use super::*;
use crate::hir::simplify::walk::for_each_nested_block_mut;

pub(super) fn fold_lexical_continuations(
    block: &mut HirBlock,
    continuation: Option<HirLabelId>,
) -> bool {
    let mut changed = false;
    let mut next = continuation;
    for stmt in block.stmts.iter_mut().rev() {
        match stmt {
            HirStmt::Label(label) => {
                next = label.tbc_barriers.is_empty().then_some(label.id);
                continue;
            }
            HirStmt::If(branch) => {
                changed |= fold_lexical_continuations(&mut branch.then_block, next);
                if let Some(otherwise) = &mut branch.else_block {
                    changed |= fold_lexical_continuations(otherwise, next);
                }
                changed |= collapse_escape_guard(branch);
            }
            HirStmt::Block(body) => {
                changed |= fold_lexical_continuations(body, next);
            }
            _ => for_each_nested_block_mut(stmt, &mut |body| {
                changed |= fold_lexical_continuations(body, None);
            }),
        }
        next = None;
    }

    if let Some(target) = continuation {
        changed |= fold_forward_gotos(&mut block.stmts, FoldKind::TerminalElse, Some(target));
        changed |= fold_forward_gotos(&mut block.stmts, FoldKind::Guard, Some(target));
        if matches!(block.stmts.last(), Some(HirStmt::Goto(jump)) if jump.target == target) {
            block.stmts.pop();
            changed = true;
        }
    }
    changed
}

fn collapse_escape_guard(branch: &mut HirIf) -> bool {
    if branch
        .else_block
        .as_ref()
        .is_some_and(|body| !body.stmts.is_empty())
    {
        return false;
    }
    let [HirStmt::If(inner)] = branch.then_block.stmts.as_slice() else {
        return false;
    };
    if inner
        .else_block
        .as_ref()
        .is_some_and(|body| !body.stmts.is_empty())
        || !matches!(inner.then_block.stmts.last(), Some(HirStmt::Goto(_)))
    {
        return false;
    }
    let HirStmt::If(inner) = branch.then_block.stmts.pop().expect("matched single guard") else {
        unreachable!();
    };
    branch.cond = HirExpr::LogicalAnd(Box::new(HirLogicalExpr {
        lhs: std::mem::replace(&mut branch.cond, HirExpr::Boolean(false)),
        rhs: inner.cond,
    }));
    branch.then_block = inner.then_block;
    true
}
