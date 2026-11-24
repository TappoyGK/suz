//! 候補の増分を、その値を読む場所だけへ伝える。
//! 間接参照の参照先が増えたときはコピー辺を追加し、それ以降の更新も追跡する。
use super::{Analysis, Constraint, Node, Place, Reference};
use std::collections::{BTreeSet, VecDeque};
mod components;
pub(super) use components::{partition, representatives};

#[derive(Default)]
struct Worklist {
    representative: Vec<Node>,
    // srcの候補が増えると、copies[src]にある各ノードへ同じ増分を渡す。
    copies: Vec<BTreeSet<(Node, Option<usize>)>>,
    // ポインタの候補から保存場所を解決する必要がある制約の番号。
    indirect: Vec<Vec<usize>>,
    pending: Vec<Vec<Reference>>,
    queued: Vec<bool>,
    queue: VecDeque<Node>,
    #[cfg(test)]
    propagated: usize,
}

impl Worklist {
    fn resize(&mut self, count: usize) {
        // 間接参照で後から作るフィールドは独立したノードとして追加する。
        self.representative.extend(self.representative.len()..count);
        self.copies.resize_with(count, BTreeSet::new);
        self.indirect.resize_with(count, Vec::new);
        self.pending.resize_with(count, Vec::new);
        self.queued.resize(count, false);
    }

    fn add(&mut self, analysis: &mut Analysis, dst: Node, values: &[Reference]) {
        self.resize(analysis.targets.len());
        let dst = self.representative[dst];
        for &value in values {
            #[cfg(test)]
            {
                self.propagated += 1;
            }
            if analysis.targets[dst].insert(value) {
                self.pending[dst].push(value);
            }
        }
        if !self.pending[dst].is_empty() && !self.queued[dst] {
            self.queued[dst] = true;
            self.queue.push_back(dst);
        }
    }

    fn transfer(
        &mut self,
        analysis: &mut Analysis,
        dst: Node,
        values: &[Reference],
        limit: Option<usize>,
    ) {
        let Some(limit) = limit else {
            self.add(analysis, dst, values);
            return;
        };
        let mut limited = Vec::with_capacity(values.len());
        for &value in values {
            // 静的textは関数を通ってもプログラム終了まで有効。
            if matches!(
                analysis.cells[&value.target].origin,
                super::Origin::Permanent
            ) {
                limited.push(value);
                continue;
            }
            // 実体と同じ期間でも制限は残す。その実体のフィールドから長命な参照を
            // 読み出したときにも、戻り値から引き継いだ利用期間を適用するため。
            if let Some(previous) = value.limit {
                if analysis.outlives(limit, previous) {
                    limited.push(value);
                    continue;
                }
                // 比較できない2つの期間は両方残す。全候補について保存を検査するため、
                // 後から付けた制限が前の制限を上書きして寿命を延ばすことはない。
                if !analysis.outlives(previous, limit) {
                    limited.push(value);
                }
            }
            limited.push(Reference {
                limit: Some(limit),
                ..value
            });
        }
        self.add(analysis, dst, &limited);
    }

    fn copy(&mut self, analysis: &mut Analysis, dst: Node, src: Node, limit: Option<usize>) {
        self.resize(analysis.targets.len());
        let dst = self.representative[dst];
        let src = self.representative[src];
        if (dst == src && limit.is_none()) || !self.copies[src].insert((dst, limit)) {
            return;
        }
        // 遅れて見つかった参照先にも、既に確定した候補を一度だけ渡す。
        // 辺の登録後はpendingの増分だけが流れる。
        let known: Vec<_> = analysis.targets[src].iter().copied().collect();
        self.transfer(analysis, dst, &known, limit);
    }

    fn connect(&mut self, analysis: &mut Analysis, constraint: &Constraint, reference: Reference) {
        let cell = reference.target;
        match constraint {
            Constraint::Copy(..) | Constraint::Limit(..) => {
                unreachable!("copy has no indirect place")
            }
            Constraint::Load(dst, _) => self.copy(analysis, *dst, cell, reference.limit),
            Constraint::Store(_, src, _) => self.copy(analysis, cell, *src, None),
            Constraint::Address(dst, _) => self.add(analysis, *dst, &[reference]),
        }
    }

