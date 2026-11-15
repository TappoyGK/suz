use super::{
    control, declarations, diagnostics,
    expression::coerced as expression,
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
    super::generic::validate(program)?;
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
    *globals.instances.borrow_mut() = super::generic::Instances::new(program.functions.len());
    for function in &program.functions {
        if globals.functions[&function.name].is_generic() {
            super::generic::analyze(function, &globals)?;
        }
    }
    let mut functions = Vec::new();
    for (id, function) in program.functions.iter().enumerate() {
        if globals.functions[&function.name].is_generic() {
            continue;
        }
        let (checked, found) = check_function(
            function,
            &globals,
            id,
            super::generic::GenericTypes::default(),
        )?;
        functions.push(checked);
        warnings.extend(found);
    }
    loop {
        let Some(instance) = globals.instances.borrow_mut().queue.pop_front() else {
            break;
        };
        let function = &program.functions[instance.function];
        let mut context = globals.clone();
        context.generic_calls = instance.calls.clone();
        let (checked, found) = check_function(function, &context, instance.id, instance.types)
            .map_err(|error| {
                let mut diagnostic = instance.calls[0].error(
                    error.kind,
                    format!(
                        "generic call to '{}' is invalid for these concrete types [num.fn-body]",
                        function.name
                    ),
                );
                diagnostic
                    .related
                    .extend(instance.calls.iter().skip(1).map(|site| {
                        site.error(
                            ErrorKind::InvalidSource,
                            "instantiated through this call [num.fn-body]",
                        )
                    }));
                diagnostic.related.push(error);
                diagnostic
            })?;
        functions.push(checked);
        warnings.extend(found);
    }
    warnings.sort_by_key(|l| (l.line, l.column));
    // [num.fn-warning] One source operation is reported once across instances.
    warnings.dedup_by_key(|l| (l.line, l.column));
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

pub(super) fn check_function(
    function: &crate::ast::Function,
    globals: &Scope,
    id: usize,
    types: super::generic::GenericTypes,
) -> Result<(CheckedFunction, Vec<crate::ast::Location>), Diagnostic> {
    control::validate_return_positions(&function.statements)?;
    let mut warnings = Vec::new();
    let mut scope = globals.clone();
    scope.generic_types = types;
    if scope.in_generic() && !scope.analyzing_generic {
        scope.comparison_warnings = Default::default();
        scope.division_warnings = Default::default();
    }
    scope.result = scope.resolve_type(function.result);
    let mut declared = Vec::new();
    for (parameter_id, parameter) in function.parameters.iter().enumerate() {
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
        let parameter_type = scope.resolve_type(parameter.ty);
        scope.insert(
            parameter.name.clone(),
            Binding {
                id: parameter_id,
                ty: parameter_type,
                generic_dependent: parameter.ty.generic(),
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
    if function.result != Type::Void
        && (!scope.analyzing_generic || scope.analysis_errors.borrow().is_empty())
    {
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
    Ok((
        CheckedFunction {
            id,
            parameters: function
                .parameters
                .iter()
                .map(|p| scope.resolve_type(p.ty))
                .collect(),
            result: scope.result,
            statements,
        },
        warnings,
    ))
}

pub(super) fn call(
    expr: &Expression,
    name: &str,
    arguments: &[Expression],
    expected: Option<Type>,
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
    let generic = signature.is_generic();
    let types = super::generic::resolve(signature, arguments, expected, scope, expr.location)?;
    let concrete = |ty| types.resolve(ty);
    let id = if generic {
        scope.instances.borrow_mut().request(
            signature.id,
            types,
            scope.generic_calls.clone(),
            expr.location,
        )
    } else {
        signature.id
    };
    let arguments = arguments
        .iter()
        .zip(&signature.parameters)
        .map(|(argument, ty)| {
            // [num.fn-convert] Report the generic parameter's contract, not
            // an incidental default type chosen after a family mismatch.
            expression(argument, Some(concrete(*ty)), scope).map_err(|error| {
                if generic && error.rule() == "type.expected" {
                    argument.location.error(ErrorKind::InvalidSource, if *ty == Type::GenericInt &&
                        (super::expression::has_float_literal(argument, scope) || matches!(super::expression::operand_type(argument, scope), Some(Type::Float(_)))) {
                        "floating-point values cannot be passed to an int parameter [num.fn-convert]".to_owned()
                    } else { format!("argument cannot be passed as {} without loss [num.fn-convert]", scope.type_name(concrete(*ty))) })
                } else { error }
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(CheckedExpression {
        ty: concrete(signature.result),
        value: Value::Call { id, arguments },
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
