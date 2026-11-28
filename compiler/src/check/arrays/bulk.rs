//! 1次元配列への要素ごとの一括書き込み（`out#[] = 式`、`out#[s:e] = 式`、複合代入）を検査する。
//!
//! 利用元: [`crate::check::statements`]。
//! 主な対象規則: `[array.bulk]`、`[array.write]`。

use super::bounds;
use crate::{
    Diagnostic, ErrorKind,
    ast::{
        ArrayBulkWrite, AssignmentOperator, BinaryOperator, BulkSelector, Expression,
        ExpressionKind, FailureHandler, Location, Type,
    },
    check::{
        control::lower_return_handler,
        expression,
        ir::{BulkInput, CheckedBulkWrite, CheckedExpression, CheckedMask, CheckedStatement},
        scope::Scope,
        statements::{Binding, StatementContext},
    },
};

pub(in crate::check) fn check(
    bulk: &ArrayBulkWrite,
    context: &mut StatementContext<'_>,
) -> Result<(), Diagnostic> {
    if matches!(bulk.operator, Some(AssignmentOperator::Append)) {
        return Err(bulk.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("array.bulk", "bulk write does not support <-"),
        ));
    }

    let mut target = expression::expression(&bulk.target, None, context.bindings)?;
    let Some(target_array_id) = target.ty.array_id() else {
        return Err(bulk.target.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("array.write", "bulk write requires an array reference"),
        ));
    };
    if target.ty.is_nilable() {
        return Err(bulk.target.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "array.value",
                "use let before writing to a possibly nil array"
            ),
        ));
    }
    if target.ty.is_readonly_array() {
        return Err(bulk.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "array.write",
                "cannot write through a readonly array reference"
            ),
        ));
    }
    let target_array = context
        .bindings
        .structs
        .arrays
        .borrow()
        .get(target_array_id);
    if target_array.rank() != 1 {
        return Err(bulk.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("array.write", "bulk write requires a 1D array"),
        ));
    }
    let target_enum = target_array.first_enum();
    if target_enum.is_some() && matches!(bulk.selector, BulkSelector::Slice { .. }) {
        return Err(bulk.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("array.enum-index", "enum-indexed arrays cannot be sliced"),
        ));
    }
    let out_elem_ty = target_array.element;

    lower_return_handler(
        &mut target,
        &mut context.output,
        context.next_id,
        bulk.target.location,
    );

    let (slice, mut start, mut end, slice_proven, slice_location) = match &bulk.selector {
        BulkSelector::All => (false, None, None, true, bulk.location),
        BulkSelector::Slice { start, end } => {
            let checked_start = if let Some(s) = start {
                let mut v = expression::expression(s, None, context.bindings)?;
                if !v.ty.is_integer() {
                    return Err(s.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("array.index", "array slice bounds require integers"),
                    ));
                }
                lower_return_handler(&mut v, &mut context.output, context.next_id, s.location);
                Some(v)
            } else {
                None
            };
            let checked_end = if let Some(e) = end {
                let mut v = expression::expression(e, None, context.bindings)?;
                if !v.ty.is_integer() {
                    return Err(e.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("array.index", "array slice bounds require integers"),
                    ));
                }
                lower_return_handler(&mut v, &mut context.output, context.next_id, e.location);
                Some(v)
            } else {
                None
            };
            let proven = bounds::slice(
                &target,
                checked_start.as_ref(),
                checked_end.as_ref(),
                context.bindings,
            )
            .is_some();
            (true, checked_start, checked_end, proven, bulk.location)
        }
    };

    let out_len_affine = if slice {
        bounds::slice_length_affine(&target, start.as_ref(), end.as_ref(), context.bindings)
    } else {
        bounds::target_dim_affine(&target, 0, context.bindings)
    };

    let mut lowerer = Lowerer {
        context,
        out_len_affine,
        target_enum,
        inputs: Vec::new(),
        temp_names: Vec::new(),
    };

    let rhs_result =
        lowerer.lower_root(&bulk.value, bulk.operator.is_none().then_some(out_elem_ty));
    let lowered_rhs = match rhs_result {
        Ok(expr) => expr,
        Err(err) => {
            lowerer.cleanup();
            return Err(err);
        }
    };

    let (out_elem_id, element_result) = if let Some(AssignmentOperator::Binary(op)) = bulk.operator
    {
        let id = *lowerer.context.next_id;
        *lowerer.context.next_id += 1;
        let name = format!("$bulk_out_elem_{id}");
        lowerer.context.bindings.locals.insert(
            name.clone(),
            Binding::local(id, out_elem_ty, false, false, bulk.location),
        );
        lowerer.temp_names.push(name.clone());
        let compound_expr = Expression {
            height: lowered_rhs.height + 2,
            location: bulk.location,
            kind: ExpressionKind::Binary {
                operator: op,
                left: Box::new(Expression {
                    height: 1,
                    location: bulk.location,
                    kind: ExpressionKind::Name(name),
                }),
                right: Box::new(lowered_rhs),
            },
        };
        let checked = crate::check::operator_methods::compound(
            &compound_expr,
            out_elem_ty,
            lowerer.context.bindings,
        );
        (Some(id), checked)
    } else {
        let checked =
            expression::coerced(&lowered_rhs, Some(out_elem_ty), lowerer.context.bindings);
        (None, checked)
    };

    let inputs = std::mem::take(&mut lowerer.inputs);
    lowerer.cleanup();

    let element_value = element_result?;
    if contains_return_handler_ir(&element_value) {
        return Err(bulk.value.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "array.bulk-handler",
                "or return is not allowed inside a bulk write element expression"
            ),
        ));
    }

    // 事前評価される入力配列・スカラー内の `or return` はループ前に持ち上げる。
    let mut lowered_inputs = Vec::with_capacity(inputs.len());
    for mut input in inputs {
        match &mut input {
            BulkInput::Array { expr, location, .. } => {
                lower_return_handler(expr, &mut context.output, context.next_id, *location);
            }
            BulkInput::Scalar { expr, .. } => {
                lower_return_handler(
                    expr,
                    &mut context.output,
                    context.next_id,
                    bulk.value.location,
                );
            }
        }
        lowered_inputs.push(input);
    }

    // 念のため start / end の未使用警告を避けつつ保持
    let _ = (&mut start, &mut end);

    context
        .bindings
        .array_bounds
        .facts
        .borrow_mut()
        .forget_shared();
    context
        .output
        .push(CheckedStatement::ArrayBulkWrite(Box::new(
            CheckedBulkWrite {
                target,
                start,
                end,
                slice,
                slice_proven,
                slice_location,
                inputs: lowered_inputs,
                out_elem_id,
                mask: None,
                element_value,
                location: bulk.location,
            },
        )));
    Ok(())
}

