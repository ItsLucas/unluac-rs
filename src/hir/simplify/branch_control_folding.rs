//! branch-control 收敛：删除无求值行为的空/常量分支，把公共 direct-copy/goto 尾部移出分支，
//! 将 repeat 尾部的单次 break guard 收回 until 条件，并把残留 goto 壳恢复成普通条件结构。
//!
//! 这里只消费已经存在的 `If/Goto/Label`，不重新解释 CFG，也不接管同一 lvalue 选值；
//! branch-value 形状仍由 `branch_value_folding` 先处理。每轮先为当前 block 建一次 label
//! 位置和引用计数，再按不交叉区间从右向左改写，避免多个 guard 共用 label 时反复全块
//! 扫描和重建。
//! 词法 continuation 只穿过尾部 if/do 传播，共享终态路由只消费封闭入口与单个终态
//! transfer；两者都不跨 loop/cleanup 边界，也不移动 predicate producer 或 edge copy。
//!
//! 例如 `if false then body end` 会被删除，`if true then body end` 会保留原 branch block
//! 的词法作用域后去掉条件壳；动态 lookup、调用、table 构造与元方法比较都不进入该规则。

mod continuations;
mod path_conditions;
mod terminal_routes;

use std::collections::BTreeMap;

use crate::hir::common::{
    HirBlock, HirCallExpr, HirCallStmt, HirExpr, HirIf, HirLabelId, HirLogicalExpr, HirProto,
    HirStmt, HirUnaryOpKind, LocalId,
};
use crate::hir::expr_safety::{expr_is_discard_safe, expr_is_repeatable};

use super::carried_locals::{CarryBinding, single_binding_copy};
use super::expr_facts::expr_truthiness;
use super::label_refs::count_label_references;
use super::logical_simplify::{normalize_condition_context, simplify_condition_truthiness_shape};
use super::visit::{HirVisitor, visit_block, visit_expr, visit_stmts};
use super::walk::{HirRewritePass, rewrite_proto};

pub(super) fn fold_branch_control_in_proto(proto: &mut HirProto) -> bool {
    let mut changed = false;
    loop {
        let path_changed = path_conditions::specialize_stable_path_conditions(proto);
        changed |= continuations::fold_lexical_continuations(&mut proto.body, None);
        changed |= path_changed | rewrite_proto(proto, &mut BranchControlPass);
        // 删除不可达写可能让下一项 local 立刻满足稳定性证明。这里收完本 pass 自己的
        // 单调链，避免合法的长链逐项消耗全局 scheduler 的固定轮次预算。
        if !path_changed {
            return changed;
        }
    }
}

struct BranchControlPass;

impl HirRewritePass for BranchControlPass {
    fn rewrite_block(&mut self, block: &mut HirBlock) -> bool {
        let constant_changed = fold_constant_control(&mut block.stmts);
        let common_tail_changed = sink_common_branch_tails(&mut block.stmts);
        let empty_changed = remove_discard_safe_empty_ifs(&mut block.stmts);
        let terminal_routes_changed =
            terminal_routes::fold_shared_terminal_routes(&mut block.stmts);
        let alternative_guard_changed = fold_alternative_guard_routes(&mut block.stmts);
        let leading_escape_changed = fold_leading_branch_escapes(&mut block.stmts);
        let terminal_changed = fold_forward_gotos(&mut block.stmts, FoldKind::TerminalElse, None);
        let guard_changed = fold_forward_gotos(&mut block.stmts, FoldKind::Guard, None);
        let nop_changed = remove_nop_goto_labels(&mut block.stmts);
        constant_changed
            || common_tail_changed
            || empty_changed
            || terminal_routes_changed
            || alternative_guard_changed
            || leading_escape_changed
            || terminal_changed
            || guard_changed
            || nop_changed
    }

    fn rewrite_stmt(&mut self, stmt: &mut HirStmt) -> bool {
        fold_sibling_branch_escapes(stmt)
            || fold_trailing_repeat_break_condition(stmt)
            || fold_effect_only_call(stmt)
            || fold_leading_while_break_guard(stmt)
            || naturalize_if_polarity(stmt)
    }
}

