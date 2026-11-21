//! 関数の宣言・引数・戻り値と本体を検査する。汎用関数の記号的な検査と、呼び出しに必要な具体化をまとめる。
//!
//! 利用元: [`crate::check`]。
//! 主な対象規則: `[decl.duplicate]`、`[decl.shadowing]`、`[fn.call]`、`[fn.parameter-scope]`、`[fn.return]`、`[num.fn-body]`。

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
    let structs = Rc::new(super::type_parameters::structs::Structs::new(program));
    structs.prepare()?;
    let mut signatures = BTreeMap::new();
    for (id, function) in program.functions.iter().enumerate() {
        let mut method = function.method;
        if let Some(method) = &mut method {
            method.owner = structs.resolve_with_parameters(
                method.owner,
                &[],
                function.location,
                &function.type_parameters,
            )?;
        }
        if let Some(method) = method {
            let member = function.name.rsplit('.').next().unwrap();
            let collision = match method.owner {
                Type::Struct(id) => structs.get(id).fields.iter().any(|f| f.name == member),
                Type::Enum(ty) => program.enums[ty.id].variants.iter().any(|v| v == member),
                _ => unreachable!(),
            };
            if method.operator.is_some() && signatures.contains_key(&function.name) {
                return Err(function.location.error(
                    ErrorKind::InvalidSource,
                    if function.parameters.len() == 1 {
                        crate::diagnostic!(
                            "operator.unary",
                            "duplicate unary operator definition"
                        )
                    } else {
                        crate::diagnostic!(
                            "operator.overload",
                            "duplicate operator definition for this right-hand type or numeric family"
                        )
                    },
                ));
            }
            if collision || signatures.contains_key(&function.name) {
                return Err(function.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "method.name",
                        "method name conflicts with a field, variant or another method"
                    ),
                ));
            }
        }
        if signatures.contains_key(&function.name)
            || program.bindings.iter().any(|c| c.name == function.name)
            || program.enums.iter().any(|e| e.name == function.name)
            || structs.contains_name(&function.name)
            || structs.shapes.contains_name(&function.name)
        {
            return Err(function.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("decl.duplicate", "duplicate top-level name"),
            ));
        }
        signatures.insert(
            function.name.clone(),
            Signature {
                type_parameters: function.type_parameters.len(),
                type_parameter_names: (!function.type_parameters.is_empty()).then(|| {
                    Rc::new(
                        function
                            .type_parameters
                            .iter()
                            .map(|p| p.name.clone())
                            .collect(),
                    )
                }),
                needs: function.needs.clone(),
                method,
                external: false,
                id,
                parameters: function
                    .parameters
                    .iter()
                    .map(|p| {
                        structs.resolve_with_parameters(
                            p.ty,
                            &[],
                            p.location,
                            &function.type_parameters,
                        )
                    })
                    .collect::<Result<_, _>>()?,
                result: structs.resolve_with_parameters(
                    function.result,
                    &[],
                    function.location,
                    &function.type_parameters,
                )?,
                errors: function.errors.clone(),
            },
        );
    }
    super::ffi::signatures(program, &mut signatures)?;
    let main_id = signatures["main"].id;
    let signatures = Rc::new(signatures);
    let scope = Scope {
        function_types: structs.shapes.types.clone(),
        structs,
        has_operator_methods: program
            .functions
            .iter()
            .any(|f| f.method.is_some_and(|m| m.operator.is_some())),
        functions: signatures,
        instances: Rc::new(std::cell::RefCell::new(super::generic::Instances::new(
            program.functions.len() + program.externs.len(),
        ))),
        ..Scope::default()
    };
    let (globals, mut warnings) = declarations::resolve(&program.bindings, &program.enums, scope)?;
    for group in &program.need_groups {
        definition_scope(
            &globals,
            &group.type_parameters,
            &group.needs,
            group.location,
        )?;
    }
    for declaration in &program.fnshapes {
        // 型引数のない宣言には、空の型引数スコープを作らない。
        let context;
        let scope = if declaration.type_parameters.is_empty() {
            &globals
        } else {
            context = definition_scope(
                &globals,
                &declaration.type_parameters,
                &declaration.needs,
                declaration.location,
            )?;
            &context
        };
        let signature = &program.function_types[declaration.type_id];
        for &ty in signature.parameters.iter().chain([&signature.result]) {
            scope.resolve_type_at(ty, declaration.location)?;
        }
    }
    // フィールドの型はprepare/applyで解決済み。ここでは宣言に書かれた要求を確認する。
    for (id, declaration) in program.structs.iter().enumerate() {
        let context;
        let (scope, ty) = if declaration.type_parameters.is_empty() {
            (&globals, Type::Struct(id))
        } else {
            context = definition_scope(
                &globals,
                &declaration.type_parameters,
                &declaration.needs,
                declaration.location,
            )?;
            let ty = context.structs.apply(
                id,
                (0..declaration.type_parameters.len())
                    .map(Type::Parameter)
                    .collect(),
                declaration.location,
            )?;
            (&context, ty)
        };
        super::type_parameters::struct_requirements(ty, scope, declaration.location)?;
        if scope.structs.shapes.has_generics {
            for field in &declaration.fields {
                super::type_parameters::shape_requirements(field.ty, scope, field.location)?;
            }
        }
    }
    let mut functions = Vec::new();
    check_instances(program, &globals, &mut functions, &mut warnings)?;
    for (id, function) in program.functions.iter().enumerate() {
        if !function.type_parameters.is_empty() {
            let context = definition_scope(
                &globals,
                &function.type_parameters,
                &function.needs,
                function.location,
            )?;
            let (_, found) = check_function(function, context, id)?;
            warnings.extend(found);
            continue;
        }
        if globals.functions[&function.name].is_generic() {
            match &analyze_once(function, id, &globals).result {
                Ok(found) => warnings.extend(found.iter().copied()),
                Err(error) => return Err(error.clone()),
            }
            continue;
        }
        let result = check_function(function, globals.clone(), id);
        // 本体内のエラーより前に現れた呼び出しを先に検査する。明示的な作業スタックで呼び出し順を保ち、
        // 汎用関数の連鎖に比例してRustの呼び出しスタックが増えるのを防ぐ。
        check_instances(program, &globals, &mut functions, &mut warnings)?;
        let (checked, found) = result?;
        functions.push(checked);
        warnings.extend(found);
    }
    // 名前付き型引数の関数から既存の数値汎用関数を呼ぶ場合と、その逆の両方を処理する。
    loop {
        super::type_parameters::check_instances(program, &globals, &mut functions, &mut warnings)?;
        check_instances(program, &globals, &mut functions, &mut warnings)?;
        let instances = globals.instances.borrow();
        if instances.named_cursor == instances.named.len() && instances.queue.is_empty() {
            break;
        }
    }
    super::arena::check_program(
        &mut functions,
        program.functions.len()..program.functions.len() + program.externs.len(),
    )?;
    warnings.sort_by_key(|l| (l.line, l.column));
    // 整数演算の箇所は汎用検査と具体化の両方で集まるため、ソースの演算ごとに一度だけ報告する。
    // 比較・除算の候補は汎用検査からだけ集まる。
    warnings.dedup_by_key(|l| (l.line, l.column));
    Ok(CheckedProgram {
        externs: program.externs.clone(),
        external_base: program.functions.len(),
        function_types: globals.function_types.borrow().clone(),
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
        structs: globals.structs.snapshot(),
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
                // 再入する再帰呼び出しは既存のシグネチャを使い、完成した具体化は共有する。
                // 各本体は多くても一度だけ構築する。
                if !globals
                    .instances
                    .borrow_mut()
                    .checked_instances
                    .insert(instance.id)
                {
                    continue;
                }
                let function = &program.functions[instance.function];
                // 前方参照には汎用検査の結果が必要だが、型によらないエラーは呼び出しではなく宣言の順番で報告する。
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
                // 汎用検査を通過した本体だけを、具体型で構築し直す。
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
                super::generic::Deferred::Rule {
                    failures,
                    diagnostic,
                } => {
                    let index = instance.types.index();
                    if failures & (1 << index) != 0 {
                        return Err(instance_error(program, &instance, diagnostic(index)));
                    }
                }
                super::generic::Deferred::Call {
                    function,
                    types,
                    location,
                    function_value,
                } => {
                    if let Some(types) = types[instance.types.index()] {
                        globals.instances.borrow_mut().request(
                            *function,
                            types,
                            &Some(instance.calls.clone()),
                            *location,
                            *function_value,
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
    let mut diagnostic = calls[0].0.error(
        error.kind,
        crate::diagnostic!(
            if calls[0].1 {
                "fn.value-generic-check"
            } else {
                "num.fn-body"
            },
            "{} '{}' is invalid for these concrete types",
            if calls[0].1 {
                "generic function value for"
            } else {
                "generic call to"
            },
            program.functions[instance.function].name
        ),
    );
    diagnostic
        .related
        .extend(calls.iter().skip(1).map(|(site, function_value)| {
            site.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    if *function_value {
                        "fn.value-generic-check"
                    } else {
                        "num.fn-body"
                    },
                    "{}",
                    if *function_value {
                        "instantiated through this function value"
                    } else {
                        "instantiated through this call"
                    }
                ),
            )
        }));
    diagnostic.related.push(error);
    diagnostic
}

pub(super) fn check_function(
    function: &crate::ast::Function,
    scope: Scope,
    id: usize,
) -> Result<(CheckedFunction, Vec<crate::ast::Location>), Diagnostic> {
    let mut references = BTreeMap::new();
    if let Some(method) = function.method.filter(|m| m.mutable) {
        references.insert(
            "self".into(),
            Binding::local(0, method.owner, true, false, function.location),
        );
    }
    check_with_captures(function, scope, id, &references)
}

// 参照キャプチャーは外側の束縛への別名。未代入の判定は外側のスコープで行い、
// 本体内の代入状態を呼び出し元へ返す。通常の仮引数と値キャプチャーは従来どおり不変。
pub(super) fn check_with_captures(
    function: &crate::ast::Function,
    mut scope: Scope,
    id: usize,
    references: &BTreeMap<String, Binding>,
) -> Result<(CheckedFunction, Vec<crate::ast::Location>), Diagnostic> {
    control::validate_return_positions(&function.statements)?;
    scope.trusted_block = false;
    scope.arena_depth = 1;
    scope.recovery = None;
    scope.loop_depth = 0;
    let mut warnings = Vec::new();
    // [num.fn-same]、[num.fn-body-type]、[num.fn-recursion]: 関数の具体化内の全型注釈に同じ代入を適用する。
    // 汎用の診断は記号的な型で一度収集済み。具体化では定数登録を残すが、型ごとの警告は追加しない。
    if scope.instance() {
        scope.outputs.comparisons = Default::default();
        scope.outputs.divisions = Default::default();
    }
    scope.errors = Rc::new(function.errors.clone());
    scope.result = scope.resolve_type_at(function.result, function.location)?;
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
        let parameter_type = scope.resolve_type_at(parameter.ty, parameter.location)?;
        if let Some(binding) = references.get(&parameter.name) {
            let mut binding = binding.clone();
            binding.id = parameter_id;
            binding.ty = parameter_type;
            scope.locals.insert(parameter.name.clone(), binding);
            continue;
        }
        declared.push(parameter.name.clone());
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
    for (name, binding) in references {
        if scope.locals[name].assigned.get() {
            binding.assigned.set(true);
        }
    }
    let lifetime = super::arena::check_lifetimes(&statements, &scope, function, references, id)?;
    control::check_return_shape(function)?;
    warnings.extend(diagnostics::unhandled_arithmetic(&statements));
    Ok((
        CheckedFunction {
            lifetime,
            mutable_receiver: function.method.is_some_and(|m| m.mutable),
            id,
            parameters: function
                .parameters
                .iter()
                .map(|p| scope.resolve_type_at(p.ty, p.location))
                .collect::<Result<_, _>>()?,
            result: scope.result,
            fallible: !function.errors.is_empty(),
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
    handler: Option<&crate::ast::FailureHandler>,
) -> Result<CheckedExpression, Diagnostic> {
    if scope.contains_key(name) {
        let target = Expression {
            kind: ExpressionKind::Name(name.into()),
            location: expr.location,
            height: 1,
        };
        return super::function_values::invoke(expr, &target, arguments, scope, handler);
    }
    let signature = scope.functions.get(name).ok_or_else(|| {
        expr.location.error(
            ErrorKind::InvalidSource,
            if scope.uncaptured.contains(name) {
                crate::diagnostic!(
                    "closure.scope",
                    "outer binding '{name}' must be listed in the capture list"
                )
            } else {
                crate::diagnostic!("fn.call", "unknown function")
            },
        )
    })?;

    if signature.external && !scope.trusted_block {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("ffi.trusted", "C calls require a trusted block"),
        ));
    }
    if arguments.len() != signature.parameters.len() {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "fn.call",
                "argument count does not match the function declaration"
            ),
        ));
    }
    let explicit = match &expr.kind {
        ExpressionKind::Call { type_arguments, .. } => type_arguments.as_slice(),
        _ => &[],
    };
    if signature.type_parameters != 0 || !explicit.is_empty() {
        return super::type_parameters::call(expr, signature, explicit, arguments, scope, handler);
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
                super::function_values::argument(argument, concrete(*ty), scope)
            }
        })
        .collect::<Result<_, _>>()?;
    if generic && scope.generic_check() {
        scope.defer_call(signature.id, types.int, types.float, expr.location, false);
    }
    let id = if generic && !scope.generic_check() {
        scope.instances.borrow_mut().request(
            signature.id,
            types.concrete(),
            &scope.generic_calls,
            expr.location,
            false,
        )
    } else {
        signature.id
    };
    super::errors::finish(
        CheckedExpression {
            ty: concrete(signature.result),
            value: Value::Call {
                id,
                arguments,
                location: expr.location,
            },
        },
        &signature.errors,
        handler,
        expr,
        scope,
    )
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

/// 要求のある定義だけで要求検査を行い、通常の宣言は呼び出し元のスコープを使う。
fn definition_scope(
    globals: &Scope,
    parameters: &[crate::ast::generics::TypeParameter],
    needs: &[crate::ast::generics::Need],
    location: crate::ast::Location,
) -> Result<Scope, Diagnostic> {
    let mut context = globals.clone();
    context.named = Rc::new(super::scope::NamedTypes {
        type_parameters: Rc::new(parameters.to_vec()),
        needs: Rc::new(needs.to_vec().into()),
        type_arguments: None,
        instantiation_path: Rc::default(),
    });
    if !needs.is_empty() {
        super::type_parameters::definition_needs(&context, location)?;
    }
    Ok(context)
}
