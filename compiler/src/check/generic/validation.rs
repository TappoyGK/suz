use crate::{
    Diagnostic, ErrorKind,
    ast::{
        Expression, ExpressionKind, Location, Program, Statement, Type,
        visit::{self, Visitor},
    },
};

// This pass only validates where generic type names may be written. The
// ordinary checker owns names, scope, control flow, and conversion semantics.
pub(crate) fn validate(program: &Program) -> Result<(), Diagnostic> {
    for declaration in &program.constants {
        if declaration.annotation.is_some_and(Type::generic) {
            return Err(place_error(
                declaration.location,
                "constant annotations require a concrete type; use a concrete type or omit the annotation",
            ));
        }
        Places {
            int: false,
            float: false,
        }
        .expression(&declaration.initializer)?;
    }
    for declaration in &program.structs {
        for field in &declaration.fields {
            if field.ty.generic() {
                return Err(place_error(
                    field.location,
                    "struct fields require a concrete type",
                ));
            }
        }
    }
    for function in &program.functions {
        let mut places = Places {
            int: false,
            float: false,
        };
        for ty in function
            .parameters
            .iter()
            .map(|p| p.ty)
            .chain([function.result])
        {
            places.int |= ty == Type::GenericInt;
            places.float |= ty == Type::GenericFloat;
        }
        places.scope(&function.statements, None)?;
    }
    Ok(())
}
fn place_error(location: Location, message: &str) -> Diagnostic {
    location.error(ErrorKind::InvalidSource, format!("{message} [num.places]"))
}
struct Places {
    int: bool,
    float: bool,
}
impl Places {
    fn ty(&self, ty: Type, location: Location, annotation: bool) -> Result<(), Diagnostic> {
        let declared = match ty {
            Type::GenericInt => self.int,
            Type::GenericFloat => self.float,
            _ => return Ok(()),
        };
        if declared {
            return Ok(());
        }
        if self.int || self.float {
            Err(location.error(
                ErrorKind::InvalidSource,
                format!(
                    "{} is not declared in this function's signature [num.fn-undeclared]",
                    ty.name()
                ),
            ))
        } else {
            Err(place_error(
                location,
                if annotation {
                    "a non-generic function requires a concrete type; use a concrete type or omit the annotation"
                } else {
                    "a non-generic function requires a concrete cast target; use a concrete type"
                },
            ))
        }
    }
}
impl Visitor for Places {
    fn expression(&mut self, expr: &Expression) -> Result<(), Diagnostic> {
        if let ExpressionKind::Cast { target, .. } = &expr.kind {
            self.ty(*target, expr.location, false)?;
        }
        visit::walk_expression(self, expr)
    }
    fn statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        if let Statement::Binding {
            annotation: Some(ty),
            location,
            ..
        } = statement
        {
            self.ty(*ty, *location, true)?;
        }
        visit::walk_statement(self, statement)
    }
}
