//! Rules accepted symbolically retain their concrete failures, in checking order.
//! Invalid bodies replay this log, never their source or their universal error.
use super::{GenericTypes, domain};
use crate::check::scope::Scope;
use crate::{
    Diagnostic,
    ast::{Location, Type},
};

pub(in crate::check) enum Deferred {
    Rule(Box<[Option<Diagnostic>; domain::NUMERIC_COMBINATIONS]>),
    Call {
        function: usize,
        types: Box<[Option<GenericTypes>; domain::NUMERIC_COMBINATIONS]>,
        location: Location,
    },
}

pub(in crate::check) struct Analysis {
    pub result: Result<Vec<Location>, Diagnostic>,
    pub deferred: Vec<Deferred>,
}

impl GenericTypes {
    pub(in crate::check) fn index(self) -> usize {
        let integer = self
            .int
            .map_or(0, |ty| domain::INTS.iter().position(|t| *t == ty).unwrap());
        integer * 2 + usize::from(self.float == Some(crate::ast::FloatType::F64))
    }
}

impl Scope {
    // Call only after the rule has passed its symbolic judgment. An error in
    // any remaining substitution is consequently a deferred judgment.
    pub(in crate::check) fn defer_rule(&self, mut error: impl FnMut(usize) -> Option<Diagnostic>) {
        let Some(log) = &self.deferred else {
            return;
        };
        let errors = std::array::from_fn(&mut error);
        if errors.iter().any(Option::is_some) {
            log.borrow_mut().push(Deferred::Rule(Box::new(errors)));
        }
    }

    pub(in crate::check) fn defer_call(
        &self,
        function: usize,
        int: Option<Type>,
        float: Option<Type>,
        location: Location,
    ) {
        let Some(log) = &self.deferred else {
            return;
        };
        let types = std::array::from_fn(|i| {
            Some(GenericTypes {
                int: match int.map(|t| domain::at(t, i)) {
                    None => None,
                    Some(Some(Type::Integer(ty))) => Some(ty),
                    _ => return None,
                },
                float: match float.map(|t| domain::at(t, i)) {
                    None => None,
                    Some(Some(Type::Float(ty))) => Some(ty),
                    _ => return None,
                },
            })
        });
        log.borrow_mut().push(Deferred::Call {
            function,
            types: Box::new(types),
            location,
        });
    }
}
