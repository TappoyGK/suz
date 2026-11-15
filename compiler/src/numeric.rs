//! Exact integers used by range analysis and constant evaluation. The sign is
//! separate so unsigned 128-bit values never pass through a signed conversion.
use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Int {
    pub negative: bool,
    pub magnitude: u128,
}
impl Int {
    pub const ZERO: Self = Self {
        negative: false,
        magnitude: 0,
    };
    pub fn new(negative: bool, magnitude: u128) -> Self {
        Self {
            negative: negative && magnitude != 0,
            magnitude,
        }
    }
    pub fn from_i128(value: i128) -> Self {
        Self::new(value < 0, value.unsigned_abs())
    }
    pub fn to_i128(self) -> Option<i128> {
        if self.negative && self.magnitude == 1u128 << 127 {
            Some(i128::MIN)
        } else {
            i128::try_from(self.magnitude)
                .ok()
                .map(|n| if self.negative { -n } else { n })
        }
    }
    pub fn neg(self) -> Self {
        Self::new(!self.negative, self.magnitude)
    }
    pub fn add(self, b: Self) -> Option<Self> {
        if self.negative == b.negative {
            Some(Self::new(
                self.negative,
                self.magnitude.checked_add(b.magnitude)?,
            ))
        } else if self.magnitude >= b.magnitude {
            Some(Self::new(self.negative, self.magnitude - b.magnitude))
        } else {
            Some(Self::new(b.negative, b.magnitude - self.magnitude))
        }
    }
    pub fn sub(self, b: Self) -> Option<Self> {
        self.add(b.neg())
    }
    pub fn mul(self, b: Self) -> Option<Self> {
        Some(Self::new(
            self.negative != b.negative,
            self.magnitude.checked_mul(b.magnitude)?,
        ))
    }
    pub fn div(self, b: Self) -> Option<Self> {
        Some(Self::new(
            self.negative != b.negative,
            self.magnitude.checked_div(b.magnitude)?,
        ))
    }
    pub fn rem(self, b: Self) -> Option<Self> {
        Some(Self::new(
            self.negative,
            self.magnitude.checked_rem(b.magnitude)?,
        ))
    }
    pub fn float(self, single: bool) -> f64 {
        let value = if single {
            (self.magnitude as f32) as f64
        } else {
            self.magnitude as f64
        };
        if self.negative { -value } else { value }
    }
    pub fn compare_float(self, b: f64) -> Option<Ordering> {
        if b.is_nan() {
            return None;
        }
        if b == f64::INFINITY {
            return Some(Ordering::Less);
        }
        if b == f64::NEG_INFINITY {
            return Some(Ordering::Greater);
        }
        let negative = b < 0.0;
        if self.negative != negative {
            return Some(if self.negative {
                Ordering::Less
            } else {
                Ordering::Greater
            });
        }
        let bits = b.abs().to_bits();
        let exponent = ((bits >> 52) & 2047) as i32;
        let mantissa = (bits & ((1u64 << 52) - 1)) | if exponent == 0 { 0 } else { 1u64 << 52 };
        let shift = if exponent == 0 {
            -1074
        } else {
            exponent - 1023 - 52
        };
        let order = if shift >= 0 {
            let width = 64 - mantissa.leading_zeros() + shift as u32;
            if width > 128 {
                Ordering::Less
            } else {
                self.magnitude.cmp(&((mantissa as u128) << shift))
            }
        } else {
            let shift = (-shift) as u32;
            let (integer, fraction) = if shift >= 64 {
                (0, mantissa != 0)
            } else {
                (mantissa >> shift, mantissa & ((1u64 << shift) - 1) != 0)
            };
            match self.magnitude.cmp(&(integer as u128)) {
                Ordering::Equal if fraction => Ordering::Less,
                order => order,
            }
        };
        Some(if negative { order.reverse() } else { order })
    }
}
impl Ord for Int {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.negative, other.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (true, true) => other.magnitude.cmp(&self.magnitude),
            _ => self.magnitude.cmp(&other.magnitude),
        }
    }
}
impl PartialOrd for Int {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl std::fmt::Display for Int {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.negative {
            write!(f, "-")?;
        }
        write!(f, "{}", self.magnitude)
    }
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Range {
    pub min: Int,
    pub max: Int,
}
impl Range {
    pub fn point(value: Int) -> Self {
        Self {
            min: value,
            max: value,
        }
    }
    pub fn bounds(min: i128, max: i128) -> Self {
        Self {
            min: Int::from_i128(min),
            max: Int::from_i128(max),
        }
    }
    pub fn fits(self) -> bool {
        !self.min.negative
            || (self.min.magnitude <= 1u128 << 127 && self.max <= Int::from_i128(i128::MAX))
    }
    pub fn contained(self, min: i128, max: i128) -> bool {
        self.min >= Int::from_i128(min) && self.max <= Int::from_i128(max)
    }
    pub fn fits_type(self, ty: crate::ast::Type) -> bool {
        ty.integer_bounds().is_some_and(|(lo, hi)| {
            self.contained(lo, hi)
                && (ty != crate::ast::Type::Char
                    || self.max < Int::from_i128(0xd800)
                    || self.min > Int::from_i128(0xdfff))
        })
    }
    pub fn exact_float(self, single: bool) -> bool {
        if self.min == self.max {
            let n = self.min.magnitude;
            n == 0 || 128 - n.leading_zeros() - n.trailing_zeros() <= if single { 24 } else { 53 }
        } else {
            let limit = 1i128 << if single { 24 } else { 53 };
            self.contained(-limit, limit)
        }
    }
    pub fn binary(self, op: crate::ast::BinaryOperator, b: Self) -> Result<Option<Self>, ()> {
        use crate::ast::BinaryOperator::*;
        if matches!(op, Divide | Remainder) && b.min == Int::ZERO && b.max == Int::ZERO {
            return Ok(None);
        }
        let mut values = Vec::new();
        if matches!(op, Remainder) {
            let m = b.min.magnitude.max(b.max.magnitude) - 1;
            return Ok(Some(Self {
                min: self.min.min(Int::ZERO).max(Int::new(true, m)),
                max: self.max.max(Int::ZERO).min(Int::new(false, m)),
            }));
        }
        let mut rights = vec![b.min, b.max];
        if matches!(op, Divide) {
            rights.retain(|n| *n != Int::ZERO);
            if b.min < Int::ZERO && b.max >= Int::ZERO {
                rights.push(Int::from_i128(-1));
            }
            if b.max > Int::ZERO && b.min <= Int::ZERO {
                rights.push(Int::from_i128(1));
            }
        }
        for a in [self.min, self.max] {
            for b in &rights {
                values.push(
                    match op {
                        Add => a.add(*b),
                        Subtract => a.sub(*b),
                        Multiply => a.mul(*b),
                        Divide => a.div(*b),
                        _ => unreachable!(),
                    }
                    .ok_or(())?,
                );
            }
        }
        let range = Self {
            min: *values.iter().min().unwrap(),
            max: *values.iter().max().unwrap(),
        };
        if range.fits() {
            Ok(Some(range))
        } else {
            Err(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::BinaryOperator;

    #[test]
    fn interval_estimates_enclose_every_small_integer_result() {
        for lo in -3..=3 {
            for hi in lo..=3 {
                for divisor_lo in -3..=3 {
                    for divisor_hi in divisor_lo..=3 {
                        for op in [
                            BinaryOperator::Add,
                            BinaryOperator::Subtract,
                            BinaryOperator::Multiply,
                            BinaryOperator::Divide,
                            BinaryOperator::Remainder,
                        ] {
                            let range = Range::bounds(lo, hi)
                                .binary(op, Range::bounds(divisor_lo, divisor_hi))
                                .unwrap();
                            let mut actual = Vec::new();
                            for a in lo..=hi {
                                for b in divisor_lo..=divisor_hi {
                                    let n = match op {
                                        BinaryOperator::Add => Some(a + b),
                                        BinaryOperator::Subtract => Some(a - b),
                                        BinaryOperator::Multiply => Some(a * b),
                                        BinaryOperator::Divide => a.checked_div(b),
                                        BinaryOperator::Remainder => a.checked_rem(b),
                                        _ => unreachable!(),
                                    };
                                    if let Some(n) = n {
                                        actual.push(Int::from_i128(n));
                                    }
                                }
                            }
                            assert_eq!(range.is_none(), actual.is_empty());
                            if let Some(range) = range {
                                assert!(actual.iter().all(|n| range.min <= *n && *n <= range.max));
                                if !matches!(op, BinaryOperator::Remainder) {
                                    assert_eq!(range.min, *actual.iter().min().unwrap());
                                    assert_eq!(range.max, *actual.iter().max().unwrap());
                                }
                            }
                        }
                    }
                }
            }
        }
        let u64_range = Range::bounds(0, u64::MAX as i128);
        let product = u64_range
            .binary(BinaryOperator::Multiply, u64_range)
            .unwrap()
            .unwrap();
        assert_eq!(product.max.magnitude, (u64::MAX as u128).pow(2));
        assert!(product.binary(BinaryOperator::Multiply, u64_range).is_err());
        assert!(
            !Range {
                min: Int::from_i128(-1),
                max: Int::new(false, u128::MAX)
            }
            .fits()
        );
        assert!(Range::point(Int::from_i128(i128::MIN)).fits());
    }
}
