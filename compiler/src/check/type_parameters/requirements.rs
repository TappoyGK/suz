//! 宣言した操作だけを定義内で許可し、具体化時には実際の操作の署名と照合する。
use crate::check::{
    expression,
    ir::{CheckedExpression, Value},
    scope::Scope,
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{
        Expression, ExpressionKind, FailureHandler, Location, Type,
        generics::{Need, RequiredOperation},
    },
};

fn target(expr: &Expression, scope: &Scope) -> Option<usize> {
    match expression::operand_type(expr, scope)? {
        Type::Parameter(id) => Some(id),
        _ => None,
    }
}

fn method_target(expr: &Expression, scope: &Scope) -> Option<usize> {
    let ty = expression::operand_type(expr, scope)?;
    if let Type::Parameter(id) = ty.pointee().unwrap_or(ty) {
        Some(id)
    } else {
        None
    }
}

fn find<'a>(
    parameter: usize,
    operation: &RequiredOperation,
    args: &[Expression],
    scope: &'a Scope,
) -> Option<&'a Need> {
    let (method, name) = match operation {
        RequiredOperation::Operator(op) => (false, *op),
        RequiredOperation::Method { name, .. } => (true, name.as_str()),
    };
    scope
        .named
        .needs
        .candidates(parameter, method, name)
        .find(|n| {
            n.parameter == parameter
                && &n.operation == operation
                && n.parameters.len() == args.len()
                && n.parameters.iter().zip(args).all(|(ty, arg)| {
                    expression::operand_type(arg, scope).is_none_or(|known| {
                        scope.resolve_type_at(*ty, arg.location).ok() == Some(known)
                    })
                })
        })
}

/// 構造体の等値比較は各フィールドの==を使うため、型引数のフィールドにも要求が必要。
pub(in crate::check) fn struct_equality(
    ty: Type,
    scope: &Scope,
    location: Location,
) -> Result<(), Diagnostic> {
    if !scope.structs.symbolic(ty) {
        return Ok(());
    }
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        match ty {
            Type::Struct(id) => pending.extend(scope.structs.get(id).fields.iter().map(|f| f.ty)),
            Type::Parameter(id)
                if !scope.named.needs.candidates(id, false, "==").any(|n| {
                    n.parameter == id
                        && n.operation == RequiredOperation::Operator("==")
                        && n.parameters == [ty]
                        && n.result == Type::Bool
                }) =>
            {
                return Err(missing_operator(location, id, "==", scope));
            }
            _ => {}
        }
    }
    Ok(())
}

fn missing_operator(location: Location, parameter: usize, op: &str, scope: &Scope) -> Diagnostic {
    location.error(
        ErrorKind::InvalidSource,
        crate::diagnostic!(
            "generic.operations",
            "operation '{}' on type parameter {} requires a matching @needs declaration",
            op,
            scope.type_name(Type::Parameter(parameter))
        ),
    )
}

fn missing(location: Location) -> Diagnostic {
    location.error(
        ErrorKind::InvalidSource,
        crate::diagnostic!(
            "generic.needs",
            "this operation is not declared by the type parameter's requirements"
        ),
    )
}

/// ヒントの取得は副作用を持たず、要求の結果型だけを返す。実引数はoperationで検査する。
pub(in crate::check) fn hint(expr: &Expression, scope: &Scope) -> Option<Type> {
    if let Some((op, operand)) = crate::check::operator_methods::unary(expr) {
        return find(
            target(operand, scope)?,
            &RequiredOperation::Operator(op),
            &[],
            scope,
        )
        .and_then(|n| scope.resolve_type_at(n.result, expr.location).ok());
    }
    match &expr.kind {
        ExpressionKind::Binary { operator, left, right } => find(target(left, scope)?, &RequiredOperation::Operator(operator.c_symbol()), std::slice::from_ref(right), scope).and_then(|n| scope.resolve_type_at(n.result, expr.location).ok()),
        ExpressionKind::MethodCall { target: receiver, name, mutable, arguments } => method_target(receiver, scope).and_then(|parameter| scope.named.needs.candidates(parameter, true, name).find(|n| {
            n.parameters.len() == arguments.len()
                && matches!(&n.operation, RequiredOperation::Method { mutable: write, .. } if mutable == write)
        })).and_then(|n| scope.resolve_type_at(n.result, expr.location).ok()),
        _ => None,
    }
}