// `if a then goto S end; if b then goto F end; ::S::` checks b only
// when a is false. Combine the guard without duplicating either evaluation.
fn fold_alternative_guard_routes(stmts: &mut Vec<HirStmt>) -> bool {
    let references = count_label_references(stmts);
    let mut changed = false;
    let mut index = 0;
    while index + 2 < stmts.len() {
        let HirStmt::Label(success) = &stmts[index + 2] else {
            index += 1;
            continue;
        };
        if !success.tbc_barriers.is_empty() || references.get(&success.id).copied() != Some(1) {
            index += 1;
            continue;
        }
        let Some(succeeds) = leading_escape_condition(&stmts[index], success.id) else {
            index += 1;
            continue;
        };
        let Some((failure, invert)) = fold_target(&stmts[index + 1], FoldKind::Guard) else {
            index += 1;
            continue;
        };
        if failure == success.id {
            index += 1;
            continue;
        }
        let HirStmt::If(guard) = &stmts[index + 1] else {
            unreachable!();
        };
        let fails = normalize_condition_context(&guard.cond, invert).expr;
        let combined = HirStmt::If(Box::new(HirIf {
            cond: HirExpr::LogicalAnd(Box::new(HirLogicalExpr {
                lhs: normalize_condition_context(&succeeds, true).expr,
                rhs: fails,
            })),
            then_block: HirBlock {
                stmts: vec![HirStmt::Goto(Box::new(crate::hir::HirGoto {
                    target: failure,
                }))],
            },
            else_block: None,
        }));
        stmts.splice(index..index + 3, [combined]);
        changed = true;
        index = index.saturating_sub(2);
    }
    changed
}

// 入口 guard 可以进入另一 arm 的首 label，或进入整个 if 的词法 continuation；
// 只合并条件，保留各 arm 正文与 producer/cleanup 的原有作用域和执行顺序。
fn fold_leading_branch_escapes(stmts: &mut [HirStmt]) -> bool {
    let mut changed = false;
    for index in 0..stmts.len().saturating_sub(1) {
        let HirStmt::Label(label) = &stmts[index + 1] else {
            continue;
        };
        if !label.tbc_barriers.is_empty() {
            continue;
        }
        let target = label.id;
        let HirStmt::If(branch) = &mut stmts[index] else {
            continue;
        };
        if branch
            .else_block
            .as_ref()
            .is_some_and(|block| !block.stmts.is_empty())
        {
            continue;
        }
        changed |= fold_arm_escape(&mut branch.cond, &mut branch.then_block, target, true);
    }
    changed
}

fn fold_sibling_branch_escapes(stmt: &mut HirStmt) -> bool {
    // `if a then if b then T else goto E end else ::E:: F end`
    // 收成 `if a and b then T else F end`；label 仍由全 proto 引用清扫决定是否删除。
    let HirStmt::If(branch) = stmt else {
        return false;
    };
    let Some(otherwise) = &mut branch.else_block else {
        return false;
    };
    if let Some(HirStmt::Label(label)) = otherwise.stmts.first()
        && label.tbc_barriers.is_empty()
        && fold_arm_escape(&mut branch.cond, &mut branch.then_block, label.id, true)
    {
        return true;
    }
    if let Some(HirStmt::Label(label)) = branch.then_block.stmts.first()
        && label.tbc_barriers.is_empty()
    {
        return fold_arm_escape(&mut branch.cond, otherwise, label.id, false);
    }
    false
}

fn fold_arm_escape(
    cond: &mut HirExpr,
    body: &mut HirBlock,
    target: HirLabelId,
    from_then: bool,
) -> bool {
    let Some(escape) = take_leading_escape(body, target) else {
        return false;
    };
    let logical = Box::new(HirLogicalExpr {
        lhs: std::mem::replace(cond, HirExpr::Boolean(false)),
        rhs: normalize_condition_context(&escape, from_then).expr,
    });
    *cond = if from_then {
        HirExpr::LogicalAnd(logical)
    } else {
        HirExpr::LogicalOr(logical)
    };
    true
}

