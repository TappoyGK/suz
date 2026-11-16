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
                crate::diagnostic!("decl.duplicate", "duplicate top-level name"),
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
    let mut functions = Vec::new();
    for (id, function) in program.functions.iter().enumerate() {
        if globals.functions[&function.name].is_generic() {
            match &analyze_once(function, id, &globals).result {
                Ok(found) => warnings.extend(found.iter().copied()),
                Err(error) => return Err(error.clone()),
            }
            continue;
        }
        let result = check_function(function, globals.clone(), id);
        // Calls encountered before a local error must be validated first. The
        // explicit work stack preserves call order without a Rust call stack
        // proportional to a chain of generic functions.
        check_instances(program, &globals, &mut functions, &mut warnings)?;
        let (checked, found) = result?;
        functions.push(checked);
        warnings.extend(found);
    }
    warnings.sort_by_key(|l| (l.line, l.column));
    // Integer arithmetic sites are collected in generic checks and concrete
    // instances; report each source operation once. Comparisons/divisions
    // already come only from the generic check.
    warnings.dedup_by_key(|l| (l.line, l.column));
    Ok(CheckedProgram {
        division_warnings: globals
            .outputs
            .divisions
            .borrow()
            .iter()
            .map(|&(line, column)| crate::ast::Location { line, column })
            .collect(),
        comparison_warnings: globals
            .outputs
            .comparisons
            .borrow()
            .iter()
            .map(|(&(line, column), &value)| (crate::ast::Location { line, column }, value))
            .collect(),
        constant_texts: globals.outputs.constant_texts.borrow().clone(),
        main_location: program.functions[main_id].keyword_location,
        structs: program.structs.clone(),
        enums: program.enums.clone(),
        functions,
        main_id,
        unhandled_arithmetic: warnings,
    })
}

fn analyze_once(
    function: &crate::ast::Function,
    id: usize,
    scope: &Scope,
) -> Rc<super::generic::Analysis> {
    if let Some(result) = scope.instances.borrow().analyzed_functions.get(&id) {
        return result.clone();
    }
    let result = Rc::new(super::generic::analyze(function, scope));
    scope
        .instances
        .borrow_mut()
        .analyzed_functions
        .insert(id, result.clone());
    result
}

fn check_instances(
    program: &Program,
    globals: &Scope,
    functions: &mut Vec<CheckedFunction>,
    warnings: &mut Vec<crate::ast::Location>,
) -> Result<(), Diagnostic> {
    enum Task {
        Check(super::generic::Instance),
        Deferred(
            super::generic::Instance,
            Rc<super::generic::Analysis>,
            usize,
        ),
        Finish(
            super::generic::Instance,
            Result<(CheckedFunction, Vec<crate::ast::Location>), Diagnostic>,
        ),
    }
    let mut tasks: Vec<_> = globals
        .instances
        .borrow_mut()
        .queue
        .drain(..)
        .rev()
        .map(Task::Check)
        .collect();
    while let Some(task) = tasks.pop() {
        match task {
            Task::Check(instance) => {
                // Re-entrant recursion uses its existing signature; a completed
                // instance is shared. Every body is constructed at most once.
                if !globals
                    .instances
                    .borrow_mut()
                    .checked_instances
                    .insert(instance.id)
                {
                    continue;
                }
                let function = &program.functions[instance.function];
                // Forward calls need the generic result, but an unconditional
                // error belongs to the declaration's turn, not to this call.
                let analysis = analyze_once(function, instance.function, globals);
                if analysis.result.is_err() {
                    for index in (0..analysis.deferred.len()).rev() {
                        tasks.push(Task::Deferred(instance.clone(), analysis.clone(), index));
                    }
                    continue;
                }
                let mut context = globals.clone();
                context.generic_calls = Some(instance.calls.clone());
                context.mode = super::scope::Mode::Instance(instance.types);
                // Only a valid generic body is rebuilt with concrete types.
                let result = check_function(function, context, instance.id);
                tasks.push(Task::Finish(instance, result));
                tasks.extend(
                    globals
                        .instances
                        .borrow_mut()
                        .queue
                        .drain(..)
                        .rev()
                        .map(Task::Check),
                );
            }
            Task::Deferred(instance, analysis, index) => match &analysis.deferred[index] {
                super::generic::Deferred::Rule(errors) => {
                    if let Some(error) = &errors[instance.types.index()] {
                        return Err(instance_error(program, &instance, error.clone()));
                    }
                }
                super::generic::Deferred::Call {
                    function,
                    types,
                    location,
                } => {
                    if let Some(types) = types[instance.types.index()] {
                        globals.instances.borrow_mut().request(
                            *function,
                            types,
                            &Some(instance.calls.clone()),
                            *location,
                        );
                        tasks.extend(
                            globals
                                .instances
                                .borrow_mut()
                                .queue
                                .drain(..)
                                .rev()
                                .map(Task::Check),
                        );
                    }
                }
            },
            Task::Finish(instance, result) => {
                let (checked, found) =
                    result.map_err(|error| instance_error(program, &instance, error))?;
                functions.push(checked);
                warnings.extend(found);
            }
        }
    }
    Ok(())
}

