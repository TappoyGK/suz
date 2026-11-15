use super::{constants, diagnostics, expression::coerced as expression, statements::Binding};
use crate::check::scope::Scope;
use crate::{
    Diagnostic, ErrorKind,
    ast::{ConstDeclaration, Expression, ExpressionKind, FailureHandler, Location},
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn dependencies(
    expr: &Expression,
    names: &mut Vec<(String, Location)>,
) -> Result<(), Diagnostic> {
    match &expr.kind {
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
        ExpressionKind::Struct { fields, .. } => {
            for field in fields {
                dependencies(&field.value, names)?;
            }
        }
        ExpressionKind::Field { target, .. } => dependencies(target, names)?,
        ExpressionKind::Name(name) => names.push((name.clone(), expr.location)),
        ExpressionKind::Integer { .. }
        | ExpressionKind::Float(_)
        | ExpressionKind::Char(_)
        | ExpressionKind::Bool(_)
        | ExpressionKind::Variant { .. } => {}
        ExpressionKind::Group(inner)
        | ExpressionKind::Negate(inner)
        | ExpressionKind::Not(inner)
        | ExpressionKind::BitNot(inner) => dependencies(inner, names)?,
        ExpressionKind::Binary { left, right, .. }
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
        | ExpressionKind::Block { .. }
        | ExpressionKind::If { .. }
        | ExpressionKind::Match { .. } => {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                "value blocks are not constant expressions [const.expression]",
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
        Some(FailureHandler::Value(value)) => dependencies(value, names),
        Some(FailureHandler::Return { location, .. }) => Err(location.error(
            ErrorKind::InvalidSource,
            "control transfers are not constant expressions [const.expression]",
        )),
        None => Ok(()),
    }
}

struct Resolver<'a> {
    declarations: BTreeMap<String, &'a ConstDeclaration>,
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
                format!("'{name}' is not a constant [const.expression]"),
            )
        })?;
        if !self.visiting.insert(name.to_owned()) {
            return Err(location.error(
                ErrorKind::InvalidSource,
                "cyclic constant dependency [const.initializer]",
            ));
        }
        // Resolve every dependency, even in an unselected branch. Constant
        // expression grammar, types and cycles are independent of short-circuiting.
        let mut names = Vec::new();
        dependencies(&declaration.initializer, &mut names)?;
        for (dependency, location) in names {
            self.resolve(&dependency, location)?;
        }
        let checked = expression(
            &declaration.initializer,
            declaration.annotation,
            &self.bindings,
        )?;
        diagnostics::collect(&checked, &mut self.warnings);
        let value = constants::evaluate(&checked)?.ok_or_else(|| {
            declaration.location.error(
                ErrorKind::Unsupported,
                "this constant expression is not implemented",
            )
        })?;
        super::text::register_constants(&value, &self.bindings);
        self.bindings.insert(
            name.to_owned(),
            Binding {
                generic: None,
                generic_source: None,
                loop_variable: false,
                // Constants are substituted as evaluated values, not local slots.
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
}

pub(super) fn resolve(
    declarations: &[ConstDeclaration],
    enums: &[crate::ast::EnumDeclaration],
    structs: &[crate::ast::StructDeclaration],
) -> Result<(Scope, Vec<Location>), Diagnostic> {
    let mut resolver = Resolver {
        declarations: BTreeMap::new(),
        bindings: Scope {
            enums: std::rc::Rc::new(enums.to_vec()),
            structs: std::rc::Rc::new(structs.to_vec()),
            ..Scope::default()
        },
        visiting: BTreeSet::new(),
        warnings: Vec::new(),
    };
    for declaration in declarations {
        if structs.iter().any(|s| s.name == declaration.name)
            || enums.iter().any(|e| e.name == declaration.name)
            || declaration.name == "main"
            || resolver
                .declarations
                .insert(declaration.name.clone(), declaration)
                .is_some()
        {
            return Err(declaration.location.error(
                ErrorKind::InvalidSource,
                format!(
                    "duplicate declaration '{}' [decl.duplicate]",
                    declaration.name
                ),
            ));
        }
    }
    for declaration in declarations {
        resolver.resolve(&declaration.name, declaration.location)?;
    }
    Ok((resolver.bindings, resolver.warnings))
}