fn take_leading_escape(body: &mut HirBlock, target: HirLabelId) -> Option<HirExpr> {
    if let Some(escape) = body
        .stmts
        .first()
        .and_then(|guard| leading_escape_condition(guard, target))
    {
        body.stmts.remove(0);
        return Some(escape);
    }
    let [HirStmt::If(guard)] = body.stmts.as_slice() else {
        return None;
    };
    let then_escapes =
        matches!(guard.then_block.stmts.as_slice(), [HirStmt::Goto(jump)] if jump.target == target);
    let else_escapes = guard.else_block.as_ref().is_some_and(|otherwise| {
        matches!(otherwise.stmts.as_slice(), [HirStmt::Goto(jump)] if jump.target == target)
    });
    if then_escapes == else_escapes {
        return None;
    }
    let HirStmt::If(mut guard) = body.stmts.pop().expect("matched single branch") else {
        unreachable!();
    };
    let escape = normalize_condition_context(&guard.cond, !then_escapes).expr;
    *body = if then_escapes {
        guard.else_block.take().unwrap_or_default()
    } else {
        guard.then_block
    };
    Some(escape)
}

fn leading_escape_condition(stmt: &HirStmt, target: HirLabelId) -> Option<HirExpr> {
    let HirStmt::If(guard) = stmt else {
        return None;
    };
    if guard
        .else_block
        .as_ref()
        .is_some_and(|block| !block.stmts.is_empty())
    {
        return None;
    }
    let [child] = guard.then_block.stmts.as_slice() else {
        return None;
    };
    match child {
        HirStmt::Goto(jump) if jump.target == target => Some(guard.cond.clone()),
        HirStmt::If(_) => Some(HirExpr::LogicalAnd(Box::new(HirLogicalExpr {
            lhs: guard.cond.clone(),
            rhs: leading_escape_condition(child, target)?,
        }))),
        _ => None,
    }
}

fn sink_common_branch_tails(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = fold_terminal_else_tail(stmts);
    let original = std::mem::take(stmts);
    let mut rewritten = Vec::with_capacity(original.len());

    for stmt in original {
        let HirStmt::If(mut if_stmt) = stmt else {
            rewritten.push(stmt);
            continue;
        };
        let Some(common_tail) = take_common_direct_copy_tail(&mut if_stmt)
            .or_else(|| take_common_goto_tail(&mut if_stmt))
        else {
            rewritten.push(HirStmt::If(if_stmt));
            continue;
        };
        rewritten.push(HirStmt::If(if_stmt));
        rewritten.push(common_tail);
        changed = true;
    }

    *stmts = rewritten;
    changed
}

fn fold_terminal_else_tail(stmts: &mut Vec<HirStmt>) -> bool {
    let Some((tail, prefix)) = stmts.split_last() else {
        return false;
    };
    if !matches!(
        tail,
        HirStmt::Return(_) | HirStmt::Break | HirStmt::Continue
    ) {
        return false;
    }
    let Some(branch) = prefix.last() else {
        return false;
    };
    let Some((_, invert)) = fold_target(branch, FoldKind::TerminalElse)
        .or_else(|| fold_target(branch, FoldKind::Guard))
    else {
        return false;
    };
    let tail = stmts.pop().expect("matched terminal successor");
    let Some(HirStmt::If(branch)) = stmts.last_mut() else {
        unreachable!();
    };
    if invert {
        branch.cond = normalize_condition_context(&branch.cond, true).expr;
        branch.then_block = branch.else_block.take().expect("matched inverted arm");
    }
    branch.else_block = Some(HirBlock { stmts: vec![tail] });
    true
}

fn take_common_direct_copy_tail(if_stmt: &mut HirIf) -> Option<HirStmt> {
    let else_block = if_stmt.else_block.as_ref()?;
    let then_tail = if_stmt.then_block.stmts.last()?;
    let else_tail = else_block.stmts.last()?;
    if then_tail != else_tail {
        return None;
    }
    let (target, source) = single_binding_copy(then_tail)?;
    if !arm_allows_direct_copy_sink(&if_stmt.then_block, target, source)
        || !arm_allows_direct_copy_sink(else_block, target, source)
    {
        return None;
    }

    take_common_tail(if_stmt)
}

