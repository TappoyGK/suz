use crate::{
    Diagnostic, ErrorKind,
    ast::{
        self, Expression, ExpressionKind, Location, Program, Statement, Type,
        visit::{self, Visitor},
    },
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, PartialEq, Eq)]
enum NameKind {
    Other,
    Constant,
    Parameter,
    LoopVariable,
    Fix,
    Var,
}
impl NameKind {
    fn assignment_error(self) -> Option<&'static str> {
        match self {
            Self::Other => Some("assignment target is not in scope [decl.local-scope]"),
            Self::Constant => Some("cannot assign to a constant [const.decl]"),
            Self::Parameter => Some("cannot assign to a parameter [fn.parameter]"),
            Self::LoopVariable => Some("cannot assign to a loop variable [for.variable]"),
            Self::Fix => Some("cannot assign to a fix binding [var.fix-assign]"),
            Self::Var => None,
        }
    }
}

pub(crate) fn validate(program: &Program) -> Result<(), Diagnostic> {
    let mut globals = BTreeMap::new();
    for name in program
        .constants
        .iter()
        .map(|c| c.name.as_str())
        .chain(program.functions.iter().map(|f| f.name.as_str()))
        .chain(program.structs.iter().map(|s| s.name.as_str()))
        .chain(program.enums.iter().map(|e| e.name.as_str()))
    {
        globals.insert(name.to_owned(), NameKind::Other);
    }
    for constant in &program.constants {
        globals.insert(constant.name.clone(), NameKind::Constant);
    }
    for declaration in &program.constants {
        if declaration.annotation.is_some_and(Type::generic) {
            return Err(place_error(declaration.location));
        }
        let mut visitor = Validate {
            assigned: BTreeSet::new(),
            active: [false; 2],
            names: globals.clone(),
            check_names: false,
            loops: 0,
            returned: false,
            result: Type::Void,
            program,
        };
        visitor.expression(&declaration.initializer)?;
    }
    for declaration in &program.structs {
        for field in &declaration.fields {
            if field.ty.generic() {
                return Err(place_error(field.location));
            }
        }
    }
    for function in &program.functions {
        let mut active = [false; 2];
        for ty in function
            .parameters
            .iter()
            .map(|p| p.ty)
            .chain([function.result])
        {
            if let Some(i) = super::index(ty) {
                active[i] = true;
            }
        }
        let generic = active.iter().any(|v| *v);
        let mut visitor = Validate {
            assigned: BTreeSet::new(),
            active,
            names: globals.clone(),
            check_names: generic,
            loops: 0,
            returned: false,
            result: function.result,
            program,
        };
        for parameter in &function.parameters {
            if generic
                && visitor
                    .names
                    .insert(parameter.name.clone(), NameKind::Parameter)
                    .is_some()
            {
                return Err(parameter.location.error(
                    ErrorKind::InvalidSource,
                    "parameter duplicates or shadows a name [fn.parameter-scope]",
                ));
            }
        }
        super::super::control::validate_return_positions(&function.statements)?;
        visitor.scope(&function.statements, None)?;
        if generic && function.result != Type::Void {
            if !visitor.returned {
                return Err(function.end.error(ErrorKind::InvalidSource, "a value-returning function requires a return expression [fn.value-return-required]"));
            }
            if !stops(&function.statements) {
                return Err(function.end.error(
                    ErrorKind::InvalidSource,
                    "a value-returning function may reach its end [fn.fallthrough]",
                ));
            }
        }
    }
    Ok(())
}
fn place_error(location: Location) -> Diagnostic {
    location.error(ErrorKind::InvalidSource, "generic numeric types require a generic function; use a concrete type or omit the annotation [num.places]")
}
struct Validate<'a> {
    assigned: BTreeSet<String>,
    active: [bool; 2],
    names: BTreeMap<String, NameKind>,
    check_names: bool,
    loops: usize,
    returned: bool,
    result: Type,
    program: &'a Program,
}
impl Validate<'_> {
    fn ty(&self, ty: Type, location: Location) -> Result<(), Diagnostic> {
        if let Some(i) = super::index(ty)
            && !self.active[i]
        {
            return Err(if self.active.iter().any(|v| *v) {
                location.error(
                    ErrorKind::InvalidSource,
                    format!(
                        "{} is not declared in this function's signature [num.fn-undeclared]",
                        ty.name()
                    ),
                )
            } else {
                place_error(location)
            });
        }
        Ok(())
    }
    fn insert(&mut self, name: &str, kind: NameKind, location: Location) -> Result<(), Diagnostic> {
        if self.check_names && self.names.contains_key(name) {
            return Err(location.error(
                ErrorKind::InvalidSource,
                "binding duplicates or shadows a name [decl.shadowing]",
            ));
        }
        self.assigned.remove(name);
        self.names.insert(name.to_owned(), kind);
        Ok(())
    }
}
impl Visitor for Validate<'_> {
    fn expression(&mut self, expr: &Expression) -> Result<(), Diagnostic> {
        match &expr.kind {
            ExpressionKind::Cast { target, .. } => self.ty(*target, expr.location)?,
            ExpressionKind::Name(name) if self.check_names => {
                let binding = self
                    .names
                    .get(name)
                    .is_some_and(|kind| *kind != NameKind::Other);
                if !binding {
                    return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        "binding is not in scope [decl.local-scope]",
                    ));
                }
            }
            ExpressionKind::Call { name, arguments } if self.check_names => {
                let function = self
                    .program
                    .functions
                    .iter()
                    .find(|f| &f.name == name)
                    .ok_or_else(|| {
                        expr.location
                            .error(ErrorKind::InvalidSource, "unknown function [fn.call]")
                    })?;
                if arguments.len() != function.parameters.len() {
                    return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        "argument count does not match declaration [fn.call]",
                    ));
                }
            }
            ExpressionKind::Struct { id, fields } if self.check_names => {
                super::super::expression::structs::initializer_indices(
                    &self.program.structs[*id],
                    fields,
                    expr.location,
                )?;
            }
            ExpressionKind::Variant { enum_name, variant }
                if self.check_names
                    && !self
                        .program
                        .enums
                        .iter()
                        .any(|e| &e.name == enum_name && e.variants.contains(variant)) =>
            {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    "unknown enum or variant [enum.access]",
                ));
            }
            _ => {}
        }
        if let ExpressionKind::Cast { fallback, .. }
        | ExpressionKind::ArithmeticHandler { fallback, .. }
        | ExpressionKind::Slice { fallback, .. } = &expr.kind
            && matches!(fallback, Some(ast::FailureHandler::Return { .. }))
        {
            self.returned = true;
        }
        visit::walk_expression(self, expr)
    }
    fn scope(
        &mut self,
        statements: &[Statement],
        value: Option<&Expression>,
    ) -> Result<(), Diagnostic> {
        let old = self.names.clone();
        for statement in statements {
            self.statement(statement)?;
        }
        if let Some(value) = value {
            self.expression(value)?;
        }
        if self.check_names {
            for statement in statements {
                if let Statement::Binding {
                    name,
                    mutable: true,
                    location,
                    ..
                } = statement
                    && !self.assigned.contains(name)
                {
                    return Err(location.error(
                        ErrorKind::InvalidSource,
                        format!("var binding '{name}' is never reassigned [var.never-reassigned]"),
                    ));
                }
            }
        }
        self.names = old;
        Ok(())
    }
    fn statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        match statement {
            Statement::Binding {
                name,
                annotation,
                initializer,
                location,
                mutable,
            } => {
                if let Some(ty) = annotation {
                    self.ty(*ty, *location)?;
                }
                self.expression(initializer)?;
                self.insert(
                    name,
                    if *mutable {
                        NameKind::Var
                    } else {
                        NameKind::Fix
                    },
                    *location,
                )
            }
            Statement::Assign { name, location, .. } if self.check_names => {
                if let Some(message) = self
                    .names
                    .get(name)
                    .copied()
                    .unwrap_or(NameKind::Other)
                    .assignment_error()
                {
                    return Err(location.error(ErrorKind::InvalidSource, message));
                }
                self.assigned.insert(name.clone());
                visit::walk_statement(self, statement)
            }
            Statement::Return(value) => {
                self.returned = true;
                if self.check_names
                    && (self.result == Type::Void) != matches!(value.kind, ExpressionKind::Void)
                {
                    return Err(value.location.error(
                        ErrorKind::InvalidSource,
                        "return value does not match the function's return form [fn.return]",
                    ));
                }
                self.expression(value)
            }
            Statement::Break(location) | Statement::Continue(location)
                if self.check_names && self.loops == 0 =>
            {
                Err(location.error(
                    ErrorKind::InvalidSource,
                    "break and continue require a loop [loop.jump-context]",
                ))
            }
            Statement::For {
                name,
                start,
                end,
                body,
                location,
                ..
            } => {
                self.expression(start)?;
                if let Some(end) = end {
                    self.expression(end)?;
                }
                let old = self.names.clone();
                self.insert(name, NameKind::LoopVariable, *location)?;
                self.loops += 1;
                self.scope(body, None)?;
                self.loops -= 1;
                self.names = old;
                Ok(())
            }
            Statement::Loop { condition, body } => {
                if let Some(condition) = condition {
                    self.expression(condition)?;
                }
                self.loops += 1;
                self.scope(body, None)?;
                self.loops -= 1;
                Ok(())
            }
            _ => visit::walk_statement(self, statement),
        }
    }
}
fn breaks(statements: &[Statement]) -> bool {
    statements.iter().any(|s| match s {
        Statement::Break(_) => true,
        Statement::Block(s) => breaks(s),
        Statement::If {
            branches,
            otherwise,
        } => {
            branches.iter().any(|(_, s)| breaks(s)) || otherwise.as_ref().is_some_and(|s| breaks(s))
        }
        Statement::Match { arms, .. } => arms.iter().any(|a| breaks(&a.body)),
        _ => false,
    })
}
fn stops(statements: &[Statement]) -> bool {
    statements.iter().any(|s| match s {
        Statement::Return(_) | Statement::Break(_) | Statement::Continue(_) => true,
        Statement::Block(s) => stops(s),
        Statement::If {
            branches,
            otherwise,
        } => branches.iter().all(|(_, s)| stops(s)) && otherwise.as_ref().is_some_and(|s| stops(s)),
        Statement::Match { arms, .. } => !arms.is_empty() && arms.iter().all(|a| stops(&a.body)),
        Statement::Loop { condition, body } => {
            condition
                .as_ref()
                .is_none_or(|c| matches!(c.kind, ExpressionKind::Bool(true)))
                && !breaks(body)
        }
        _ => false,
    })
}
