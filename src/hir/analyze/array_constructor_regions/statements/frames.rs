//! 将原始 SSA/临时引用映射到已证明占据该物理槽的源码绑定。
//!
//! 例如循环合流后的旧 TempRef 和入环 LocalRef 可共同指向同一 pinned local；
//! 映射依赖父事务收集的真实 use-value 别名。隐藏控制槽不可读取，歧义映射必须拒绝。
//! 递归处理表字段、开放尾项、调用参数与闭包捕获，不创建补槽变量或更改 pack 宽度。

use super::*;

#[derive(Clone)]
pub(super) struct FrameSlot {
    pub(super) original: HirExpr,
    pub(super) value: Option<HirExpr>,
}
pub(super) type Frame = Vec<FrameSlot>;

pub(super) fn remap_expression(expression: &mut HirExpr, frame: &Frame) -> Option<()> {
    match expression {
        HirExpr::LocalRef(_) | HirExpr::TempRef(_) | HirExpr::ParamRef(_) => {
            let mut matches = frame.iter().filter(|slot| slot.original == *expression);
            let value = matches.next()?.value.clone()?;
            if matches.any(|slot| slot.value.as_ref() != Some(&value)) {
                return None;
            }
            *expression = value;
        }
        HirExpr::TableAccess(access) => {
            remap_expression(&mut access.base, frame)?;
            remap_expression(&mut access.key, frame)?;
        }
        HirExpr::Unary(unary) => remap_expression(&mut unary.expr, frame)?,
        HirExpr::Binary(binary) => {
            remap_expression(&mut binary.lhs, frame)?;
            remap_expression(&mut binary.rhs, frame)?;
        }
        HirExpr::Call(call) => {
            remap_expression(&mut call.callee, frame)?;
            for argument in &mut call.args.fixed {
                remap_expression(argument, frame)?;
            }
            if let Some(tail) = call.args.tail.take() {
                if tail.exact_width().is_some() {
                    return None;
                }
                let mut expression = tail.into_expr();
                remap_expression(&mut expression, frame)?;
                call.args.tail = Some(crate::hir::common::HirPackTail::open(expression));
            }
        }
        HirExpr::TableConstructor(table) => {
            for field in &mut table.fields {
                match field {
                    HirTableField::Array(value) => remap_expression(value, frame)?,
                    HirTableField::Record(field) => {
                        if let crate::hir::common::HirTableKey::Expr(key) = &mut field.key {
                            remap_expression(key, frame)?;
                        }
                        remap_expression(&mut field.value, frame)?;
                    }
                }
            }
            if let Some(tail) = table.trailing_multivalue.take() {
                if tail.exact_width().is_some() {
                    return None;
                }
                let mut expression = tail.into_expr();
                remap_expression(&mut expression, frame)?;
                table.trailing_multivalue = Some(crate::hir::common::HirPackTail::open(expression));
            }
        }
        HirExpr::Closure(closure) => {
            for capture in &mut closure.captures {
                remap_expression(&mut capture.value, frame)?;
            }
        }
        HirExpr::Nil
        | HirExpr::Boolean(_)
        | HirExpr::Integer(_)
        | HirExpr::String(_)
        | HirExpr::GlobalRef(_)
        | HirExpr::UpvalueRef(_) => {}
        HirExpr::Number(value) if value.is_finite() => {}
        _ => return None,
    }
    Some(())
}
