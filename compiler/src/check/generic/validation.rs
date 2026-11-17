//! 汎用数値型の名前を記述できる場所を検査する。通常の名前解決や型変換の判定は関数・式の検査に任せる。
//!
//! 利用元: [`crate::check::functions`]。
//! 主な対象規則: `[num.fn-undeclared]`、`[num.places]`。

use crate::{
    Diagnostic, ErrorKind,
    ast::{
        Expression, ExpressionKind, Location, Program, Statement, Type,
        visit::{self, Visitor},
    },
};

// ここでは汎用型名を記述できる場所だけを検査する。
// 名前・スコープ・制御・変換の意味は通常の検査が担当する。
pub(crate) fn validate(program: &Program) -> Result<(), Diagnostic> {
    for declaration in &program.constants {
        if declaration.annotation.is_some_and(Type::generic) {
            return Err(place_error(
                declaration.location,
                "constant annotations cannot use int or float; use a concrete type or omit the annotation",
            ));
        }
        Places {
            int: false,
            float: false,
            constant: true,
        }
        .expression(&declaration.initializer)?;
    }
    for declaration in &program.structs {
        for field in &declaration.fields {
            if field.ty.generic() {
                return Err(place_error(
                    field.location,
                    "struct fields cannot use int or float; use a concrete type",
                ));
            }
        }
    }
    for function in &program.functions {
        let mut places = Places {
            int: function.declares_numeric(Type::GenericInt),
            float: function.declares_numeric(Type::GenericFloat),
            constant: false,
        };
        places.scope(&function.statements, None)?;
    }
    Ok(())
}
fn place_error(location: Location, message: &str) -> Diagnostic {
    location.error(
        ErrorKind::InvalidSource,
        crate::diagnostic!("num.places", "{message}"),
    )
}
struct Places {
    constant: bool,
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
                crate::diagnostic!(
                    "num.fn-undeclared",
                    "{} is not declared in this function's signature",
                    ty.name()
                ),
            ))
        } else {
            Err(place_error(
                location,
                if self.constant {
                    "constant cast targets cannot use int or float; use a concrete type"
                } else if annotation {
                    "local annotations cannot use int or float without declaring them in the function signature; use a concrete type or omit the annotation"
                } else {
                    "cast targets cannot use int or float without declaring them in the function signature; use a concrete type"
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
