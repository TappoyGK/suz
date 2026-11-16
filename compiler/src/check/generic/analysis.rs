use super::super::{functions, scope::Scope};
use crate::{
    Diagnostic,
    ast::{Function, Location},
};

// [num.fn-check], [num.fn-unused]: one traversal with symbolic numeric types.
// Each rule judges its own type function; no diagnostic intersection or recovery.
pub(crate) fn analyze(function: &Function, globals: &Scope) -> Result<Vec<Location>, Diagnostic> {
    let mut scope = globals.clone();
    scope.mode = super::super::scope::Mode::GenericCheck;
    // Only concrete emitted instances contribute runtime constant registrations.
    scope.outputs.constant_texts = Default::default();
    let (_, warnings) = functions::check_function(function, scope, 0)?;
    Ok(warnings)
}