fn invoke(
    need: &Need,
    receiver: &Expression,
    arguments: &[Expression],
    expr: &Expression,
    scope: &Scope,
    handler: Option<&FailureHandler>,
) -> Result<CheckedExpression, Diagnostic> {
    let (arena, mutable) = match need.operation {
        RequiredOperation::Method { arena, mutable, .. } => (arena, mutable),
        _ => (false, false),
    };
    if arena && scope.arena_depth == 0 {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "arena.fn-call",
                "arena method requires an enclosing arena or arena fn"
            ),
        ));
    }
    let receiver = if matches!(need.operation, RequiredOperation::Method { .. }) {
        crate::check::methods::receiver(receiver, mutable, scope)?
    } else {
        expression::expression(receiver, None, scope)?
    };
    let mut checked = vec![receiver];
    for (argument, &ty) in arguments.iter().zip(&need.parameters) {
        checked.push(crate::check::function_values::argument(
            argument,
            scope.resolve_type_at(ty, argument.location)?,
            scope,
        )?);
    }
    // 定義時だけの仮の呼び出し。具体化では元のASTを通常検査するので、
    // この値がC生成へ届いたり、組み込み算術へ呼び出し境界を追加したりすることはない。
    crate::check::errors::finish(
        CheckedExpression {
            ty: scope.resolve_type_at(need.result, expr.location)?,
            value: Value::Call {
                id: usize::MAX,
                arguments: checked,
                arena,
                location: expr.location,
            },
        },
        &need.errors,
        handler,
        expr,
        scope,
    )
}

pub(in crate::check) fn operation(
    expr: &Expression,
    scope: &Scope,
) -> Result<Option<CheckedExpression>, Diagnostic> {
    if let ExpressionKind::ArithmeticHandler { operand, fallback } = &expr.kind {
        let mut inner = operand.as_ref();
        while let ExpressionKind::Group(value) = &inner.kind {
            inner = value;
        }
        if let ExpressionKind::MethodCall {
            target: receiver, ..
        } = &inner.kind
            && method_target(receiver, scope).is_some()
        {
            return method(inner, scope, fallback.as_ref()).map(Some);
        }
        if symbolic_arithmetic(inner, scope) {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "generic.arithmetic-handler",
                    "or cannot handle arithmetic depending on a type parameter"
                ),
            ));
        }
        return Ok(None);
    }
    if let Some((op, operand)) = crate::check::operator_methods::unary(expr)
        && let Some(parameter) = target(operand, scope)
    {
        let need = find(parameter, &RequiredOperation::Operator(op), &[], scope)
            .ok_or_else(|| missing_operator(expr.location, parameter, op, scope))?;
        return invoke(need, operand, &[], expr, scope, None).map(Some);
    }
    match &expr.kind {
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } if target(left, scope).is_some() => {
            if !crate::check::operator_methods::supported(*operator) {
                return Err(missing(expr.location));
            }
            let args = std::slice::from_ref(right.as_ref());
            let need = find(
                target(left, scope).unwrap(),
                &RequiredOperation::Operator(operator.c_symbol()),
                args,
                scope,
            )
            .ok_or_else(|| {
                missing_operator(
                    expr.location,
                    target(left, scope).unwrap(),
                    operator.c_symbol(),
                    scope,
                )
            })?;
            invoke(need, left, args, expr, scope, None).map(Some)
        }
        ExpressionKind::MethodCall {
            target: receiver, ..
        } if method_target(receiver, scope).is_some() => method(expr, scope, None).map(Some),
        ExpressionKind::Comparison {
            operands,
            operators,
            locations,
        } if operands.iter().any(|e| target(e, scope).is_some()) => {
            let mut values = Vec::new();
            for (i, pair) in operands.windows(2).enumerate() {
                if let Some(parameter) = target(&pair[0], scope) {
                    let need = find(
                        parameter,
                        &RequiredOperation::Operator(operators[i].symbol()),
                        &pair[1..],
                        scope,
                    )
                    .ok_or_else(|| {
                        missing_operator(locations[i], parameter, operators[i].symbol(), scope)
                    })?;
                    invoke(need, &pair[0], &pair[1..], expr, scope, None)?;
                } else {
                    let pair_expr = Expression {
                        kind: ExpressionKind::Comparison {
                            operands: pair.to_vec(),
                            operators: vec![operators[i]],
                            locations: vec![locations[i]],
                        },
                        location: locations[i],
                        height: expr.height,
                    };
                    // 左辺が具体型なら、その演算の通常の型規則で右辺も検査する。
                    if target(&pair[1], scope).is_some() {
                        return Err(missing(locations[i]));
                    }
                    expression::expression(&pair_expr, None, scope)?;
                }
            }
            for operand in operands {
                values.push(expression::expression(operand, None, scope)?);
            }
            Ok(Some(CheckedExpression {
                ty: Type::Bool,
                value: Value::Comparison {
                    operands: values,
                    operators: operators.clone(),
                    methods: Vec::new(),
                },
            }))
        }
        _ => Ok(None),
    }
}