/// `out#[条件] = 式` の条件位置に現れる配列比較マスクかどうかを判定する。
pub(in crate::check) fn is_mask_expr(expr: &Expression, scope: &Scope) -> bool {
    match &expr.kind {
        ExpressionKind::Group(inner) | ExpressionKind::BitNot(inner) => is_mask_expr(inner, scope),
        ExpressionKind::Binary {
            operator: BinaryOperator::And | BinaryOperator::Or | BinaryOperator::Xor,
            left,
            right,
        } => is_mask_expr(left, scope) || is_mask_expr(right, scope),
        ExpressionKind::Comparison { operands, .. } => operands
            .iter()
            .any(|operand| contains_array_expr(operand, scope)),
        _ => false,
    }
}

/// マスク付き一括書き込み（`out#[条件] = 式`、`out#[条件] += 式`）を検査する。
pub(in crate::check) fn check_mask(
    base: &Expression,
    mask_expr: &Expression,
    value: &Expression,
    operator: Option<AssignmentOperator>,
    location: Location,
    context: &mut StatementContext<'_>,
) -> Result<(), Diagnostic> {
    if matches!(operator, Some(AssignmentOperator::Append)) {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("array.bulk", "bulk write does not support <-"),
        ));
    }

    // 切り出しとマスクの直接連結（`out.[s:e]#[条件] = 式`）は対象外とする。
    let mut ungrouped_base = base;
    while let ExpressionKind::Group(inner) = &ungrouped_base.kind {
        ungrouped_base = inner;
    }
    if matches!(
        ungrouped_base.kind,
        ExpressionKind::Slice { index: false, .. }
    ) {
        return Err(base.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "array.bulk-mask",
                "slicing cannot be chained directly with a masked bulk write; bind the slice to a variable first"
            ),
        ));
    }

    let mut target = expression::expression(base, None, context.bindings)?;
    let Some(target_array_id) = target.ty.array_id() else {
        return Err(base.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("array.write", "bulk write requires an array reference"),
        ));
    };
    if target.ty.is_nilable() {
        return Err(base.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "array.value",
                "use let before writing to a possibly nil array"
            ),
        ));
    }
    if target.ty.is_readonly_array() {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "array.write",
                "cannot write through a readonly array reference"
            ),
        ));
    }
    let target_array = context
        .bindings
        .structs
        .arrays
        .borrow()
        .get(target_array_id);
    if target_array.rank() != 1 {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("array.write", "bulk write requires a 1D array"),
        ));
    }
    let target_enum = target_array.first_enum();
    let out_elem_ty = target_array.element;

    lower_return_handler(
        &mut target,
        &mut context.output,
        context.next_id,
        base.location,
    );

    let out_len_affine = bounds::target_dim_affine(&target, 0, context.bindings);

    let mut lowerer = Lowerer {
        context,
        out_len_affine,
        target_enum,
        inputs: Vec::new(),
        temp_names: Vec::new(),
    };

    let mask_result = lowerer.lower_mask(mask_expr);
    let checked_mask = match mask_result {
        Ok(m) => m,
        Err(err) => {
            lowerer.cleanup();
            return Err(err);
        }
    };

    let rhs_result = lowerer.lower_root(value, operator.is_none().then_some(out_elem_ty));
    let lowered_rhs = match rhs_result {
        Ok(expr) => expr,
        Err(err) => {
            lowerer.cleanup();
            return Err(err);
        }
    };

    let (out_elem_id, element_result) = if let Some(AssignmentOperator::Binary(op)) = operator {
        let id = *lowerer.context.next_id;
        *lowerer.context.next_id += 1;
        let name = format!("$bulk_out_elem_{id}");
        lowerer.context.bindings.locals.insert(
            name.clone(),
            Binding::local(id, out_elem_ty, false, false, location),
        );
        lowerer.temp_names.push(name.clone());
        let compound_expr = Expression {
            height: lowered_rhs.height + 2,
            location,
            kind: ExpressionKind::Binary {
                operator: op,
                left: Box::new(Expression {
                    height: 1,
                    location,
                    kind: ExpressionKind::Name(name),
                }),
                right: Box::new(lowered_rhs),
            },
        };
        let checked = crate::check::operator_methods::compound(
            &compound_expr,
            out_elem_ty,
            lowerer.context.bindings,
        );
        (Some(id), checked)
    } else {
        let checked =
            expression::coerced(&lowered_rhs, Some(out_elem_ty), lowerer.context.bindings);
        (None, checked)
    };

    let inputs = std::mem::take(&mut lowerer.inputs);
    lowerer.cleanup();

    let element_value = element_result?;
    if contains_return_handler_ir(&element_value) || mask_contains_return_handler_ir(&checked_mask)
    {
        return Err(value.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "array.bulk-handler",
                "or return is not allowed inside a bulk write element expression"
            ),
        ));
    }

    let mut lowered_inputs = Vec::with_capacity(inputs.len());
    for mut input in inputs {
        match &mut input {
            BulkInput::Array { expr, location, .. } => {
                lower_return_handler(expr, &mut context.output, context.next_id, *location);
            }
            BulkInput::Scalar { expr, .. } => {
                lower_return_handler(expr, &mut context.output, context.next_id, value.location);
            }
        }
        lowered_inputs.push(input);
    }

    context
        .bindings
        .array_bounds
        .facts
        .borrow_mut()
        .forget_shared();
    context
        .output
        .push(CheckedStatement::ArrayBulkWrite(Box::new(
            CheckedBulkWrite {
                target,
                start: None,
                end: None,
                slice: false,
                slice_proven: true,
                slice_location: location,
                inputs: lowered_inputs,
                out_elem_id,
                mask: Some(checked_mask),
                element_value,
                location,
            },
        )));
    Ok(())
}

