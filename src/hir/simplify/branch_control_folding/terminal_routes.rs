//! branch-control 的共享终态路由：两条 predicate 保持每条路径上的单次终态执行。
//!
//! 输入 `if not a then goto F end; ::S:: return v; ::F:: if b then goto S end`
//! 收成 `if a or b then return v end`。第二项有 producer 前缀时保留有序的两个
//! early-return guard，前缀不被提升或复制。只读 HIR 条件、label 引用与终态事实；
//! 两个入口必须闭合，共享体只能是单个终态 transfer，前缀没有 local/cleanup/jump 边界。
//! 这不是任意向后跳转消除：每条动态路径仍只执行一次终态求值，不跨循环或重识别 CFG。

use super::*;

pub(super) fn fold_shared_terminal_routes(stmts: &mut Vec<HirStmt>) -> bool {
    let references = count_label_references(stmts);
    let labels = index_top_level_labels(stmts);
    let mut changed = false;
    for start in (0..stmts.len().saturating_sub(4)).rev() {
        let Some((failure, invert)) = fold_target(&stmts[start], FoldKind::Guard) else {
            continue;
        };
        let Some(&failure_index) = labels.get(&failure) else {
            continue;
        };
        if failure_index <= start + 2 || failure_index + 1 >= stmts.len() {
            continue;
        }
        let HirStmt::Label(success) = &stmts[start + 1] else {
            continue;
        };
        let HirStmt::Label(failure_label) = &stmts[failure_index] else {
            continue;
        };
        if failure_label.id != failure
            || !success.tbc_barriers.is_empty()
            || !failure_label.tbc_barriers.is_empty()
            || references.get(&success.id) != Some(&1)
            || references.get(&failure) != Some(&1)
        {
            continue;
        }
        let Some((second_index, second)) = find_second_guard(stmts, failure_index + 1, success.id)
        else {
            continue;
        };
        let body = &stmts[start + 2..failure_index];
        if !matches!(
            body,
            [HirStmt::Return(_) | HirStmt::Break | HirStmt::Continue]
        ) {
            continue;
        }
        let HirStmt::If(first) = &stmts[start] else {
            unreachable!();
        };
        let first = normalize_condition_context(&first.cond, !invert).expr;
        let prefix = &stmts[failure_index + 1..second_index];
        let guard = |cond| {
            HirStmt::If(Box::new(HirIf {
                cond,
                then_block: HirBlock {
                    stmts: body.to_vec(),
                },
                else_block: None,
            }))
        };
        let replacement = if prefix.is_empty() {
            vec![guard(HirExpr::LogicalOr(Box::new(HirLogicalExpr {
                lhs: first,
                rhs: second,
            })))]
        } else {
            let mut replacement = vec![guard(first)];
            replacement.extend_from_slice(prefix);
            replacement.push(guard(second));
            replacement
        };
        stmts.splice(start..second_index + 1, replacement);
        changed = true;
    }
    changed
}

fn find_second_guard(
    stmts: &[HirStmt],
    start: usize,
    target: HirLabelId,
) -> Option<(usize, HirExpr)> {
    for (index, stmt) in stmts.iter().enumerate().skip(start) {
        if let Some(cond) = leading_escape_condition(stmt, target) {
            return Some((index, cond));
        }
        let mut boundary = TerminalBoundary { safe: true };
        visit_stmts(std::slice::from_ref(stmt), &mut boundary);
        if !boundary.safe {
            return None;
        }
    }
    None
}

struct TerminalBoundary {
    safe: bool,
}

impl HirVisitor for TerminalBoundary {
    fn visit_stmt(&mut self, stmt: &HirStmt) {
        self.safe &= !matches!(
            stmt,
            HirStmt::LocalDecl(_)
                | HirStmt::Goto(_)
                | HirStmt::Label(_)
                | HirStmt::ToBeClosed(_)
                | HirStmt::Close(_)
                | HirStmt::Return(_)
                | HirStmt::Break
                | HirStmt::Continue
                | HirStmt::While(_)
                | HirStmt::Repeat(_)
                | HirStmt::NumericFor(_)
                | HirStmt::GenericFor(_)
        );
    }
}
