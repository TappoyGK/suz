mod policy;
mod validation;

use super::{
    expression,
    scope::{Scope, Signature},
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, FloatType, IntegerType, Location, Type},
};
pub(super) use policy::analyze;
use std::collections::{BTreeMap, VecDeque};
pub(super) use validation::validate;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct GenericTypes {
    pub int: Option<IntegerType>,
    pub float: Option<FloatType>,
}
impl GenericTypes {
    pub fn resolve(self, ty: Type) -> Type {
        match ty {
            Type::GenericInt => Type::Integer(self.int.expect("int declared in signature")),
            Type::GenericFloat => Type::Float(self.float.expect("float declared in signature")),
            _ => ty,
        }
    }
    pub fn is_empty(self) -> bool {
        self.int.is_none() && self.float.is_none()
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    Int,
    Float,
}
impl Family {
    fn of(ty: Type) -> Option<Self> {
        match ty {
            Type::GenericInt => Some(Self::Int),
            Type::GenericFloat => Some(Self::Float),
            _ => None,
        }
    }
    fn contains(self, ty: Type) -> bool {
        matches!(
            (self, ty),
            (Self::Int, Type::Integer(_)) | (Self::Float, Type::Float(_))
        )
    }
}

#[derive(Clone)]
pub(super) struct Instance {
    pub function: usize,
    pub types: GenericTypes,
    pub id: usize,
    pub calls: Vec<Location>,
}
#[derive(Default, Clone)]
pub(super) struct Instances {
    ids: BTreeMap<(usize, GenericTypes), usize>,
    pub queue: VecDeque<Instance>,
    next_id: usize,
}
impl Instances {
    // [num.monomorphize] Template IDs occupy the initial range; generated
    // instances have distinct IDs and identical substitutions share one ID.
    pub fn new(first_id: usize) -> Self {
        Self {
            next_id: first_id,
            ..Self::default()
        }
    }
    pub fn request(
        &mut self,
        function: usize,
        types: GenericTypes,
        mut calls: Vec<Location>,
        call: Location,
    ) -> usize {
        let key = (function, types);
        if let Some(id) = self.ids.get(&key) {
            return *id;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.ids.insert(key, id);
        calls.push(call);
        self.queue.push_back(Instance {
            function,
            types,
            id,
            calls,
        });
        id
    }
}

fn without_groups(mut expr: &Expression) -> &Expression {
    while let ExpressionKind::Group(inner) = &expr.kind {
        expr = inner;
    }
    expr
}
fn candidates(
    signature: &Signature,
    arguments: &[Expression],
    family: Family,
    scope: &Scope,
    into: &mut Vec<Type>,
) {
    for (parameter, argument) in signature.parameters.iter().zip(arguments) {
        if Family::of(*parameter) != Some(family) {
            continue;
        }
        let argument = without_groups(argument);
        if let ExpressionKind::Call { name, arguments } = &argument.kind
            && let Some(inner) = scope.functions.get(name)
            && Family::of(inner.result) == Some(family)
        {
            // [num.fn-nested] Collect the connected calls before applying defaults.
            candidates(inner, arguments, family, scope, into);
            continue;
        }
        if matches!(
            argument.kind,
            ExpressionKind::Integer { .. } | ExpressionKind::Float(_)
        ) {
            continue;
        }
        let known = expression::operand_type(argument, scope).or_else(|| {
            super::exact::is_operator(argument).then_some(
                if expression::has_float_literal(argument, scope) {
                    Type::Float(FloatType::F64)
                } else {
                    Type::Integer(IntegerType::I64)
                },
            )
        });
        if let Some(ty) = known
            && family.contains(ty)
        {
            into.push(ty);
        }
    }
}
pub(super) fn resolve(
    signature: &Signature,
    arguments: &[Expression],
    expected: Option<Type>,
    scope: &Scope,
    location: Location,
) -> Result<GenericTypes, Diagnostic> {
    let mut result = GenericTypes::default();
    for family in [Family::Int, Family::Float] {
        if !signature
            .parameters
            .iter()
            .chain([&signature.result])
            .any(|ty| Family::of(*ty) == Some(family))
        {
            continue;
        }
        let mut types = Vec::new();
        candidates(signature, arguments, family, scope, &mut types);
        if Family::of(signature.result) == Some(family)
            && let Some(ty) = expected
            && family.contains(ty)
        {
            types.push(ty);
        }
        let mut joined = None;
        for ty in types {
            joined = Some(match joined {
                None => ty,
                Some(from) => super::exact::join(from, ty).ok_or_else(|| {
                    location.error(
                        ErrorKind::InvalidSource,
                        "generic arguments have no common numeric type [num.fn-resolve]",
                    )
                })?,
            });
        }
        match family {
            Family::Int => {
                result.int = Some(match joined {
                    Some(Type::Integer(t)) => t,
                    None => IntegerType::I64,
                    _ => unreachable!("integer candidates only"),
                });
            }
            Family::Float => {
                result.float = Some(match joined {
                    Some(Type::Float(t)) => t,
                    None => FloatType::F64,
                    _ => unreachable!("float candidates only"),
                });
            }
        }
    }
    Ok(result)
}
pub(super) fn call_operand_type(
    name: &str,
    arguments: &[Expression],
    scope: &Scope,
) -> Option<Type> {
    let signature = scope.functions.get(name)?;
    let Some(family) = Family::of(signature.result) else {
        return Some(signature.result);
    };
    let mut types = Vec::new();
    candidates(signature, arguments, family, scope, &mut types);
    expression::joined_type(types.into_iter().map(Some))
}
