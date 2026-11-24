//! トップレベル宣言の初期値を定数式として検査する。fixの依存を解決し、評価済みの定数を作る。
//!
//! 利用元: [`crate::check::functions`]。
//! 主な対象規則: `[const.expression]`、`[const.initializer]`、`[decl.duplicate]`。

use super::{
    constants, diagnostics, expression::coerced as expression, ir::CheckedExpression,
    statements::Binding,
};
use crate::check::scope::Scope;
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, FailureHandler, Location, TopLevelBinding},
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn dependencies(
    expr: &Expression,
    names: &mut Vec<(String, Location)>,
) -> Result<(), Diagnostic> {
    match &expr.kind {
        ExpressionKind::ArrayInit {
            values, lengths, ..
        } => {
            for value in values.iter().chain(lengths) {
                dependencies(value, names)?;
            }
        }
        ExpressionKind::TextBuffer { .. } => {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("const.expression", "textbuf is not a constant expression"),
            ));
        }
        ExpressionKind::New(inner) => {
            let message = if matches!(inner.kind, ExpressionKind::ArrayInit { .. }) {
                crate::diagnostic!(
                    "const.expression",
                    "array allocation is not a constant expression"
                )
            } else {
                crate::diagnostic!("const.expression", "new is not a constant expression")
            };
            return Err(expr.location.error(ErrorKind::InvalidSource, message));
        }
        ExpressionKind::MethodCall { .. } | ExpressionKind::Receiver { .. } => {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "const.expression",
                    "method calls are not constant expressions"
                ),
            ));
        }
        ExpressionKind::Panic(_) => {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("const.expression", "panic is not constant"),
            ));
        }
        ExpressionKind::Closure(_) => {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("fn.value-const", "closures are not constant expressions"),
            ));
        }
        ExpressionKind::Slice {
            target,
            start,
            end,
            fallback,
            ..
        } => {
            dependencies(target, names)?;
            for bound in [start, end].into_iter().flatten() {
                dependencies(bound, names)?;
            }
            handler_dependencies(fallback, names)?;
        }
        ExpressionKind::Template(parts) => {
            for part in parts {
                if let crate::ast::TemplatePart::Value(value) = part {
                    dependencies(value, names)?;
                }
            }
        }
        ExpressionKind::With { target, fields } => {
            dependencies(target, names)?;
            for field in fields {
                dependencies(&field.value, names)?;
            }
        }
        ExpressionKind::Struct { fields, .. } => {
            for field in fields {
                dependencies(&field.value, names)?;
            }
        }
        ExpressionKind::Field { target, .. } => dependencies(target, names)?,
        ExpressionKind::Name(name) => names.push((name.clone(), expr.location)),
        ExpressionKind::Null
        | ExpressionKind::FunctionValue { .. }
        | ExpressionKind::Integer { .. }
        | ExpressionKind::Float(_)
        | ExpressionKind::Char(_)
        | ExpressionKind::Bool(_)
        | ExpressionKind::Variant { .. } => {}
        ExpressionKind::Group(inner)
        | ExpressionKind::Negate(inner)
        | ExpressionKind::Deref(inner)
        | ExpressionKind::CBytes(inner)
        | ExpressionKind::CByteLen(inner)
        | ExpressionKind::Len(inner)
        | ExpressionKind::Not(inner)
        | ExpressionKind::BitNot(inner) => dependencies(inner, names)?,
        ExpressionKind::TextAppend { left, right }
        | ExpressionKind::Binary { left, right, .. }
        | ExpressionKind::Logical { left, right, .. } => {
            dependencies(left, names)?;
            dependencies(right, names)?;
        }
        ExpressionKind::Comparison { operands, .. } => {
            for operand in operands {
                dependencies(operand, names)?;
            }
        }
        ExpressionKind::ArithmeticHandler { operand, fallback }
        | ExpressionKind::Cast {
            operand, fallback, ..
        } => {
            dependencies(operand, names)?;
            handler_dependencies(fallback, names)?;
        }
        ExpressionKind::Void
        | ExpressionKind::Call { .. }
        | ExpressionKind::Invoke { .. }
        | ExpressionKind::Block { .. }
        | ExpressionKind::If { .. }
        | ExpressionKind::Match { .. } => {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "const.expression",
                    "value blocks are not constant expressions"
                ),
            ));
        }
    }
    Ok(())
}

