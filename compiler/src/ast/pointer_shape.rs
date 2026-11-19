//! ポインタの段数と各段のnull許容性。通常の型まで大きくして検査を遅くしないため、
//! 62段までは1語に収め、それを超えた場合だけ不変の形状を共通化する。

use std::collections::BTreeMap;
use std::sync::{OnceLock, RwLock};

/// 下位ビットが末端側のnull許容性、その上の1が段数の目印。
/// 最上位ビットが1なら深い形状の表番号。浅い型は表もロックも使わない。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PointerShape(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct DeepShape {
    depth: usize,
    nullable: [u64; 4],
}

#[derive(Default)]
struct Shapes {
    values: Vec<DeepShape>,
    indices: BTreeMap<DeepShape, u64>,
}

// 型番号やソースへの参照は含めない。形状だけをプロセス内で共通化するので、
// 並列のコンパイルや以前のコンパイルから保持されたTypeも同じ番号を使える。
fn shapes() -> &'static RwLock<Shapes> {
    static SHAPES: OnceLock<RwLock<Shapes>> = OnceLock::new();
    SHAPES.get_or_init(|| RwLock::new(Shapes::default()))
}

const TABLE: u64 = 1 << 63;
impl PointerShape {
    pub fn new() -> Self {
        Self(2)
    }

    pub fn depth(self) -> usize {
        if self.0 & TABLE == 0 {
            (63 - self.0.leading_zeros()) as usize
        } else {
            self.expanded().depth
        }
    }

    fn expanded(self) -> DeepShape {
        if self.0 & TABLE == 0 {
            let depth = self.depth();
            DeepShape {
                depth,
                nullable: [self.0 ^ (1 << depth), 0, 0, 0],
            }
        } else {
            shapes().read().unwrap().values[(self.0 ^ TABLE) as usize]
        }
    }

    fn from_expanded(value: DeepShape) -> Self {
        if value.depth <= 62 {
            return Self(value.nullable[0] | (1 << value.depth));
        }
        if let Some(index) = shapes().read().unwrap().indices.get(&value) {
            return Self(TABLE | index);
        }
        let mut shapes = shapes().write().unwrap();
        // 読み取りロックを離した間に別のスレッドが追加していても番号を重複させない。
        if let Some(index) = shapes.indices.get(&value) {
            return Self(TABLE | index);
        }
        let index = shapes.values.len() as u64;
        shapes.values.push(value);
        shapes.indices.insert(value, index);
        Self(TABLE | index)
    }

    pub fn is_nullable(self) -> bool {
        if self.0 & TABLE == 0 {
            return self.0 & (1 << (self.depth() - 1)) != 0;
        }
        let value = self.expanded();
        value.depth <= 256
            && value.nullable[(value.depth - 1) / 64] & (1 << ((value.depth - 1) % 64)) != 0
    }

    pub fn with_nullable(self, enabled: bool) -> Self {
        if self.0 & TABLE == 0 {
            let bit = 1 << (self.depth() - 1);
            return Self(if enabled { self.0 | bit } else { self.0 & !bit });
        }
        let mut value = self.expanded();
        assert!(
            value.depth <= 256 || !enabled,
            "nullable pointer nesting is checked before construction"
        );
        if value.depth <= 256 {
            let bit = 1 << ((value.depth - 1) % 64);
            if enabled {
                value.nullable[(value.depth - 1) / 64] |= bit;
            } else {
                value.nullable[(value.depth - 1) / 64] &= !bit;
            }
        }
        Self::from_expanded(value)
    }

    pub fn push(self) -> Self {
        if self.0 & TABLE == 0 && self.depth() < 62 {
            let sentinel = 1 << self.depth();
            return Self((self.0 ^ sentinel) | (sentinel << 1));
        }
        let mut value = self.expanded();
        // 推論した257段は検査器が診断するまで一時的に保持する。
        value.depth += 1;
        Self::from_expanded(value)
    }

    pub fn pop(self) -> Self {
        let shape = self.with_nullable(false);
        if shape.0 & TABLE == 0 {
            let sentinel = 1 << shape.depth();
            return Self((shape.0 ^ sentinel) | (sentinel >> 1));
        }
        let mut value = shape.expanded();
        value.depth -= 1;
        Self::from_expanded(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nullability_survives_deep_pointer_round_trips() {
        let mut shape = PointerShape::new();
        let mut levels = Vec::new();
        for depth in 1..=256 {
            shape = shape.with_nullable(depth % 3 == 0);
            levels.push(shape);
            shape = shape.push();
        }
        assert_eq!(shape.depth(), 257);
        for expected in levels.into_iter().rev() {
            shape = shape.pop();
            assert_eq!(shape, expected);
            assert_eq!(shape.is_nullable(), shape.depth().is_multiple_of(3));
            assert_eq!(
                shape.with_nullable(true).with_nullable(false),
                shape.with_nullable(false)
            );
        }
    }
}
