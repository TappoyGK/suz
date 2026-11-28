//! ポインタの段数と各段のnil許容性・読み取り専用性。通常の型まで大きくして検査を遅くしないため、
//! 31段までは1語に収め、それを超えた場合だけ不変の形状を共通化する。

use std::collections::BTreeMap;
use std::sync::{OnceLock, RwLock};

/// 下位ビットから2ビットずつ（偶数ビットがnil許容性、奇数ビットが読み取り専用性）、
/// その上の1が段数の目印。最上位ビットが1なら深い形状の表番号。浅い型は表もロックも使わない。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PointerShape(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct DeepShape {
    depth: usize,
    nilable: [u64; 4],
    readonly: [u64; 4],
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
const INLINE_MAX_DEPTH: usize = 31;

impl PointerShape {
    pub fn new() -> Self {
        Self(1 << 2)
    }

    pub fn depth(self) -> usize {
        if self.0 & TABLE == 0 {
            ((63 - self.0.leading_zeros()) / 2) as usize
        } else {
            self.expanded().depth
        }
    }

    fn expanded(self) -> DeepShape {
        if self.0 & TABLE == 0 {
            let depth = self.depth();
            let payload = self.0 ^ (1 << (2 * depth));
            let mut nilable = 0u64;
            let mut readonly = 0u64;
            for i in 0..depth {
                if payload & (1 << (2 * i)) != 0 {
                    nilable |= 1 << i;
                }
                if payload & (1 << (2 * i + 1)) != 0 {
                    readonly |= 1 << i;
                }
            }
            DeepShape {
                depth,
                nilable: [nilable, 0, 0, 0],
                readonly: [readonly, 0, 0, 0],
            }
        } else {
            shapes().read().unwrap().values[(self.0 ^ TABLE) as usize]
        }
    }

    fn from_expanded(value: DeepShape) -> Self {
        if value.depth <= INLINE_MAX_DEPTH {
            let mut bits = 1u64 << (2 * value.depth);
            for i in 0..value.depth {
                if value.nilable[0] & (1 << i) != 0 {
                    bits |= 1 << (2 * i);
                }
                if value.readonly[0] & (1 << i) != 0 {
                    bits |= 1 << (2 * i + 1);
                }
            }
            return Self(bits);
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

    pub fn is_nilable(self) -> bool {
        if self.0 & TABLE == 0 {
            return self.0 & (1 << (2 * (self.depth() - 1))) != 0;
        }
        let value = self.expanded();
        value.depth <= 256
            && value.nilable[(value.depth - 1) / 64] & (1 << ((value.depth - 1) % 64)) != 0
    }

    pub fn is_readonly(self) -> bool {
        if self.0 & TABLE == 0 {
            return self.0 & (1 << (2 * (self.depth() - 1) + 1)) != 0;
        }
        let value = self.expanded();
        value.depth <= 256
            && value.readonly[(value.depth - 1) / 64] & (1 << ((value.depth - 1) % 64)) != 0
    }

    pub fn with_nilable(self, enabled: bool) -> Self {
        if self.0 & TABLE == 0 {
            let bit = 1 << (2 * (self.depth() - 1));
            return Self(if enabled { self.0 | bit } else { self.0 & !bit });
        }
        let mut value = self.expanded();
        assert!(
            value.depth <= 256 || !enabled,
            "nilable pointer nesting is checked before construction"
        );
        if value.depth <= 256 {
            let bit = 1 << ((value.depth - 1) % 64);
            if enabled {
                value.nilable[(value.depth - 1) / 64] |= bit;
            } else {
                value.nilable[(value.depth - 1) / 64] &= !bit;
            }
        }
        Self::from_expanded(value)
    }

    pub fn with_readonly(self, enabled: bool) -> Self {
        if self.0 & TABLE == 0 {
            let bit = 1 << (2 * (self.depth() - 1) + 1);
            return Self(if enabled { self.0 | bit } else { self.0 & !bit });
        }
        let mut value = self.expanded();
        assert!(
            value.depth <= 256 || !enabled,
            "readonly pointer nesting is checked before construction"
        );
        if value.depth <= 256 {
            let bit = 1 << ((value.depth - 1) % 64);
            if enabled {
                value.readonly[(value.depth - 1) / 64] |= bit;
            } else {
                value.readonly[(value.depth - 1) / 64] &= !bit;
            }
        }
        Self::from_expanded(value)
    }

    pub fn push(self) -> Self {
        if self.0 & TABLE == 0 && self.depth() < INLINE_MAX_DEPTH {
            let sentinel = 1 << (2 * self.depth());
            return Self((self.0 ^ sentinel) | (sentinel << 2));
        }
        let mut value = self.expanded();
        // 推論した257段は検査器が診断するまで一時的に保持する。
        value.depth += 1;
        Self::from_expanded(value)
    }

    pub fn pop(self) -> Self {
        let shape = self.with_nilable(false).with_readonly(false);
        if shape.0 & TABLE == 0 {
            let sentinel = 1 << (2 * shape.depth());
            return Self((shape.0 ^ sentinel) | (sentinel >> 2));
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
    fn different_nilable_and_readonly_layers_remain_distinct_across_storage_boundaries() {
        // 段は末端から数える。表現が切り替わる31/32段でも、違う段のnil許容性・読み取り専用性を混同しない。
        for depth in [30, 31, 32, 33, 64, 128, 256] {
            let mut layers = vec![0, 1, 31, 32, depth]; // 0は全段が非nil・可変の形。
            layers.retain(|&layer| layer <= depth);
            layers.sort_unstable();
            layers.dedup();
            let shapes: Vec<_> = layers
                .iter()
                .flat_map(|&marked_layer| {
                    [false, true].into_iter().map(move |readonly_only| {
                        let mut shape = PointerShape::new();
                        for layer in 1..=depth {
                            if layer > 1 {
                                shape = shape.push();
                            }
                            if layer == marked_layer {
                                shape = shape.with_nilable(!readonly_only);
                                shape = shape.with_readonly(readonly_only);
                            }
                        }
                        (marked_layer, readonly_only, shape)
                    })
                })
                .filter(|&(layer, readonly_only, _)| layer != 0 || !readonly_only)
                .collect();
            for (i, &(layer_i, ro_i, shape)) in shapes.iter().enumerate() {
                for &(layer_j, ro_j, other) in shapes.iter().skip(i + 1) {
                    assert_ne!(
                        shape, other,
                        "depth={depth}, ({layer_i}, ro={ro_i}) and ({layer_j}, ro={ro_j})"
                    );
                }
            }
        }
    }

    #[test]
    fn nilability_and_readonly_survive_deep_pointer_round_trips() {
        let mut shape = PointerShape::new();
        let mut levels = Vec::new();
        for depth in 1..=256 {
            shape = shape
                .with_nilable(depth % 3 == 0)
                .with_readonly(depth % 5 == 0);
            levels.push(shape);
            shape = shape.push();
        }
        assert_eq!(shape.depth(), 257);
        for expected in levels.into_iter().rev() {
            shape = shape.pop();
            assert_eq!(shape, expected);
            assert_eq!(shape.is_nilable(), shape.depth().is_multiple_of(3));
            assert_eq!(shape.is_readonly(), shape.depth().is_multiple_of(5));
            assert_eq!(
                shape.with_nilable(true).with_nilable(false),
                shape.with_nilable(false)
            );
            assert_eq!(
                shape.with_readonly(true).with_readonly(false),
                shape.with_readonly(false)
            );
        }
    }
}
