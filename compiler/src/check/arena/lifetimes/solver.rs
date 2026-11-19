//! 候補の増分を、その値を読む場所だけへ伝える。
//! 間接参照の参照先が増えたときはコピー辺を追加し、それ以降の更新も追跡する。
use super::{Analysis, Constraint, Node, Place};
use std::collections::{BTreeSet, VecDeque};
mod components;

#[derive(Default)]
struct Worklist {
    representative: Vec<Node>,
    // srcの候補が増えると、copies[src]にある各ノードへ同じ増分を渡す。
    copies: Vec<BTreeSet<Node>>,
    // ポインタの候補から保存場所を解決する必要がある制約の番号。
    indirect: Vec<Vec<usize>>,
    pending: Vec<Vec<Node>>,
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

    fn add(&mut self, analysis: &mut Analysis<'_>, dst: Node, values: &[Node]) {
        self.resize(analysis.targets.len());
        let dst = self.representative[dst];
        for &value in values {
            #[cfg(test)]
            {
                self.propagated += 1;
            }
            // arena fnの効果はR内だけに限定する。R外の入力は別の候補集合に残して
            // 呼び出しを拒否し、不正な呼び出しの効果で合法な呼び出しを汚染しない。
            if analysis
                .call_regions
                .get(&dst)
                .is_some_and(|region| analysis.cells[&value].region != *region)
            {
                continue;
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

    fn copy(&mut self, analysis: &mut Analysis<'_>, dst: Node, src: Node) {
        self.resize(analysis.targets.len());
        let dst = self.representative[dst];
        let src = self.representative[src];
        if dst == src || !self.copies[src].insert(dst) {
            return;
        }
        // 遅れて見つかった参照先にも、既に確定した候補を一度だけ渡す。
        // 辺の登録後はpendingの増分だけが流れる。
        let known: Vec<_> = analysis.targets[src].iter().copied().collect();
        self.add(analysis, dst, &known);
    }

    fn connect(&mut self, analysis: &mut Analysis<'_>, constraint: &Constraint, cell: Node) {
        match constraint {
            Constraint::Copy(_, _) => unreachable!("copy has no indirect place"),
            Constraint::Load(dst, _) => self.copy(analysis, *dst, cell),
            Constraint::Store(_, src, _) => self.copy(analysis, cell, *src),
            Constraint::Address(dst, _) => self.add(analysis, *dst, &[cell]),
        }
    }

    fn run(&mut self, analysis: &mut Analysis<'_>, constraints: &[Constraint]) {
        self.representative = components::representatives(analysis, constraints);
        self.resize(analysis.targets.len());
        for node in 0..analysis.targets.len() {
            let representative = self.representative[node];
            if representative != node {
                let initial = std::mem::take(&mut analysis.targets[node]);
                analysis.targets[representative].extend(initial);
            }
        }
        // newが作った初期候補をキューへ載せる。nullには候補がない。
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
                    self.copy(analysis, *dst, *src);
                    continue;
                }
                Constraint::Load(_, place)
                | Constraint::Address(_, place)
                | Constraint::Store(place, _, _) => place,
            };
            match place {
                Place::Direct(cell) => self.connect(analysis, constraint, *cell),
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
            for dst in destinations {
                self.add(analysis, dst, &added);
            }
            for index in 0..self.indirect[node].len() {
                let constraint = &constraints[self.indirect[node][index]];
                let place = match constraint {
                    Constraint::Load(_, place)
                    | Constraint::Address(_, place)
                    | Constraint::Store(place, _, _) => place,
                    Constraint::Copy(_, _) => {
                        unreachable!("only indirect constraints are registered")
                    }
                };
                let Place::Indirect(_, path) = place else {
                    unreachable!()
                };
                // 新しい参照先だけを解決し、Load/Storeを通常のコピー辺へ変換する。
                // 保存先が後から増える場合と、保存する値が後から増える場合の両方を扱う。
                for &target in &added {
                    let cell = analysis.project(target, path);
                    self.connect(analysis, constraint, cell);
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

pub(super) fn propagate(analysis: &mut Analysis<'_>, constraints: &[Constraint]) {
    Worklist::default().run(analysis, constraints);
}

#[cfg(test)]
mod tests;
