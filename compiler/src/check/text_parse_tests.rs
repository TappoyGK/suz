use super::parse;
use crate::ast::{FloatType, Type};
use crate::check::ir::Value;

fn check(text: &str) {
    let clean = text.replace('_', "");
    for ty in [FloatType::F32, FloatType::F64] {
        let expected = match ty {
            FloatType::F32 => clean.parse::<f32>().unwrap() as f64,
            FloatType::F64 => clean.parse::<f64>().unwrap(),
        };
        let parsed = parse(text, Type::Float(ty), false);
        if expected.is_infinite() && !matches!(clean.as_str(), "inf" | "-inf") {
            assert!(parsed.is_none(), "{ty:?}: {text}");
            continue;
        }
        let Value::Float(bits) = parsed.unwrap_or_else(|| panic!("{ty:?}: {text}")).value else {
            panic!("wrong parsed value type");
        };
        let actual = match ty {
            FloatType::F32 => f32::from_bits(bits as u32) as f64,
            FloatType::F64 => f64::from_bits(bits),
        };
        if expected.is_nan() {
            assert!(actual.is_nan(), "{ty:?}: {text}");
        } else {
            assert_eq!(actual.to_bits(), expected.to_bits(), "{ty:?}: {text}");
        }
    }
}

// Exact 2^-power written in decimal, including values below f64's range.
fn negative_power_of_two(power: usize) -> String {
    let mut digits = vec![1_u8];
    for _ in 0..power {
        let mut carry = 0;
        for digit in &mut digits {
            let product = *digit * 5 + carry;
            *digit = product % 10;
            carry = product / 10;
        }
        if carry != 0 {
            digits.push(carry);
        }
    }
    format!(
        "0.{}{}",
        "0".repeat(power - digits.len()),
        digits
            .into_iter()
            .rev()
            .map(|n| char::from(b'0' + n))
            .collect::<String>()
    )
}

#[test]
fn decimal_float_parsing_matches_independent_rounding_at_boundaries() {
    for text in [
        "0",
        "-0",
        "0.0",
        "-0.0",
        "1",
        "1_000",
        "1.2_5e+1",
        "1.0e-3_2",
        "nan",
        "-nan",
        "inf",
        "-inf",
        "16777217",
        "16777219",
        "9007199254740993",
        "9007199254740995",
        "1.0000000596046447753906249",
        "1.000000059604644775390625",
        "1.0000000596046447753906250000000000001",
        "1.000000178813934326171875",
        "1.00000000000000011102230246251565404236316680908203125",
        "3.40282346638528859811704183484516925440e38",
        "3.40282356779733661637539395458142568447e38",
        "3.40282356779733661637539395458142568448e38",
        "1.17549435082228750796873653722224568e-38",
        "1.7976931348623157e308",
        "1.7976931348623158e308",
        "1.7976931348623159e308",
        "2.2250738585072011e-308",
        "2.2250738585072012e-308",
        "2.2250738585072014e-308",
        "4.9406564584124654e-324",
        "2.4703282292062327e-324",
        "2.4703282292062328e-324",
        "1.0e18446744073709551615",
        "1.0e18446744073709551616",
        "1.0e-18446744073709551616",
        "1.0e999999999999999999999999999999999",
        "0.0e999999999999999999999999999999999",
        "-0.0e-999999999999999999999999999999999",
    ] {
        check(text);
    }
    for power in [126, 149, 150, 1022, 1074, 1075] {
        let exact = negative_power_of_two(power);
        check(&exact);
        check(&format!("-{exact}"));
        // Discarded digits must still distinguish an exact tie from above it.
        check(&format!("{exact}{}1", "0".repeat(1000)));
        check(&format!("-{exact}{}1", "0".repeat(1000)));
    }
    check(&format!("1.000000059604644775390625{}1", "0".repeat(1000)));
    check(&format!("1{}.0e-2000", "0".repeat(2000)));
    check(&format!("0.{}1e+2001", "0".repeat(2000)));
}

#[test]
fn decimal_float_parsing_requires_complete_suz_syntax() {
    for text in [
        "",
        "-",
        "+1",
        " 1",
        "1 ",
        "1\0",
        "1_",
        "_1",
        "1__0",
        "-_1",
        "1e2",
        ".1",
        "1.",
        "1._0",
        "1_.0",
        "1.0e",
        "1.0e+",
        "1.0e_1",
        "1.0e1_",
        "1.0e1__0",
        "1.0E1",
        "0x1p0",
        "NaN",
        "INF",
        "infinity",
        "nan(1)",
        "1,0",
        "１.０",
        "1.0e999999999999999999999999999999999x",
        "0.0e999999999999999999999999999999999x",
    ] {
        for ty in [FloatType::F32, FloatType::F64] {
            assert!(
                parse(text, Type::Float(ty), false).is_none(),
                "{ty:?}: {text:?}"
            );
        }
    }
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn decimal_float_parsing_matches_independent_parser_for_long_generated_inputs() {
    let mut state = 0xa51f_ef27_41de_8a69_u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..4096 {
        let count = (next() % 1200) as usize;
        let exponent = next() as i32 % 400;
        let mut input = if next() & 1 == 0 {
            String::new()
        } else {
            String::from("-")
        };
        input.push(char::from(b'1' + (next() % 9) as u8));
        input.push('.');
        for _ in 0..=count {
            input.push(char::from(b'0' + (next() % 10) as u8));
        }
        input.push_str(&format!("e{exponent}"));
        check(&input);
    }
}
