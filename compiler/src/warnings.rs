use crate::{ArithmeticMode, Diagnostic, ErrorKind, Warning, ast::Location};

pub(crate) struct AllowItem {
    pub rule: String,
    pub reason: String,
    pub location: Location,
}

pub(crate) struct AllowScope {
    pub start: Location,
    pub end: Location,
    pub items: Vec<AllowItem>,
}

fn position(location: Location) -> (usize, usize) {
    (location.line, location.column)
}

impl AllowScope {
    fn contains(&self, location: Location) -> bool {
        position(self.start) <= position(location) && position(location) < position(self.end)
    }
}

pub(crate) fn collect(
    locations: &[Location],
    scopes: &[AllowScope],
    mode: ArithmeticMode,
    log_allow: bool,
) -> Result<Vec<Warning>, Diagnostic> {
    for scope in scopes {
        for item in &scope.items {
            if scopes.iter().any(|outer| {
                !std::ptr::eq(scope, outer)
                    && outer.contains(item.location)
                    && outer.items.iter().any(|other| other.rule == item.rule)
            }) {
                return Err(item.location.error(
                    ErrorKind::InvalidSource,
                    "an enclosing allow already suppresses this rule [allow.nested]",
                ));
            }
            if !locations.iter().any(|location| scope.contains(*location)) {
                return Err(item.location.error(
                    ErrorKind::InvalidSource,
                    "allow has no matching warning sites [allow.unused]",
                ));
            }
        }
    }
    let message = "integer arithmetic has no 'or' handler [diagnostic.arithmetic]";
    if mode == ArithmeticMode::Deny
        && let Some(location) = locations.first()
    {
        return Err(location.error(ErrorKind::InvalidSource, message));
    }
    if mode != ArithmeticMode::Warn {
        return Ok(Vec::new());
    }
    Ok(locations
        .iter()
        .filter_map(|location| {
            let reason = scopes
                .iter()
                .find(|scope| scope.contains(*location))
                .map(|scope| scope.items[0].reason.clone());
            if reason.is_some() && !log_allow {
                return None;
            }
            Some(Warning {
                line: location.line,
                column: location.column,
                message: message.into(),
                allowed_reason: reason,
            })
        })
        .collect())
}