struct Lowerer<'a, 'b> {
    context: &'a mut StatementContext<'b>,
    out_len_affine: Option<super::facts::Affine>,
    target_enum: Option<crate::ast::EnumType>,
    inputs: Vec<BulkInput>,
    temp_names: Vec<String>,
}

impl Lowerer<'_, '_> {
    fn cleanup(&mut self) {
        for name in self.temp_names.drain(..) {
            self.context.bindings.locals.remove(&name);
        }
    }

    fn lower_root(
        &mut self,
        expr: &Expression,
        expected_scalar: Option<Type>,
    ) -> Result<Expression, Diagnostic> {
        self.lower_expr(expr, false, expected_scalar)
    }

    fn lower_mask(&mut self, expr: &Expression) -> Result<CheckedMask, Diagnostic> {
        match &expr.kind {
            ExpressionKind::Group(inner) => self.lower_mask(inner),
            ExpressionKind::BitNot(inner) => {
                Ok(CheckedMask::Not(Box::new(self.lower_mask(inner)?)))
            }
            ExpressionKind::Binary {
                operator: op @ (BinaryOperator::And | BinaryOperator::Or | BinaryOperator::Xor),
                left,
                right,
            } => {
                let left = Box::new(self.lower_mask(left)?);
                let right = Box::new(self.lower_mask(right)?);
                Ok(match op {
                    BinaryOperator::And => CheckedMask::And(left, right),
                    BinaryOperator::Or => CheckedMask::Or(left, right),
                    BinaryOperator::Xor => CheckedMask::Xor(left, right),
                    _ => unreachable!(),
                })
            }
            ExpressionKind::Comparison {
                locations,
                operators,
                operands,
            } => {
                if !operands
                    .iter()
                    .any(|operand| contains_array_expr(operand, self.context.bindings))
                {
                    return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "array.bulk-mask",
                            "bulk write mask comparison requires at least one array operand"
                        ),
                    ));
                }
                let mut lowered_operands = Vec::with_capacity(operands.len());
                for operand in operands {
                    lowered_operands.push(self.lower_expr(operand, false, None)?);
                }
                let lowered = Expression {
                    height: expr.height,
                    location: expr.location,
                    kind: ExpressionKind::Comparison {
                        locations: locations.clone(),
                        operators: operators.clone(),
                        operands: lowered_operands,
                    },
                };
                let checked =
                    expression::expression(&lowered, Some(Type::Bool), self.context.bindings)?;
                Ok(CheckedMask::Leaf(Box::new(checked)))
            }
            _ => Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "array.bulk-mask",
                    "bulk write mask requires array comparisons combined with &, |, ^, or ~"
                ),
            )),
        }
    }

    fn lower_expr(
        &mut self,
        expr: &Expression,
        in_fallback: bool,
        expected_scalar: Option<Type>,
    ) -> Result<Expression, Diagnostic> {
        let has_array = contains_array_expr(expr, self.context.bindings);
        match &expr.kind {
            ExpressionKind::Group(inner) => {
                let lowered = self.lower_expr(inner, in_fallback, expected_scalar)?;
                Ok(Expression {
                    height: lowered.height + 1,
                    location: expr.location,
                    kind: ExpressionKind::Group(Box::new(lowered)),
                })
            }
            ExpressionKind::ArithmeticHandler { operand, fallback } => {
                check_no_return_handler(fallback.as_ref())?;
                if !has_array && !in_fallback {
                    return self.lower_scalar(expr, expected_scalar);
                }
                let lowered_operand = self.lower_expr(operand, in_fallback, expected_scalar)?;
                let lowered_fallback = match fallback {
                    Some(FailureHandler::Value(fb)) => Some(FailureHandler::Value(Box::new(
                        self.lower_expr(fb, true, expected_scalar)?,
                    ))),
                    other => other.clone(),
                };
                Ok(Expression {
                    height: expr.height,
                    location: expr.location,
                    kind: ExpressionKind::ArithmeticHandler {
                        operand: Box::new(lowered_operand),
                        fallback: lowered_fallback,
                    },
                })
            }
            ExpressionKind::Cast {
                operand,
                target,
                kind,
                fallback,
            } => {
                check_no_return_handler(fallback.as_ref())?;
                if !has_array && !in_fallback {
                    return self.lower_scalar(expr, expected_scalar);
                }
                let lowered_operand = self.lower_expr(operand, in_fallback, None)?;
                let lowered_fallback = match fallback {
                    Some(FailureHandler::Value(fb)) => Some(FailureHandler::Value(Box::new(
                        self.lower_expr(fb, true, Some(*target))?,
                    ))),
                    other => other.clone(),
                };
                Ok(Expression {
                    height: expr.height,
                    location: expr.location,
                    kind: ExpressionKind::Cast {
                        operand: Box::new(lowered_operand),
                        target: *target,
                        kind: *kind,
                        fallback: lowered_fallback,
                    },
                })
            }
            ExpressionKind::Binary {
                operator,
                left,
                right,
            } if has_array || in_fallback => {
                let lowered_left = self.lower_expr(left, in_fallback, None)?;
                let lowered_right = self.lower_expr(right, in_fallback, None)?;
                Ok(Expression {
                    height: 1 + lowered_left.height.max(lowered_right.height),
                    location: expr.location,
                    kind: ExpressionKind::Binary {
                        operator: *operator,
                        left: Box::new(lowered_left),
                        right: Box::new(lowered_right),
                    },
                })
            }
            ExpressionKind::Negate(inner) if has_array || in_fallback => {
                let lowered = self.lower_expr(inner, in_fallback, None)?;
                Ok(Expression {
                    height: lowered.height + 1,
                    location: expr.location,
                    kind: ExpressionKind::Negate(Box::new(lowered)),
                })
            }
            ExpressionKind::WrapNegate(inner) if has_array || in_fallback => {
                let lowered = self.lower_expr(inner, in_fallback, None)?;
                Ok(Expression {
                    height: lowered.height + 1,
                    location: expr.location,
                    kind: ExpressionKind::WrapNegate(Box::new(lowered)),
                })
            }
            ExpressionKind::SaturatingNegate(inner) if has_array || in_fallback => {
                let lowered = self.lower_expr(inner, in_fallback, None)?;
                Ok(Expression {
                    height: lowered.height + 1,
                    location: expr.location,
                    kind: ExpressionKind::SaturatingNegate(Box::new(lowered)),
                })
            }
            ExpressionKind::BitNot(inner) if has_array || in_fallback => {
                let lowered = self.lower_expr(inner, in_fallback, None)?;
                Ok(Expression {
                    height: lowered.height + 1,
                    location: expr.location,
                    kind: ExpressionKind::BitNot(Box::new(lowered)),
                })
            }
            ExpressionKind::Not(inner) if has_array || in_fallback => {
                let lowered = self.lower_expr(inner, in_fallback, None)?;
                Ok(Expression {
                    height: lowered.height + 1,
                    location: expr.location,
                    kind: ExpressionKind::Not(Box::new(lowered)),
                })
            }
            _ => {
                if has_array {
                    self.lower_array_leaf(expr)
                } else if in_fallback || is_untyped_literal_expr(expr) {
                    Ok(expr.clone())
                } else {
                    self.lower_scalar(expr, expected_scalar)
                }
            }
        }
    }

    fn lower_array_leaf(&mut self, expr: &Expression) -> Result<Expression, Diagnostic> {
        let checked = expression::expression(expr, None, self.context.bindings)?;
        let Some(array_id_ty) = checked.ty.array_id() else {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("array.bulk", "expected a 1D array in bulk write"),
            ));
        };
        if checked.ty.is_nilable() {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("array.value", "use let before reading a possibly nil array"),
            ));
        }
        let array = self
            .context
            .bindings
            .structs
            .arrays
            .borrow()
            .get(array_id_ty);
        if array.rank() != 1 {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("array.bulk", "bulk write requires 1D arrays"),
            ));
        }
        if array.first_enum() != self.target_enum {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "array.bulk",
                    "cannot mix enum-indexed arrays with integer-indexed or differently-indexed arrays in bulk write"
                ),
            ));
        }
        let length_proven = self.target_enum.is_some()
            || self.out_len_affine.as_ref().is_some_and(|out_len| {
                bounds::proves_same_length(out_len, &checked, self.context.bindings)
            });

        let array_id = *self.context.next_id;
        *self.context.next_id += 1;
        let elem_id = *self.context.next_id;
        *self.context.next_id += 1;
        let name = format!("$bulk_elem_{elem_id}");
        self.context.bindings.locals.insert(
            name.clone(),
            Binding::local(elem_id, array.element, false, false, expr.location),
        );
        self.temp_names.push(name.clone());
        self.inputs.push(BulkInput::Array {
            array_id,
            elem_id,
            expr: checked,
            length_proven,
            location: expr.location,
        });
        Ok(Expression {
            height: 1,
            location: expr.location,
            kind: ExpressionKind::Name(name),
        })
    }

    fn lower_scalar(
        &mut self,
        expr: &Expression,
        expected: Option<Type>,
    ) -> Result<Expression, Diagnostic> {
        if is_untyped_literal_expr(expr) {
            return Ok(expr.clone());
        }
        let checked = if let Some(ty) = expected {
            expression::coerced(expr, Some(ty), self.context.bindings)?
        } else {
            expression::expression(expr, None, self.context.bindings)?
        };
        let local_id = *self.context.next_id;
        *self.context.next_id += 1;
        let name = format!("$bulk_scalar_{local_id}");
        self.context.bindings.locals.insert(
            name.clone(),
            Binding::local(local_id, checked.ty, false, false, expr.location),
        );
        self.temp_names.push(name.clone());
        self.inputs.push(BulkInput::Scalar {
            local_id,
            expr: checked,
        });
        Ok(Expression {
            height: 1,
            location: expr.location,
            kind: ExpressionKind::Name(name),
        })
    }
}