fn take_common_goto_tail(if_stmt: &mut HirIf) -> Option<HirStmt> {
    let else_block = if_stmt.else_block.as_ref()?;
    let (target, pop_then, pop_else) =
        match (if_stmt.then_block.stmts.last()?, else_block.stmts.last()?) {
            (HirStmt::Goto(left), HirStmt::Goto(right)) if left.target == right.target => {
                (left.target, true, true)
            }
            (HirStmt::Goto(jump), HirStmt::Return(_) | HirStmt::Break | HirStmt::Continue) => {
                (jump.target, true, false)
            }
            (HirStmt::Return(_) | HirStmt::Break | HirStmt::Continue, HirStmt::Goto(jump)) => {
                (jump.target, false, true)
            }
            _ => return None,
        };
    let mut boundary = GotoTailBoundary {
        target,
        defined_inside: false,
    };
    visit_block(&if_stmt.then_block, &mut boundary);
    visit_block(else_block, &mut boundary);
    if boundary.defined_inside {
        return None;
    }
    // 两臂先完成各自 cleanup 再跳到共同外部目标；移到 if 后没有新增求值事件。
    // 已 return/break/continue 的臂不会执行后置 goto；其终态求值仍留在原 arm。
    if pop_then && pop_else {
        take_common_tail(if_stmt)
    } else if pop_then {
        if_stmt.then_block.stmts.pop()
    } else {
        if_stmt.else_block.as_mut()?.stmts.pop()
    }
}

struct GotoTailBoundary {
    target: HirLabelId,
    defined_inside: bool,
}

impl HirVisitor for GotoTailBoundary {
    fn visit_stmt(&mut self, stmt: &HirStmt) {
        self.defined_inside |= matches!(stmt, HirStmt::Label(label) if label.id == self.target);
    }
}

fn take_common_tail(if_stmt: &mut HirIf) -> Option<HirStmt> {
    let common_tail = if_stmt.then_block.stmts.pop()?;
    let removed_else_tail = if_stmt.else_block.as_mut()?.stmts.pop();
    debug_assert_eq!(removed_else_tail.as_ref(), Some(&common_tail));
    Some(common_tail)
}

fn arm_allows_direct_copy_sink(
    block: &HirBlock,
    target: CarryBinding,
    source: CarryBinding,
) -> bool {
    let mut visitor = DirectCopySinkBoundary {
        locals: [target.local(), source.local()],
        safe: true,
    };
    visit_block(block, &mut visitor);
    visitor.safe
}

struct DirectCopySinkBoundary {
    locals: [Option<LocalId>; 2],
    safe: bool,
}

impl DirectCopySinkBoundary {
    fn introduces(&self, local: LocalId) -> bool {
        self.locals.contains(&Some(local))
    }
}

impl HirVisitor for DirectCopySinkBoundary {
    fn visit_stmt(&mut self, stmt: &HirStmt) {
        self.safe &= match stmt {
            HirStmt::LocalDecl(local_decl) => !local_decl
                .bindings
                .iter()
                .any(|local| self.introduces(*local)),
            HirStmt::NumericFor(numeric_for) => !self.introduces(numeric_for.binding),
            HirStmt::GenericFor(generic_for) => !generic_for
                .bindings
                .iter()
                .any(|local| self.introduces(*local)),
            HirStmt::ToBeClosed(_) | HirStmt::Close(_) => false,
            _ => true,
        };
    }
}

