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
    let mut signatures = BTreeMap::new();
    for (id, function) in program.functions.iter().enumerate() {
        if signatures.contains_key(&function.name)
            || program.bindings.iter().any(|c| c.name == function.name)
            || program.enums.iter().any(|e| e.name == function.name)
            || program.structs.iter().any(|s| s.name == function.name)
            || program.fnshapes.iter().any(|s| s.name == function.name)
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
    let scope = Scope {
        functions: signatures,
        fnshapes: Rc::new(program.fnshapes.clone()),
        function_types: Rc::new(std::cell::RefCell::new(program.function_types.clone())),
        instances: Rc::new(std::cell::RefCell::new(super::generic::Instances::new(
            program.functions.len(),
        ))),
        ..Scope::default()
    };
    let (globals, mut warnings, initializers) =
        declarations::resolve(&program.bindings, &program.enums, &program.structs, scope)?;
    let mut functions = Vec::new();
    check_instances(program, &globals, &mut functions, &mut warnings)?;
    for (id, function) in program.functions.iter().enumerate() {
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
    // 全関数（未使用の汎用関数を含む）の検査後に、大域変数への構文上の代入を確認する。
    for declaration in program.bindings.iter().filter(|d| d.mutable) {
        if !globals.globals[&declaration.name].assigned.get() {
            return Err(declaration.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "var.never-reassigned",
                    "global var '{}' is never reassigned; declare it with fix",
                    declaration.name
                ),
            ));
        }
    }
    warnings.sort_by_key(|l| (l.line, l.column));
    // 整数演算の箇所は汎用検査と具体化の両方で集まるため、ソースの演算ごとに一度だけ報告する。
    // 比較・除算の候補は汎用検査からだけ集まる。
    warnings.dedup_by_key(|l| (l.line, l.column));
    Ok(CheckedProgram {
        function_types: globals.function_types.borrow().clone(),
        globals: initializers,
        global_warnings: program
            .bindings
            .iter()
            .filter(|d| d.mutable)
            .map(|d| d.location)
            .collect(),
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
    check_with_captures(function, scope, id, &BTreeMap::new())
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
    let mut warnings = Vec::new();
    // [num.fn-same]、[num.fn-body-type]、[num.fn-recursion]: 関数の具体化内の全型注釈に同じ代入を適用する。
    // 汎用の診断は記号的な型で一度収集済み。具体化では定数登録を残すが、型ごとの警告は追加しない。
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
        let parameter_type = scope.resolve_type(parameter.ty);
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
                global: false,
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
    if scope.contains_key(name) {
        let target = Expression {
            kind: ExpressionKind::Name(name.into()),
            location: expr.location,
            height: 1,
        };
        return super::function_values::invoke(expr, &target, arguments, scope);
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