fn check_no_return_handler(handler: Option<&FailureHandler>) -> Result<(), Diagnostic> {
    if let Some(FailureHandler::Return { location, .. }) = handler {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "array.bulk-handler",
                "or return is not allowed inside a bulk write element expression"
            ),
        ));
    }
    Ok(())
}

/// 配列参照を直接または要素演算の内部に含むかを判定する。
fn contains_array_expr(expr: &Expression, scope: &Scope) -> bool {
    match &expr.kind {
        ExpressionKind::Group(inner)
        | ExpressionKind::Negate(inner)
        | ExpressionKind::WrapNegate(inner)
        | ExpressionKind::SaturatingNegate(inner)
        | ExpressionKind::BitNot(inner)
        | ExpressionKind::Not(inner) => contains_array_expr(inner, scope),
        ExpressionKind::Binary { left, right, .. } => {
            contains_array_expr(left, scope) || contains_array_expr(right, scope)
        }
        ExpressionKind::Cast {
            operand, fallback, ..
        }
        | ExpressionKind::ArithmeticHandler { operand, fallback } => {
            contains_array_expr(operand, scope)
                || matches!(
                    fallback,
                    Some(FailureHandler::Value(fb)) if contains_array_expr(fb, scope)
                )
        }
        _ => expression::operand_type(expr, scope).is_some_and(|ty| ty.array_id().is_some()),
    }
}