fn fold_constant_control(stmts: &mut Vec<HirStmt>) -> bool {
    let original = std::mem::take(stmts);
    let mut rewritten = Vec::with_capacity(original.len());
    let mut changed = false;

    for stmt in original {
        if let HirStmt::While(current) = &stmt
            && current.body.stmts.is_empty()
            && expr_is_repeatable(&current.cond)
            && matches!(rewritten.last(),
                Some(HirStmt::While(previous))
                    if previous.body.stmts.is_empty() && previous.cond == current.cond)
        {
            changed = true;
            continue;
        }
        if matches!(&stmt, HirStmt::Block(block) if block.stmts.is_empty())
            || matches!(
                &stmt,
                HirStmt::While(while_stmt) if while_stmt.cond == HirExpr::Boolean(false)
            )
        {
            changed = true;
            continue;
        }
        let HirStmt::If(mut if_stmt) = stmt else {
            rewritten.push(stmt);
            continue;
        };
        let selected_then = if expr_is_discard_safe(&if_stmt.cond)
            && !discard_safe_expr_has_unresolved(&if_stmt.cond)
        {
            expr_truthiness(&if_stmt.cond).or_else(|| {
                if_stmt
                    .else_block
                    .as_ref()
                    .is_some_and(|else_block| if_stmt.then_block == *else_block)
                    .then_some(true)
            })
        } else {
            None
        };
        let Some(selected_then) = selected_then else {
            rewritten.push(HirStmt::If(if_stmt));
            continue;
        };

        let selected = if selected_then {
            if_stmt.then_block
        } else {
            if_stmt.else_block.take().unwrap_or_default()
        };
        if !selected.stmts.is_empty() {
            rewritten.push(HirStmt::Block(Box::new(selected)));
        }
        changed = true;
    }

    *stmts = rewritten;
    changed
}

fn fold_effect_only_call(stmt: &mut HirStmt) -> bool {
    let HirStmt::If(if_stmt) = stmt else {
        return false;
    };
    if !if_arms_are_empty(if_stmt) {
        return false;
    }

    let Some(call) = take_effect_only_call(&mut if_stmt.cond) else {
        return false;
    };
    *stmt = HirStmt::CallStmt(Box::new(HirCallStmt { call: *call }));
    true
}

fn remove_discard_safe_empty_ifs(stmts: &mut Vec<HirStmt>) -> bool {
    let original_len = stmts.len();
    stmts.retain(|stmt| {
        !matches!(
            stmt,
            HirStmt::If(if_stmt)
                if if_arms_are_empty(if_stmt)
                    && expr_is_discard_safe(&if_stmt.cond)
                    && !discard_safe_expr_has_unresolved(&if_stmt.cond)
        )
    });
    stmts.len() != original_len
}

/// `expr_is_discard_safe` 可递归接纳的表达式形状中是否仍携带显式诊断。
fn discard_safe_expr_has_unresolved(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Unresolved(_) => true,
        HirExpr::Unary(unary) => discard_safe_expr_has_unresolved(&unary.expr),
        HirExpr::Binary(binary) => {
            discard_safe_expr_has_unresolved(&binary.lhs)
                || discard_safe_expr_has_unresolved(&binary.rhs)
        }
        HirExpr::LogicalAnd(logical) | HirExpr::LogicalOr(logical) => {
            discard_safe_expr_has_unresolved(&logical.lhs)
                || discard_safe_expr_has_unresolved(&logical.rhs)
        }
        _ => false,
    }
}

fn if_arms_are_empty(if_stmt: &HirIf) -> bool {
    if_stmt.then_block.stmts.is_empty()
        && if_stmt
            .else_block
            .as_ref()
            .is_none_or(|block| block.stmts.is_empty())
}

fn take_effect_only_call(mut expr: &mut HirExpr) -> Option<Box<HirCallExpr>> {
    loop {
        match expr {
            HirExpr::Call(_) => {
                let HirExpr::Call(call) = std::mem::replace(expr, HirExpr::Nil) else {
                    unreachable!("matched call must remain a call")
                };
                return Some(call);
            }
            HirExpr::Unary(unary) if unary.op == HirUnaryOpKind::Not => {
                expr = &mut unary.expr;
            }
            _ => return None,
        }
    }
}