    fn run(&mut self, analysis: &mut Analysis, constraints: &[Constraint]) {
        self.run_with(
            analysis,
            constraints,
            components::representatives(analysis, constraints),
        );
    }

    fn run_with(
        &mut self,
        analysis: &mut Analysis,
        constraints: &[Constraint],
        representative: Vec<Node>,
    ) {
        self.representative = representative;
        self.resize(analysis.targets.len());
        for node in 0..analysis.targets.len() {
            let representative = self.representative[node];
            if representative != node {
                let initial = std::mem::take(&mut analysis.targets[node]);
                analysis.targets[representative].extend(initial);
            }
        }
        // newが作った初期候補をキューへ載せる。nilには候補がない。
        for (node, targets) in analysis.targets.iter().enumerate() {
            if !targets.is_empty() {
                self.pending[node].extend(targets);
                self.queued[node] = true;
                self.queue.push_back(node);
            }
        }
        for (index, constraint) in constraints.iter().enumerate() {
            let place = match constraint {
                Constraint::Copy(dst, src) => {
                    self.copy(analysis, *dst, *src, None);
                    continue;
                }
                Constraint::Limit(dst, src, limit) => {
                    self.copy(analysis, *dst, *src, Some(*limit));
                    continue;
                }
                Constraint::Load(_, place)
                | Constraint::Address(_, place)
                | Constraint::Store(place, _, _) => place,
            };
            match place {
                Place::Direct(cell) => self.connect(analysis, constraint, Reference::object(*cell)),
                Place::Indirect(pointer, _) => {
                    self.indirect[self.representative[*pointer]].push(index)
                }
            }
        }
        while let Some(node) = self.queue.pop_front() {
            self.queued[node] = false;
            let added = std::mem::take(&mut self.pending[node]);
            // 候補集合を写さず、コピー先の番号だけを一時保存する。
            let destinations: Vec<_> = self.copies[node].iter().copied().collect();
            for (dst, limit) in destinations {
                self.transfer(analysis, dst, &added, limit);
            }
            for index in 0..self.indirect[node].len() {
                let constraint = &constraints[self.indirect[node][index]];
                let place = match constraint {
                    Constraint::Load(_, place)
                    | Constraint::Address(_, place)
                    | Constraint::Store(place, _, _) => place,
                    Constraint::Copy(..) | Constraint::Limit(..) => {
                        unreachable!("only indirect constraints are registered")
                    }
                };
                let Place::Indirect(_, path) = place else {
                    unreachable!()
                };
                // 新しい参照先だけを解決し、Load/Storeを通常のコピー辺へ変換する。
                // 保存先が後から増える場合と、保存する値が後から増える場合の両方を扱う。
                for &reference in &added {
                    let target = analysis.project(reference.target, path);
                    self.connect(
                        analysis,
                        constraint,
                        Reference {
                            target,
                            ..reference
                        },
                    );
                }
            }
        }
        // 保存位置や診断は元のノードを使う。収束した集合を戻して解析の外へ影響を出さない。
        for node in 0..analysis.targets.len() {
            let representative = self.representative[node];
            if representative != node {
                analysis.targets[node] = analysis.targets[representative].clone();
            }
        }
    }
}

pub(super) fn propagate(analysis: &mut Analysis, constraints: &[Constraint]) {
    Worklist::default().run(analysis, constraints);
}

// 呼び出しの入力を正規化した代表を再利用する。追加した効果用ノードは独立で、
// 元の無条件コピーが保証する等値関係は、効果を追加しても変わらない。
pub(super) fn propagate_with(
    analysis: &mut Analysis,
    constraints: &[Constraint],
    representative: Vec<Node>,
) {
    Worklist::default().run_with(analysis, constraints, representative);
}

#[cfg(test)]
mod tests;