/// 副作用がなく型未定のリテラルのみで構成される式か判定する。
fn is_untyped_literal_expr(expr: &Expression) -> bool {
    match &expr.kind {
        ExpressionKind::Integer { .. }
        | ExpressionKind::Float(_)
        | ExpressionKind::Bool(_)
        | ExpressionKind::Char(_) => true,
        ExpressionKind::Group(inner)
        | ExpressionKind::Negate(inner)
        | ExpressionKind::WrapNegate(inner)
        | ExpressionKind::SaturatingNegate(inner)
        | ExpressionKind::BitNot(inner)
        | ExpressionKind::Not(inner) => is_untyped_literal_expr(inner),
        ExpressionKind::Binary { left, right, .. } => {
            is_untyped_literal_expr(left) && is_untyped_literal_expr(right)
        }
        ExpressionKind::ArithmeticHandler { operand, fallback } => {
            is_untyped_literal_expr(operand)
                && matches!(
                    fallback,
                    Some(FailureHandler::Value(fb)) if is_untyped_literal_expr(fb)
                )
        }
        _ => false,
    }
}

fn mask_contains_return_handler_ir(mask: &CheckedMask) -> bool {
    match mask {
        CheckedMask::Leaf(expr) => contains_return_handler_ir(expr),
        CheckedMask::And(l, r) | CheckedMask::Or(l, r) | CheckedMask::Xor(l, r) => {
            mask_contains_return_handler_ir(l) || mask_contains_return_handler_ir(r)
        }
        CheckedMask::Not(inner) => mask_contains_return_handler_ir(inner),
    }
}