fn fold_trailing_repeat_break_condition(stmt: &mut HirStmt) -> bool {
    let HirStmt::Repeat(repeat_stmt) = stmt else {
        return false;
    };
    let Some((tail, prefix)) = repeat_stmt.body.stmts.split_last() else {
        return false;
    };
    let HirStmt::If(outer) = tail else {
        return false;
    };
    if !matches!(outer.then_block.stmts.as_slice(), [HirStmt::Break]) {
        return false;
    }

    let (nested_else, outer_cond, moved_cond) = if let Some(else_block) = &outer.else_block {
        let [HirStmt::If(nested)] = else_block.stmts.as_slice() else {
            return false;
        };
        if nested.else_block.is_some()
            || !matches!(nested.then_block.stmts.as_slice(), [HirStmt::Break])
        {
            return false;
        }
        (true, Some(&outer.cond), &nested.cond)
    } else {
        (false, None, &outer.cond)
    };
    if matches!(moved_cond, HirExpr::LogicalOr(_))
        || matches!(repeat_stmt.cond, HirExpr::LogicalOr(_))
        || !repeat_condition_fold_is_safe(
            prefix,
            outer_cond
                .into_iter()
                .chain([moved_cond, &repeat_stmt.cond]),
        )
    {
        return false;
    }

    let lhs = if nested_else {
        let Some(HirStmt::If(outer)) = repeat_stmt.body.stmts.last_mut() else {
            unreachable!("validated repeat tail must remain an if");
        };
        let mut nested_stmts = outer
            .else_block
            .take()
            .expect("validated repeat tail must retain its else block")
            .stmts;
        let Some(HirStmt::If(nested)) = nested_stmts.pop() else {
            unreachable!("validated repeat else must contain one if");
        };
        nested.cond
    } else {
        let Some(HirStmt::If(guard)) = repeat_stmt.body.stmts.pop() else {
            unreachable!("validated repeat tail must remain an if");
        };
        guard.cond
    };
    let rhs = std::mem::replace(&mut repeat_stmt.cond, HirExpr::Boolean(false));
    let folded = HirExpr::LogicalOr(Box::new(HirLogicalExpr { lhs, rhs }));
    // branch-control synthesizes this condition after the general logical pass.  Re-run only
    // the condition-safe normalizer here so shared stable guards are absorbed without changing
    // Lua value semantics in ordinary expression positions.
    repeat_stmt.cond = simplify_condition_truthiness_shape(&folded).unwrap_or(folded);
    true
}

fn repeat_condition_fold_is_safe<'a>(
    prefix: &[HirStmt],
    exprs: impl IntoIterator<Item = &'a HirExpr>,
) -> bool {
    let mut boundary = RepeatConditionFoldBoundary { safe: true };
    visit_stmts(prefix, &mut boundary);
    for expr in exprs {
        visit_expr(expr, &mut boundary);
    }
    boundary.safe
}

struct RepeatConditionFoldBoundary {
    safe: bool,
}

impl HirVisitor for RepeatConditionFoldBoundary {
    fn visit_stmt(&mut self, stmt: &HirStmt) {
        self.safe &= !matches!(
            stmt,
            HirStmt::ToBeClosed(_)
                | HirStmt::Close(_)
                | HirStmt::Continue
                | HirStmt::Goto(_)
                | HirStmt::Label(_)
        );
    }

    fn visit_expr(&mut self, expr: &HirExpr) {
        self.safe &= !matches!(expr, HirExpr::Decision(_) | HirExpr::Unresolved(_));
    }
}

fn fold_leading_while_break_guard(stmt: &mut HirStmt) -> bool {
    let HirStmt::While(while_stmt) = stmt else {
        return false;
    };
    if while_stmt.cond != HirExpr::Boolean(true) {
        return false;
    }
    let Some(HirStmt::If(guard)) = while_stmt.body.stmts.first() else {
        return false;
    };
    if guard.else_block.is_some() || !matches!(guard.then_block.stmts.as_slice(), [HirStmt::Break])
    {
        return false;
    }
    while_stmt.cond = normalize_condition_context(&guard.cond, true).expr;
    while_stmt.body.stmts.remove(0);
    true
}

fn naturalize_if_polarity(stmt: &mut HirStmt) -> bool {
    let HirStmt::If(if_stmt) = stmt else {
        return false;
    };
    let Some(else_block) = if_stmt.else_block.as_ref() else {
        return false;
    };
    if if_stmt.then_block.stmts.is_empty() || else_block.stmts.is_empty() {
        return false;
    }

    let current = normalize_condition_context(&if_stmt.cond, false);
    let negated = normalize_condition_context(&if_stmt.cond, true);
    if negated.not_cost < current.not_cost {
        let Some(else_block) = if_stmt.else_block.as_mut() else {
            return false;
        };
        if_stmt.cond = negated.expr;
        std::mem::swap(&mut if_stmt.then_block, else_block);
        return true;
    }

    if current.changed {
        if_stmt.cond = current.expr;
        return true;
    }
    false
}

