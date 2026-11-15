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
    comparisons: &[(Location, bool)],
    divisions: &[Location],
    scopes: &[AllowScope],
    mode: ArithmeticMode,
    log_allow: bool,
) -> Result<Vec<Warning>, Diagnostic> {
    let mut candidates: Vec<_> = locations
        .iter()
        .map(|&location| {
            (
                location,
                "diagnostic.arithmetic",
                "integer arithmetic has no 'or' handler".to_string(),
                mode == ArithmeticMode::Warn,
            )
        })
        .collect();
    candidates.extend(comparisons.iter().map(|&(location, value)| {
        (
            location,
            "diagnostic.constant-comparison",
            format!("comparison is always {value}"),
            true,
        )
    }));
    candidates.extend(divisions.iter().map(|&location| {
        (
            location,
            "diagnostic.integer-division-to-float",
            "integer division truncates before floating-point arithmetic".to_string(),
            true,
        )
    }));
    candidates.sort_by_key(|(location, ..)| position(*location));
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
            if !candidates
                .iter()
                .any(|(location, rule, ..)| *rule == item.rule && scope.contains(*location))
            {
                return Err(item.location.error(
                    ErrorKind::InvalidSource,
                    "allow has no matching warning sites [allow.unused]",
                ));
            }
        }
    }
    if mode == ArithmeticMode::Deny
        && let Some(location) = locations.first()
    {
        return Err(location.error(
            ErrorKind::InvalidSource,
            "integer arithmetic has no 'or' handler [diagnostic.arithmetic]",
        ));
    }
    Ok(candidates
        .into_iter()
        .filter_map(|(location, rule, message, enabled)| {
            if !enabled {
                return None;
            }
            let reason = scopes
                .iter()
                .filter(|scope| scope.contains(location))
                .flat_map(|scope| &scope.items)
                .find(|item| item.rule == rule)
                .map(|item| item.reason.clone());
            if reason.is_some() && !log_allow {
                return None;
            }
            Some(Warning {
                line: location.line,
                column: location.column,
                message: format!("{message} [{rule}]"),
                allowed_reason: reason,
            })
        })
        .collect())
}
