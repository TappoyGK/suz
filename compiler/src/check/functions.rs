use super::{
    control, declarations, diagnostics,
    expression::expression,
    ir::{CheckedExpression, CheckedFunction, CheckedProgram, Value},
    scope::{Scope, Signature},
    statements::{self, Binding},
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, Program, Type},
};
use std::{collections::BTreeMap, rc::Rc};

pub(super) fn check(program: &Program) -> Result<CheckedProgram, Diagnostic> {
    let mut signatures = BTreeMap::new();
    for (id, function) in program.functions.iter().enumerate() {
        if signatures.contains_key(&function.name)
            || program.constants.iter().any(|c| c.name == function.name)
            || program.enums.iter().any(|e| e.name == function.name)
            || program.structs.iter().any(|s| s.name == function.name)
        {
            return Err(function.location.error(
                ErrorKind::InvalidSource,
                "duplicate top-level name [decl.duplicate]",
            ));
        }
        signatures.insert(
            function.name.clone(),
            Signature {
                id,
                parameters: function.parameters.iter().map(|p| p.ty).collect(),
                result: function.result,
            },
        );
    }
    let main_id = signatures["main"].id;
    let signatures = Rc::new(signatures);
    let (mut globals, mut warnings) =
        declarations::resolve(&program.constants, &program.enums, &program.structs)?;
    globals.functions = signatures;
    let mut functions = Vec::new();
    for function in &program.functions {
        control::validate_return_positions(&function.statements)?;
        let mut scope = globals.clone();
        scope.result = function.result;
        let mut declared = Vec::new();
        for (id, parameter) in function.parameters.iter().enumerate() {
            if declared.contains(&parameter.name) {
                return Err(parameter.location.error(
                    ErrorKind::InvalidSource,
                    "duplicate parameter name [fn.parameter-scope]",
                ));
            }
            if scope.contains_key(&parameter.name)
                || scope.functions.contains_key(&parameter.name)
                || scope.is_type_name(&parameter.name)
            {
                return Err(parameter.location.error(
                    ErrorKind::InvalidSource,
                    "parameter shadows a top-level name [decl.shadowing]",
                ));
            }
            declared.push(parameter.name.clone());
            scope.insert(
                parameter.name.clone(),
                Binding {
                    id,
                    ty: parameter.ty,
                    mutable: false,
                    parameter: true,
                    loop_variable: false,
                    assigned: Default::default(),
                    location: parameter.location,
                    constant: None,
                },
            );
        }
        let mut has_return = false;
        let mut next_id = function.parameters.len();
        let statements = statements::block_with_bindings(
            &function.statements,
            &mut scope,
            &mut next_id,
            &mut has_return,
            0,
            declared,
        )?;
        if function.result != Type::Void {
            if !has_return {
                return Err(function.end.error(ErrorKind::InvalidSource, "a value-returning function requires a return expression [fn.value-return-required]"));
            }
            if !control::stops(&statements) {
                return Err(function.end.error(
                    ErrorKind::InvalidSource,
                    "a value-returning function may reach its end [fn.fallthrough]",
                ));
            }
        }
        warnings.extend(diagnostics::unhandled_arithmetic(&statements));
        functions.push(CheckedFunction {
            id: scope.functions[&function.name].id,
            parameters: function.parameters.iter().map(|p| p.ty).collect(),
            result: function.result,
            statements,
        });
    }
    warnings.sort_by_key(|l| (l.line, l.column));
    Ok(CheckedProgram {
        division_warnings: globals
            .division_warnings
            .borrow()
            .iter()
            .map(|&(line, column)| crate::ast::Location { line, column })
            .collect(),
        comparison_warnings: globals
            .comparison_warnings
            .borrow()
            .iter()
            .map(|(&(line, column), &value)| (crate::ast::Location { line, column }, value))
            .collect(),
        constant_texts: globals.constant_texts.borrow().clone(),
        main_location: program.functions[main_id].keyword_location,
        structs: program.structs.clone(),
        enums: program.enums.clone(),
        functions,
        main_id,
        unhandled_arithmetic: warnings,
    })
}

pub(super) fn call(
    expr: &Expression,
    name: &str,
    arguments: &[Expression],
    scope: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let signature = scope.functions.get(name).ok_or_else(|| {
        expr.location
            .error(ErrorKind::InvalidSource, "unknown function [fn.call]")
    })?;
    if arguments.len() != signature.parameters.len() {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            "argument count does not match the function declaration [fn.call]",
        ));
    }
    let arguments = arguments
        .iter()
        .zip(&signature.parameters)
        .map(|(argument, ty)| expression(argument, Some(*ty), scope))
        .collect::<Result<_, _>>()?;
    Ok(CheckedExpression {
        ty: signature.result,
        value: Value::Call {
            id: signature.id,
            arguments,
        },
    })
}

pub(super) fn return_value(
    value: &Expression,
    scope: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let bare = matches!(value.kind, ExpressionKind::Void);
    if (scope.result == Type::Void) != bare {
        return Err(value.location.error(
            ErrorKind::InvalidSource,
            "return must have a value exactly when the function returns a value [fn.return]",
        ));
    }
    expression(value, Some(scope.result), scope)
}