fn symbolic_arithmetic(expr: &Expression, scope: &Scope) -> bool {
    match &expr.kind {
        ExpressionKind::Binary { left, right, .. } => {
            target(left, scope).is_some()
                || target(right, scope).is_some()
                || symbolic_arithmetic(left, scope)
                || symbolic_arithmetic(right, scope)
        }
        ExpressionKind::Negate(inner) | ExpressionKind::BitNot(inner) => {
            target(inner, scope).is_some() || symbolic_arithmetic(inner, scope)
        }
        ExpressionKind::Group(inner) => symbolic_arithmetic(inner, scope),
        _ => false,
    }
}

fn method(
    expr: &Expression,
    scope: &Scope,
    handler: Option<&FailureHandler>,
) -> Result<CheckedExpression, Diagnostic> {
    let ExpressionKind::MethodCall {
        target: receiver,
        name,
        mutable,
        arguments,
    } = &expr.kind
    else {
        unreachable!()
    };
    let parameter = method_target(receiver, scope).unwrap();
    let need = scope.named.needs.candidates(parameter, true, name).find(|n| {
        n.parameter == parameter && n.parameters.len() == arguments.len()
            && matches!(&n.operation, RequiredOperation::Method { name: member, mutable: write, .. } if member == name && write == mutable)
    }).ok_or_else(|| missing_operator(expr.location, parameter, name, scope))?;
    invoke(need, receiver, arguments, expr, scope, handler)
}

fn error_spec(errors: &[crate::ast::ErrorVariant], scope: &Scope) -> String {
    if errors.is_empty() {
        return String::new();
    }
    let mut names = Vec::new();
    for group in errors.chunk_by(|a, b| a.type_id == b.type_id) {
        let declaration = &scope.enums[group[0].type_id];
        if group.len() == declaration.variants.len() {
            names.push(declaration.name.clone());
        } else {
            names.extend(
                group
                    .iter()
                    .map(|e| format!("{}.{}", declaration.name, declaration.variants[e.variant])),
            );
        }
    }
    format!(" or {}", names.join(", "))
}

