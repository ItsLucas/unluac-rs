//! 在已认证源码帧内只遍历嵌套函数体，保护父函数的语句与表达式。
//!
//! 依赖 HIR 传递的 source_frame 证书；例如父函数的单次读取 local 不能被内联，
//! 其中未认证的闭包仍须执行自己的 AST 整理。父节点与词法作用域不会进入任何
//! readability 重写 hook；此遍历不负责重新证明证书，也不跳过子函数的正常处理。

use super::*;

type FunctionRewrite<'a> = dyn FnMut(&mut AstFunctionExpr) -> bool + 'a;

pub(super) fn rewrite_functions(block: &mut AstBlock, rewrite: &mut FunctionRewrite<'_>) -> bool {
    let mut changed = false;
    for statement in &mut block.stmts {
        traverse_stmt_children!(statement, iter = iter_mut, opt = as_mut, borrow = [&mut],
            expr(value) => { changed |= expression(value, rewrite); },
            lvalue(value) => { changed |= lvalue(value, rewrite); },
            block(block) => { changed |= rewrite_functions(block, rewrite); },
            function(function) => { changed |= rewrite(function); },
            condition(value) => { changed |= expression(value, rewrite); },
            call(value) => { changed |= call(value, rewrite); }
        );
    }
    changed
}

fn expression(value: &mut AstExpr, rewrite: &mut FunctionRewrite<'_>) -> bool {
    let mut changed = false;
    traverse_expr_children!(value, iter = iter_mut, borrow = [&mut],
        expr(value) => { changed |= expression(value, rewrite); },
        function(function) => { changed |= rewrite(function); }
    );
    changed
}

fn lvalue(value: &mut AstLValue, rewrite: &mut FunctionRewrite<'_>) -> bool {
    let mut changed = false;
    traverse_lvalue_children!(value, borrow = [&mut], expr(value) => {
        changed |= expression(value, rewrite);
    });
    changed
}

fn call(value: &mut AstCallKind, rewrite: &mut FunctionRewrite<'_>) -> bool {
    let mut changed = false;
    traverse_call_children!(value, iter = iter_mut, borrow = [&mut], expr(value) => {
        changed |= expression(value, rewrite);
    });
    changed
}
