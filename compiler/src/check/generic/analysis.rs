use super::super::{functions, scope::Scope};
use crate::ast::Function;

// [num.fn-check], [num.fn-unused]: one traversal with symbolic numeric types.
// Each rule judges its own type function; no diagnostic intersection or recovery.
pub(crate) fn analyze(function: &Function, globals: &Scope) -> super::Analysis {
    let mut scope = globals.clone();
    scope.mode = super::super::scope::Mode::GenericCheck;
    // Only concrete emitted instances contribute runtime constant registrations.
    scope.outputs.constant_texts = Default::default();
    let deferred = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    scope.deferred = Some(deferred.clone());
    let result = functions::check_function(function, scope, 0).map(|(_, warnings)| warnings);
    let deferred = if result.is_err() {
        deferred.take()
    } else {
        Vec::new()
    };
    super::Analysis { result, deferred }
}