pub(super) fn satisfy(
    need: &Need,
    types: &[Type],
    scope: &Scope,
    location: Location,
) -> Result<(), Diagnostic> {
    let target = types[need.parameter];
    let parameters: Vec<_> = need
        .parameters
        .iter()
        .map(|t| scope.substitute(*t, types, location))
        .collect::<Result<_, _>>()?;
    let result = scope.substitute(need.result, types, location)?;
    let error = || {
        let mut error = location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "generic.needs-check",
                "type {} does not satisfy {}({}) -> {}{}",
                scope.type_name(target),
                match &need.operation {
                    RequiredOperation::Operator(op) => (*op).to_owned(),
                    RequiredOperation::Method {
                        name,
                        mutable,
                        arena,
                    } => format!(
                        "{}{}{name}",
                        if *arena { "arena " } else { "" },
                        if *mutable { "#" } else { "." }
                    ),
                },
                parameters
                    .iter()
                    .map(|t| scope.type_name(*t))
                    .collect::<Vec<_>>()
                    .join(", "),
                scope.type_name(result),
                error_spec(&need.errors, scope)
            ),
        );
        error.related.push(need.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("generic.needs", "requirement declared here"),
        ));
        error
    };
    if let Type::Parameter(parameter) = target {
        let (method, name, _) = need.operation_key();
        return if scope
            .named
            .needs
            .candidates(parameter, method, name)
            .any(|n| {
                n.parameter == parameter
                    && n.operation == need.operation
                    && n.parameters
                        .iter()
                        .map(|t| scope.substitute(*t, &[], location))
                        .collect::<Result<Vec<_>, _>>()
                        .ok()
                        == Some(parameters.clone())
                    && scope.substitute(n.result, &[], location).ok() == Some(result)
                    && n.errors == need.errors
            }) {
            Ok(())
        } else {
            Err(error())
        };
    }
    // 操作の照合に必要なのは宣言と仮のオペランドだけ。呼び出し元の局所変数は複製しない。
    let mut context = Scope {
        functions: scope.functions.clone(),
        structs: scope.structs.clone(),
        enums: scope.enums.clone(),
        function_types: scope.function_types.clone(),
        has_operator_methods: scope.has_operator_methods,
        named: std::rc::Rc::new(crate::check::scope::NamedTypes {
            needs: scope.named.needs.clone(),
            type_parameters: scope.named.type_parameters.clone(),
            ..Default::default()
        }),
        ..Scope::default()
    };
    let mut argument = |index: usize, ty: Type| {
        let name = format!("$requirement{index}");
        context.locals.insert(
            name.clone(),
            crate::check::statements::Binding::local(index, ty, true, false, location),
        );
        Expression {
            kind: ExpressionKind::Name(name),
            location,
            height: 1,
        }
    };
    let receiver = argument(0, target);
    let args: Vec<_> = parameters
        .iter()
        .enumerate()
        .map(|(i, &ty)| argument(i + 1, ty))
        .collect();
    let kind = match &need.operation {
        RequiredOperation::Method {
            name,
            mutable,
            arena,
        } => {
            let key = format!("{}.{}", scope.method_owner_name(target), name);
            let signature = scope.functions.get(&key).ok_or_else(error)?;
            let signature = super::method_signature(signature, target, scope, location)?;
            if signature.method.is_none_or(|m| m.mutable != *mutable)
                || signature.arena != *arena
                || signature.parameters[1..] != parameters
                || signature.result != result
                || signature.errors != need.errors
            {
                return Err(error());
            }
            return Ok(());
        }
        RequiredOperation::Operator(op) => operator(op, receiver.clone(), args.clone(), location)?,
    };
    let expr = Expression {
        kind,
        location,
        height: 2,
    };
    // 演算子メソッドは選択後の署名を一致させる。組み込み演算では、要求の戻り値型を
    // 格納先として通常の規則を検査するが、値域が常に収まることまでは要求しない。
    let selected = if matches!(target, Type::Struct(_) | Type::Enum(_))
        && !matches!(need.operation, RequiredOperation::Operator("==" | "!="))
    {
        let RequiredOperation::Operator(op) = &need.operation else {
            unreachable!()
        };
        if args.is_empty() {
            crate::check::operator_methods::unary_name(op, &receiver, &context)
        } else {
            crate::check::operator_methods::select(op, &receiver, &args[0], &context)
                .map(str::to_owned)
        }
    } else {
        None
    };
    if let Some(name) = selected {
        let signature = scope.functions.get(&name).ok_or_else(error)?;
        let signature = super::method_signature(signature, target, scope, location)?;
        let mut arguments = vec![receiver];
        arguments.extend(args);
        let resolved = crate::check::generic::resolve(
            &signature,
            &arguments,
            Some(result),
            &context,
            location,
        )
        .map_err(|_| error())?;
        let actual = resolved.concrete();
        if actual.resolve(signature.result) != result
            || signature.parameters[1..]
                .iter()
                .map(|t| actual.resolve(*t))
                .collect::<Vec<_>>()
                != parameters
        {
            return Err(error());
        }
        return Ok(());
    }
    let checked = expression::coerced(&expr, Some(result), &context).map_err(|cause| {
        let mut error = error();
        error.related.push(cause);
        error
    })?;
    if checked.ty != result {
        return Err(error());
    }
    Ok(())
}