fn instance_error(
    program: &Program,
    instance: &super::generic::Instance,
    error: Diagnostic,
) -> Diagnostic {
    let calls = instance.calls.locations();
    let mut diagnostic = calls[0].error(
        error.kind,
        crate::diagnostic!(
            "num.fn-body",
            "generic call to '{}' is invalid for these concrete types",
            program.functions[instance.function].name
        ),
    );
    diagnostic.related.extend(calls.iter().skip(1).map(|site| {
        site.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("num.fn-body", "instantiated through this call"),
        )
    }));
    diagnostic.related.push(error);
    diagnostic
}

pub(super) fn check_function(
    function: &crate::ast::Function,
    mut scope: Scope,
    id: usize,
) -> Result<(CheckedFunction, Vec<crate::ast::Location>), Diagnostic> {
    control::validate_return_positions(&function.statements)?;
    let mut warnings = Vec::new();
    // [num.fn-same], [num.fn-body-type], [num.fn-recursion]: a single
    // substitution applies to all annotations in this function instance.
    // Generic diagnostics were collected once with symbolic types. Instances
    // retain constant registrations, but cannot add type-specific warnings.
    if scope.instance() {
        scope.outputs.comparisons = Default::default();
        scope.outputs.divisions = Default::default();
    }
    scope.result = scope.resolve_type(function.result);
    let mut declared = Vec::new();
    for (parameter_id, parameter) in function.parameters.iter().enumerate() {
        if declared.contains(&parameter.name) {
            return Err(parameter.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("fn.parameter-scope", "duplicate parameter name"),
            ));
        }
        if scope.contains_key(&parameter.name)
            || scope.functions.contains_key(&parameter.name)
            || scope.is_type_name(&parameter.name)
        {
            return Err(parameter.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("decl.shadowing", "parameter shadows a top-level name"),
            ));
        }
        declared.push(parameter.name.clone());
        let parameter_type = scope.resolve_type(parameter.ty);
        scope.locals.insert(
            parameter.name.clone(),
            Binding {
                id: parameter_id,
                ty: parameter_type,
                mutable: false,
                parameter: true,
                loop_variable: false,
                assigned: Default::default(),
                location: parameter.location,
                constant: None,
            },
        );
    }
    let mut next_id = function.parameters.len();
    let statements = statements::block_with_bindings(
        &function.statements,
        &mut scope,
        &mut next_id,
        0,
        declared,
    )?;
    control::check_return_shape(function)?;
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
        expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("fn.call", "unknown function"),
        )
    })?;
    if arguments.len() != signature.parameters.len() {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "fn.call",
                "argument count does not match the function declaration"
            ),
        ));
    }
    let generic = signature.is_generic();
    let types = super::generic::resolve(signature, arguments, expected, scope, expr.location)?;
    let concrete = |ty| types.resolve(ty);
    let arguments = arguments
        .iter()
        .zip(&signature.parameters)
        .map(|(argument, ty)| {
            if let Some(family) = super::generic::Family::of(*ty) {
                super::expression::generic_argument(argument, concrete(*ty), family, scope)
            } else {
                expression(argument, Some(concrete(*ty)), scope)
            }
        })
        .collect::<Result<_, _>>()?;
    if generic && scope.generic_check() {
        scope.defer_call(signature.id, types.int, types.float, expr.location);
    }
    let id = if generic && !scope.generic_check() {
        scope.instances.borrow_mut().request(
            signature.id,
            types.concrete(),
            &scope.generic_calls,
            expr.location,
        )
    } else {
        signature.id
    };
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
            crate::diagnostic!(
                "fn.return",
                "return must have a value exactly when the function returns a value"
            ),
        ));
    }
    expression(value, Some(scope.result), scope)
}