fn contains_return_handler_ir(expr: &CheckedExpression) -> bool {
    use crate::check::ir::{CheckedHandler, Value};
    match &expr.value {
        Value::CastReturn { .. } => true,
        Value::IntegerArithmetic {
            handler, operation, ..
        } => {
            matches!(handler, Some(CheckedHandler::Return { .. }))
                || match operation {
                    crate::check::ir::IntegerOperation::Exact(e) => {
                        let mut found = false;
                        e.visit(&mut |_| {}, &mut |child| {
                            if contains_return_handler_ir(child) {
                                found = true;
                            }
                        });
                        found
                    }
                    _ => operation.operands().any(contains_return_handler_ir),
                }
        }
        Value::ArrayRead { address, handler } => {
            matches!(handler, Some(CheckedHandler::Return { .. }))
                || contains_return_handler_ir(address)
        }
        Value::ArrayView {
            target,
            start,
            end,
            handler,
            ..
        }
        | Value::Slice {
            target,
            start,
            end,
            handler,
            ..
        } => {
            matches!(handler, Some(CheckedHandler::Return { .. }))
                || contains_return_handler_ir(target)
                || start.as_deref().is_some_and(contains_return_handler_ir)
                || end.as_deref().is_some_and(contains_return_handler_ir)
        }
        _ => {
            let mut found = false;
            expr.value.visit_constant_parts(
                &mut |child| {
                    if contains_return_handler_ir(child) {
                        found = true;
                    }
                },
                &mut |_| {},
            );
            found
        }
    }
}