#[derive(Clone, Copy)]
enum FoldKind {
    TerminalElse,
    Guard,
}

struct FoldGroup {
    label: HirLabelId,
    label_index: usize,
    candidates: Vec<FoldCandidate>,
}

#[derive(Clone, Copy)]
struct FoldCandidate {
    if_index: usize,
    invert_cond: bool,
}

fn fold_forward_gotos(
    stmts: &mut Vec<HirStmt>,
    kind: FoldKind,
    continuation: Option<HirLabelId>,
) -> bool {
    let mut label_indices = index_top_level_labels(stmts);
    if let Some(target) = continuation {
        label_indices.entry(target).or_insert(stmts.len());
    }
    let label_refs = count_label_references(stmts);
    let mut groups = BTreeMap::<usize, FoldGroup>::new();

    for (if_index, stmt) in stmts.iter().enumerate() {
        let Some((target, invert_cond)) = fold_target(stmt, kind) else {
            continue;
        };
        let Some(label_index) = label_indices.get(&target).copied() else {
            continue;
        };
        if label_index <= if_index + 1 {
            continue;
        }
        let body = &stmts[(if_index + 1)..label_index];
        // 同 block 的值合流先交给 branch-values；祖先 continuation 尚无本地 label，
        // 必须先恢复 if/else 的控制壳，才能让值 pass 消费 arm 内原位的并行 copy。
        if !can_move_into_branch(body, kind)
            || matches!(kind, FoldKind::TerminalElse)
                && label_index < stmts.len()
                && is_branch_value_assignment(stmt, body, invert_cond)
        {
            continue;
        }
        groups
            .entry(label_index)
            .or_insert_with(|| FoldGroup {
                label: target,
                label_index,
                candidates: Vec::new(),
            })
            .candidates
            .push(FoldCandidate {
                if_index,
                invert_cond,
            });
    }

    if groups.is_empty() {
        return false;
    }

    // 可移动区间不含顶层 label，因此不同目标的区间不会交叉。倒序改写可保持更早
    // 区间的原始索引稳定；同一 label 的多个 guard 在一次改写中直接嵌套。
    for group in groups.into_values().rev() {
        let keep_label = group.label_index < stmts.len()
            && label_refs.get(&group.label).copied().unwrap_or_default() > group.candidates.len();
        rewrite_fold_group(stmts, group, kind, keep_label);
    }
    true
}

fn rewrite_fold_group(
    stmts: &mut Vec<HirStmt>,
    group: FoldGroup,
    kind: FoldKind,
    keep_label: bool,
) {
    let first = group.candidates[0].if_index;
    let mut next = group.label_index;
    let mut nested = Vec::new();

    for candidate in group.candidates.into_iter().rev() {
        let if_index = candidate.if_index;
        let mut body = stmts[(if_index + 1)..next].to_vec();
        body.append(&mut nested);
        let HirStmt::If(if_stmt) = stmts[if_index].clone() else {
            unreachable!("branch-control fold index must point to an if")
        };
        nested = vec![HirStmt::If(Box::new(rewrite_if(
            *if_stmt,
            body,
            kind,
            candidate.invert_cond,
        )))];
        next = if_index;
    }

    if keep_label {
        nested.push(stmts[group.label_index].clone());
    }
    let end = (group.label_index + 1).min(stmts.len());
    stmts.splice(first..end, nested);
}

fn rewrite_if(mut if_stmt: HirIf, body: Vec<HirStmt>, kind: FoldKind, invert_cond: bool) -> HirIf {
    if invert_cond {
        if_stmt.cond = if_stmt.cond.negate();
        if_stmt.then_block = if_stmt
            .else_block
            .take()
            .expect("inverted fold must have an else block");
    }
    match kind {
        FoldKind::TerminalElse => {
            let popped = if_stmt.then_block.stmts.pop();
            debug_assert!(matches!(popped, Some(HirStmt::Goto(_))));
            if_stmt.else_block = Some(HirBlock { stmts: body });
        }
        FoldKind::Guard => {
            if_stmt.cond = if_stmt.cond.negate();
            if_stmt.then_block = HirBlock { stmts: body };
            if_stmt.else_block = None;
        }
    }
    if_stmt
}

