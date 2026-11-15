use super::{ExpressionContext, expressions::expression};
use crate::{
    ast::Type,
    check::{CheckedExpression, Value},
};

pub(super) fn parts(
    expr: &CheckedExpression,
    context: &mut ExpressionContext,
) -> (String, String, String) {
    let Value::Slice {
        target,
        start,
        end,
        index,
        ..
    } = &expr.value
    else {
        unreachable!()
    };
    context.uses_text = true;
    let source_id = context.temporaries.len();
    context.temporaries.push(Type::Text);
    let source = format!("suz_cmp_{source_id}");
    let mut setup = vec![format!("{source} = {}", expression(target, context))];
    let mut valid = Vec::new();
    let mut bounds = Vec::new();
    for (bound, default) in [(start, "0".into()), (end, format!("{source}.scalars"))] {
        if let Some(bound) = bound {
            let id = context.temporaries.len();
            context.temporaries.push(bound.ty);
            setup.push(format!("suz_cmp_{id} = {}", expression(bound, context)));
            if bound.ty.integer_bounds().unwrap().0 < 0 {
                valid.push(format!("suz_cmp_{id} >= 0"));
            }
            bounds.push(format!("(uint64_t)suz_cmp_{id}"));
        } else {
            bounds.push(default);
        }
    }
    let output = context.temporaries.len();
    context.temporaries.push(expr.ty);
    valid.push(if *index {
        format!("suz_text_index({source}, {}, &suz_cmp_{output})", bounds[0])
    } else {
        format!(
            "suz_text_slice({source}, {}, {}, &suz_cmp_{output})",
            bounds[0], bounds[1]
        )
    });
    (
        setup.join(", "),
        valid.join(" && "),
        format!("suz_cmp_{output}"),
    )
}
