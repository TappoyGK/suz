//! 無条件のコピーの閉路と、入力が1つだけの値は、伝播中に同じ代表ノードを使う。
use super::*;

pub(in crate::check::arena::lifetimes) fn representatives(
    analysis: &Analysis,
    constraints: &[Constraint],
) -> Vec<Node> {
    let count = analysis.targets.len();
    let mut outgoing = vec![Vec::new(); count];
    let mut incoming = vec![Vec::new(); count];
    let mut dynamic = vec![false; count];
    for constraint in constraints {
        let (dst, src) = match constraint {
            Constraint::Copy(dst, src)
            | Constraint::Load(dst, Place::Direct(src))
            | Constraint::Store(Place::Direct(dst), src, _) => (*dst, *src),
            Constraint::Load(dst, Place::Indirect(..))
            | Constraint::Address(dst, _)
            | Constraint::Limit(dst, _, _) => {
                dynamic[*dst] = true;
                continue;
            }
            _ => continue,
        };
        outgoing[src].push(dst);
        incoming[dst].push(src);
    }
    // 読み出しや一時値に入力が1つしかなければ、その値は入力と等しい。
    // 逆辺を足して同じ成分へまとめる。保存場所は間接書込みで変わるので対象外。
    // 検査専用の返却先はソースからアドレスを取れず、間接書込みもないのでまとめられる。
    // これにより、同じ変数を何度読み出して保存しても、同じコピー辺を重複して作らない。
    for dst in 0..count {
        if !dynamic[dst]
            && (!analysis.cells.contains_key(&dst)
                || analysis.returned.values().any(|&node| node == dst))
            && analysis.targets[dst].is_empty()
            && incoming[dst].len() == 1
        {
            let src = incoming[dst][0];
            outgoing[dst].push(src);
            // 逆辺を追加しながら入力数を判定すると順序依存になるので、ここではoutgoingだけを変える。
        }
    }
    for edges in &mut incoming {
        edges.clear();
    }
    for (src, edges) in outgoing.iter().enumerate() {
        for &dst in edges {
            incoming[dst].push(src);
        }
    }
    // Kosaraju法。ソースの大きさでRustのスタックを使い切らないよう反復で走査する。
    let mut visited = vec![false; count];
    let mut order = Vec::with_capacity(count);
    for start in 0..count {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let mut stack = vec![(start, 0)];
        while let Some((node, next)) = stack.last_mut() {
            if let Some(&dst) = outgoing[*node].get(*next) {
                *next += 1;
                if !visited[dst] {
                    visited[dst] = true;
                    stack.push((dst, 0));
                }
            } else {
                order.push(*node);
                stack.pop();
            }
        }
    }
    let mut representative = vec![usize::MAX; count];
    for start in order.into_iter().rev() {
        if representative[start] != usize::MAX {
            continue;
        }
        representative[start] = start;
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            for &src in &incoming[node] {
                if representative[src] == usize::MAX {
                    representative[src] = start;
                    stack.push(src);
                }
            }
        }
    }
    representative
}
