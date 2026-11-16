//! Rules accepted symbolically retain their concrete failures, in checking order.
//! Invalid bodies replay this log, never their source or their universal error.
use super::{GenericTypes, domain};
use crate::check::scope::Scope;
use crate::{
    Diagnostic,
    ast::{Location, Type},
};

pub(in crate::check) enum Deferred {
    Rule {
        failures: u16,
        diagnostic: Box<dyn Fn(usize) -> Diagnostic>,
    },
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
        let mut errors = Vec::new();
        let mut failures = 0;
        for i in domain::indices() {
            if let Some(error) = error(i) {
                failures |= 1 << i;
                errors.push((i, error));
            }
        }
        if failures != 0 {
            log.borrow_mut().push(Deferred::Rule {
                failures,
                diagnostic: Box::new(move |i| {
                    errors
                        .iter()
                        .find(|(index, _)| *index == i)
                        .unwrap()
                        .1
                        .clone()
                }),
            });
        }
    }

    // Pure predicates use a bit set. Allocate a record only for a failing
    // substitution, and format its diagnostic only if that failure is replayed.
    pub(in crate::check) fn defer_check(
        &self,
        failures: u16,
        diagnostic: impl Fn(usize) -> Diagnostic + 'static,
    ) {
        let Some(log) = &self.deferred else {
            return;
        };
        if failures != 0 {
            log.borrow_mut().push(Deferred::Rule {
                failures,
                diagnostic: Box::new(diagnostic),
            });
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    #[test]
    fn deferred_diagnostics_are_formatted_only_when_reported() {
        let scope = Scope {
            deferred: Some(Rc::new(RefCell::new(Vec::new()))),
            ..Scope::default()
        };
        scope.defer_check(0, |_| panic!("an accepted rule cannot report a diagnostic"));
        assert!(scope.deferred.as_ref().unwrap().borrow().is_empty());
        let formatted = Rc::new(Cell::new(0));
        let counter = formatted.clone();
        scope.defer_check(1 << 3, move |_| {
            counter.set(counter.get() + 1);
            Location { line: 1, column: 1 }.error(
                crate::ErrorKind::InvalidSource,
                crate::diagnostic!("type.expected", "expected i64, found u64"),
            )
        });
        assert_eq!(formatted.get(), 0);
        let log = scope.deferred.as_ref().unwrap().borrow();
        let Deferred::Rule {
            failures,
            diagnostic,
        } = &log[0]
        else {
            panic!("expected a deferred rule")
        };
        assert_eq!(*failures, 1 << 3);
        assert_eq!(diagnostic(3).rule(), "type.expected");
        assert_eq!(formatted.get(), 1);
    }
}
