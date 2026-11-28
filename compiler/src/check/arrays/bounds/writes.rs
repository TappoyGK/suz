//! 代入・関数呼び出し・`#` メソッド呼び出し・ループ本体による条件の忘却を扱う。
//!
//! 利用元: [`super`]、[`crate::check::statements`]、[`crate::check::methods`]、
//! [`crate::check::functions`]、[`crate::check::function_values`]。

use super::super::{super::scope::Scope, facts::Facts};
use crate::{
    Diagnostic,
    ast::{
        Expression, ExpressionKind, Statement, Type,
        visit::{self, Visitor},
    },
};

/// 関数呼び出しにより書き換わりうる、参照キャプチャー済みの var と共有参照先の条件を忘れる。
pub(in crate::check) fn forget_call(scope: &Scope) {
    let mut facts = scope.array_bounds.facts.borrow_mut();
    facts.forget_shared();
    let captured = scope.ref_captured.borrow();
    if captured.is_empty() {
        return;
    }
    for &id in captured.iter() {
        facts.forget_local(id);
    }
}

/// `#` メソッドの受け手が `var` 変数またはそのフィールド経路であれば、その条件と共有参照先の条件を忘れる。
pub(in crate::check) fn forget_mutable_receiver(receiver: &Expression, scope: &Scope) {
    let mut facts = scope.array_bounds.facts.borrow_mut();
    facts.forget_shared();
    if let Some((id, fields)) = mutable_ast_path(receiver, scope) {
        facts.forget_path(id, &fields);
    }
}

fn mutable_ast_path(expr: &Expression, scope: &Scope) -> Option<(usize, Vec<usize>)> {
    let mut names = Vec::new();
    let mut cur = expr;
    loop {
        match &cur.kind {
            ExpressionKind::Group(inner) => cur = inner,
            ExpressionKind::Field { target, name } => {
                names.push(name.as_str());
                cur = target;
            }
            ExpressionKind::Name(name) => {
                let binding = scope.locals.get(name)?;
                if !binding.mutable {
                    return None;
                }
                names.reverse();
                let mut ty = binding.ty;
                let mut indices = Vec::new();
                for field_name in names {
                    let Type::Struct(sid) = ty else {
                        return None;
                    };
                    let s = scope.structs.get(sid);
                    let idx = s.fields.iter().position(|f| f.name == field_name)?;
                    ty = s.fields[idx].ty;
                    indices.push(idx);
                }
                return Some((binding.id, indices));
            }
            _ => return None,
        }
    }
}

/// ループ本体（および `while` 条件）で書き換わりうる項を事前走査してループ先頭で忘れる。
pub(in crate::check) fn forget_loop_writes(
    statements: &[Statement],
    extra_expr: Option<&Expression>,
    scope: &Scope,
) {
    let mut facts = scope.array_bounds.facts.borrow_mut();
    if facts.is_empty() {
        return;
    }
    let mut walker = WriteForgetter {
        scope,
        facts: &mut facts,
    };
    if let Some(expr) = extra_expr {
        let _ = walker.expression(expr);
    }
    let _ = walker.scope(statements, None);
}

pub(super) fn forget_expr_writes_into(expr: &Expression, scope: &Scope, facts: &mut Facts) {
    let mut walker = WriteForgetter { scope, facts };
    let _ = walker.expression(expr);
}

struct WriteForgetter<'a> {
    scope: &'a Scope,
    facts: &'a mut Facts,
}

impl WriteForgetter<'_> {
    fn forget_ref_captured(&mut self) {
        for &id in self.scope.ref_captured.borrow().iter() {
            self.facts.forget_local(id);
        }
    }
}

impl Visitor for WriteForgetter<'_> {
    fn statement(&mut self, stmt: &Statement) -> Result<(), Diagnostic> {
        if matches!(stmt, Statement::Store { .. } | Statement::ArrayBulkWrite(_)) {
            self.facts.forget_shared();
        }
        if let Statement::Assign { name, fields, .. } = stmt
            && let Some(binding) = self.scope.locals.get(name)
            && binding.mutable
        {
            let mut ty = binding.ty;
            let mut indices = Vec::new();
            let mut ok = true;
            for field_name in fields {
                if let Type::Struct(sid) = ty
                    && let Some(idx) = self
                        .scope
                        .structs
                        .get(sid)
                        .fields
                        .iter()
                        .position(|f| f.name == *field_name)
                {
                    ty = self.scope.structs.get(sid).fields[idx].ty;
                    indices.push(idx);
                } else {
                    ok = false;
                    break;
                }
            }
            if ok {
                self.facts.forget_path(binding.id, &indices);
            } else {
                self.facts.forget_local(binding.id);
            }
        }
        visit::walk_statement(self, stmt)
    }

    fn expression(&mut self, expr: &Expression) -> Result<(), Diagnostic> {
        match &expr.kind {
            ExpressionKind::Call { .. } | ExpressionKind::Invoke { .. } => {
                visit::walk_expression(self, expr)?;
                self.facts.forget_shared();
                self.forget_ref_captured();
                Ok(())
            }
            ExpressionKind::MethodCall {
                target, mutable, ..
            } => {
                visit::walk_expression(self, expr)?;
                self.facts.forget_shared();
                if *mutable && let Some((id, fields)) = mutable_ast_path(target, self.scope) {
                    self.facts.forget_path(id, &fields);
                }
                self.forget_ref_captured();
                Ok(())
            }
            ExpressionKind::Closure(closure) => {
                for cap in &closure.captures {
                    if cap.by_reference
                        && let Some(binding) = self.scope.locals.get(&cap.name)
                    {
                        self.facts.forget_local(binding.id);
                    }
                }
                Ok(())
            }
            _ => visit::walk_expression(self, expr),
        }
    }
}