fn operator(
    op: &str,
    receiver: Expression,
    args: Vec<Expression>,
    location: Location,
) -> Result<ExpressionKind, Diagnostic> {
    use crate::ast::{BinaryOperator, ComparisonOperator};
    if args.is_empty() {
        let inner = Box::new(receiver);
        return Ok(match op {
            "-" => ExpressionKind::Negate(inner),
            "~" => ExpressionKind::BitNot(inner),
            "not" => ExpressionKind::Not(inner),
            "len" => ExpressionKind::Len(inner),
            _ => return Err(missing(location)),
        });
    }
    if let Some(operator) = ComparisonOperator::from_symbol(op) {
        return Ok(ExpressionKind::Comparison {
            operands: vec![receiver, args[0].clone()],
            operators: vec![operator],
            locations: vec![location],
        });
    }
    let operator = BinaryOperator::from_symbol(op).ok_or_else(|| missing(location))?;
    Ok(ExpressionKind::Binary {
        operator,
        left: Box::new(receiver),
        right: Box::new(args[0].clone()),
    })
}

/// 別々の型引数が同じ具体型になった場合も、矛盾する要求を二つの契約として扱わない。
pub(super) fn validate_substitution(
    needs: &[Need],
    types: &[Type],
    scope: &Scope,
    location: Location,
) -> Result<(), Diagnostic> {
    // 各要求の型と充足は呼び出し元で検査する。二つ未満なら要求間の矛盾はない。
    if needs.len() < 2 {
        return Ok(());
    }
    let mut resolved: Vec<(Type, Need)> = Vec::new();
    for need in needs {
        let target = types[need.parameter];
        let mut concrete = need.clone();
        concrete.parameter = 0;
        concrete.parameters = need
            .parameters
            .iter()
            .map(|t| scope.substitute(*t, types, location))
            .collect::<Result<_, _>>()?;
        concrete.result = scope.substitute(need.result, types, location)?;
        resolved.push((target, concrete));
    }
    let mut signatures = std::collections::BTreeMap::new();
    for (target, concrete) in &resolved {
        if let Some(previous) = signatures.insert((*target, concrete.operation_key()), concrete)
            && !previous.same_signature(concrete)
        {
            let mut error = location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "generic.needs-check",
                    "type substitution makes requirement signatures conflict"
                ),
            );
            for site in [previous.definition, concrete.definition] {
                error.related.push(site.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("generic.needs", "conflicting requirement declared here"),
                ));
            }
            return Err(error);
        }
    }
    Ok(())
}