fn handler_dependencies(
    fallback: &Option<FailureHandler>,
    names: &mut Vec<(String, Location)>,
) -> Result<(), Diagnostic> {
    match fallback {
        Some(FailureHandler::Errors(handler)) => Err(handler.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "const.expression",
                "error handlers are not constant expressions"
            ),
        )),
        Some(FailureHandler::Value(value)) => dependencies(value, names),
        Some(FailureHandler::Return { location, .. } | FailureHandler::Fail { location, .. }) => {
            Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "const.expression",
                    "control transfers are not constant expressions"
                ),
            ))
        }
        Some(FailureHandler::Ignore(ignore)) => Err(ignore.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "const.expression",
                "control transfers are not constant expressions"
            ),
        )),
        None => Ok(()),
    }
}

struct Resolver<'a> {
    declarations: BTreeMap<String, &'a TopLevelBinding>,
    bindings: Scope,
    visiting: BTreeSet<String>,
    warnings: Vec<Location>,
}

impl Resolver<'_> {
    fn resolve(&mut self, name: &str, location: Location) -> Result<(), Diagnostic> {
        if self.bindings.contains_key(name) {
            return Ok(());
        }
        let declaration = *self.declarations.get(name).ok_or_else(|| {
            location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("const.expression", "'{name}' is not a constant"),
            )
        })?;
        if !self.visiting.insert(name.to_owned()) {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("const.initializer", "cyclic constant dependency"),
            ));
        }
        let value = self.initializer(declaration)?;
        self.bindings.locals.insert(
            name.to_owned(),
            Binding {
                loop_variable: false,
                // 定数はローカル変数の格納先ではなく、評価済みの値として置き換える。
                id: 0,
                ty: value.ty,
                mutable: false,
                parameter: false,
                assigned: Default::default(),
                location: declaration.location,
                constant: Some(value),
            },
        );
        self.visiting.remove(name);
        Ok(())
    }
    fn initializer(
        &mut self,
        declaration: &TopLevelBinding,
    ) -> Result<CheckedExpression, Diagnostic> {
        // 選択されない分岐も含めて、すべての依存を解決する。
        // 定数式の文法・型・循環の判定は短絡評価に左右されない。
        let mut names = Vec::new();
        dependencies(&declaration.initializer, &mut names)?;
        for (dependency, location) in names {
            self.resolve(&dependency, location)?;
        }
        let mut scope = self.bindings.clone();
        scope.constant_initializer = true;
        let annotation = declaration
            .annotation
            .map(|ty| scope.resolve_type_at(ty, declaration.location))
            .transpose()?;
        let checked = expression(&declaration.initializer, annotation, &scope)?;
        if super::ffi::uses_c_environment(&checked) {
            return Err(declaration.initializer.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "const.expression",
                    "C-dependent values are not constant expressions"
                ),
            ));
        }
        diagnostics::collect(&checked, &mut self.warnings);
        let value = constants::evaluate(&checked)?.ok_or_else(|| {
            declaration.location.error(
                ErrorKind::Unsupported,
                crate::diagnostic!(
                    "const.initializer",
                    "this constant expression is not implemented"
                ),
            )
        })?;
        super::text::register_constants(&value, &self.bindings);
        Ok(value)
    }
}

pub(super) fn resolve(
    declarations: &[TopLevelBinding],
    enums: &[crate::ast::EnumDeclaration],
    scope: Scope,
) -> Result<(Scope, Vec<Location>), Diagnostic> {
    let mut resolver = Resolver {
        declarations: BTreeMap::new(),
        bindings: Scope {
            enums: std::rc::Rc::new(enums.to_vec()),
            ..scope
        },
        visiting: BTreeSet::new(),
        warnings: Vec::new(),
    };
    for declaration in declarations {
        if resolver
            .bindings
            .structs
            .shapes
            .contains_name(&declaration.name)
            || resolver.bindings.structs.contains_name(&declaration.name)
            || enums.iter().any(|e| e.name == declaration.name)
            || declaration.name == "main"
            || resolver
                .declarations
                .insert(declaration.name.clone(), declaration)
                .is_some()
        {
            return Err(declaration.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "decl.duplicate",
                    "duplicate declaration '{}'",
                    declaration.name
                ),
            ));
        }
    }
    for declaration in declarations {
        resolver.resolve(&declaration.name, declaration.location)?;
    }
    // 定数は不変であり、すべての関数スコープと型の代入で共有する。
    resolver.bindings.constants = std::rc::Rc::new(std::mem::take(&mut resolver.bindings.locals));
    Ok((resolver.bindings, resolver.warnings))
}