fn fold_target(stmt: &HirStmt, kind: FoldKind) -> Option<(HirLabelId, bool)> {
    let HirStmt::If(if_stmt) = stmt else {
        return None;
    };
    let else_block = if_stmt.else_block.as_ref();
    let (branch, invert_cond) = match else_block {
        Some(else_block) if if_stmt.then_block.stmts.is_empty() => (else_block, true),
        Some(else_block) if else_block.stmts.is_empty() => (&if_stmt.then_block, false),
        None => (&if_stmt.then_block, false),
        Some(_) => return None,
    };
    match kind {
        FoldKind::TerminalElse => {
            if branch.stmts.len() < 2 {
                return None;
            }
            let HirStmt::Goto(goto) = branch.stmts.last()? else {
                return None;
            };
            Some((goto.target, invert_cond))
        }
        FoldKind::Guard => {
            let [HirStmt::Goto(goto)] = branch.stmts.as_slice() else {
                return None;
            };
            Some((goto.target, invert_cond))
        }
    }
}

fn can_move_into_branch(stmts: &[HirStmt], kind: FoldKind) -> bool {
    // `if cond then goto A end; goto B; ::A::` 是 island 常见的双向 guard。
    // 把唯一的备用 goto 收进反向 arm 不改变 transfer，只减少一层壳；最终 AST
    // scope verifier 仍负责确认目标 label 对嵌套 arm 可见且没有跳进 local/TBC。
    // guard 的正文可以在最后执行另一条 transfer；它仍只在同一条件下执行一次。
    // 只允许尾部 goto，不能把中途跳过的语句误当作正常顺序正文。
    let stmts = if matches!(kind, FoldKind::Guard) && matches!(stmts.last(), Some(HirStmt::Goto(_)))
    {
        &stmts[..stmts.len() - 1]
    } else {
        stmts
    };
    stmts.iter().all(|stmt| {
        !matches!(
            stmt,
            HirStmt::LocalDecl(_)
                | HirStmt::Goto(_)
                | HirStmt::Label(_)
                | HirStmt::ToBeClosed(_)
                | HirStmt::Close(_)
        )
    })
}

fn is_branch_value_assignment(if_stmt: &HirStmt, else_body: &[HirStmt], invert_cond: bool) -> bool {
    let HirStmt::If(if_stmt) = if_stmt else {
        return false;
    };
    let branch = if invert_cond {
        let Some(else_block) = if_stmt.else_block.as_ref() else {
            return false;
        };
        else_block
    } else {
        &if_stmt.then_block
    };
    let [HirStmt::Assign(then_assign), HirStmt::Goto(_)] = branch.stmts.as_slice() else {
        return false;
    };
    let [HirStmt::Assign(else_assign)] = else_body else {
        return false;
    };
    then_assign.targets == else_assign.targets
}

fn index_top_level_labels(stmts: &[HirStmt]) -> BTreeMap<HirLabelId, usize> {
    stmts
        .iter()
        .enumerate()
        .filter_map(|(index, stmt)| match stmt {
            HirStmt::Label(label) => Some((label.id, index)),
            _ => None,
        })
        .collect()
}

fn remove_nop_goto_labels(stmts: &mut Vec<HirStmt>) -> bool {
    let label_refs = count_label_references(stmts);
    let mut old = std::mem::take(stmts).into_iter().peekable();
    let mut rewritten = Vec::with_capacity(old.len());
    let mut changed = false;

    while let Some(stmt) = old.next() {
        let HirStmt::Goto(goto) = &stmt else {
            rewritten.push(stmt);
            continue;
        };
        let Some(HirStmt::Label(label)) = old.peek() else {
            rewritten.push(stmt);
            continue;
        };
        if goto.target != label.id {
            rewritten.push(stmt);
            continue;
        }

        let label = old.next().expect("peeked label must remain available");
        if label_refs.get(&goto.target).copied().unwrap_or_default() > 1 {
            rewritten.push(label);
        }
        changed = true;
    }

    *stmts = rewritten;
    changed
}
