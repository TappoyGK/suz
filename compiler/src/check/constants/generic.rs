//! 検査済みの定数値に数値型の代入を適用し、通常の定数評価で扱える形へ具体化する。ソースの式や宣言を再検査しない。
//!
//! 利用元: [`crate::check::constants`]。

use super::*;
use crate::check::{generic::domain, scope::Scope};

pub(in crate::check) fn at(
    value: &CheckedExpression,
    index: usize,
    scope: &Scope,
) -> Result<Option<CheckedExpression>, Diagnostic> {
    if !is_constant(value) {
        return Ok(None);
    }
    let mut value = value.clone();
    if substitute(&mut value, index, scope).is_none() {
        return Ok(None);
    }
    evaluate(&value)
}

// 正確な値域とテンプレートの描画結果は派生情報なので、共通の子の走査に合わせて更新する。
// Valueの子の一覧はIRだけに定義する。
fn substitute(value: &mut CheckedExpression, index: usize, scope: &Scope) -> Option<()> {
    value.ty = domain::at(value.ty, index)?;
    let complete = std::cell::Cell::new(true);
    let admitted = value.value.visit_constant_parts_mut(
        &mut |child| {
            if complete.get() {
                complete.set(substitute(child, index, scope).is_some());
            }
        },
        &mut |exact| {
            if complete.get() {
                if let Ok(range) = exact.range_at(index) {
                    exact.range = range;
                    exact.domains = None;
                } else {
                    complete.set(false);
                }
            }
        },
    );
    if !admitted || !complete.get() {
        return None;
    }
    match &mut value.value {
        Value::GenericNumericLiteral {
            text,
            negative,
            floating,
            location,
        } => {
            value.value = crate::check::expression::scalar_literal(
                text, *negative, *floating, value.ty, *location,
            )
            .ok()?
            .value;
        }
        Value::Template {
            parts, constant, ..
        } => {
            *constant = crate::check::text::constant_parts(parts, scope);
        }
        _ => {}
    }
    Some(())
}
