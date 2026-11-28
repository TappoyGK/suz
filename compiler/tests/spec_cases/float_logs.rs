use crate::{
    support::float_runtime,
    test_util::{TemporaryDirectory, compile_and_run_c23},
};
use std::{
    fmt::Write as _,
    fs,
    path::Path,
    process::{Command, Stdio},
};

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn float_interpolations_preserve_precision_and_evaluate_before_output() {
    let source = r#"fn mark(value: f32) -> f32 {
    log info "mark ${value}"
    return value
}
fn main() -> u8 {
    fix small: f32 = 0.3
    fix precise = small as f64
    fix marked = mark(small)
    fix negative_zero = -0.0
    fix negative_inf = -inf
    fix not_number = nan
    fix infinity = inf
    log info "${marked} ${precise} ${negative_zero} ${not_number} ${infinity} ${negative_inf}"
    return 0
}
"#;
    let output = compile_and_run_c23(source, "float.suz");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "info float.suz:2: mark 0.3\ninfo float.suz:13: 0.3 0.30000001192092896 -0.0 nan inf -inf\n"
    );
}

// 固定表記・科学表記やSuzで必須の「.0」に依存せず、有効桁数を比較する。
// 最短表記の基準にはRustのフォーマッターを使う。
fn significant_digits(text: &str) -> usize {
    let mantissa = text.split(['e', 'E']).next().unwrap();
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    digits.trim_matches('0').len().max(1)
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn float_logs_round_trip_and_obey_exact_boundaries_without_undefined_behavior() {
    let directory = TemporaryDirectory::new();
    let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime");
    let executable = directory.0.join("float-logs");
    let mut command = Command::new(std::env::var_os("CC").unwrap_or_else(|| "clang".into()));
    command
        .args([
            "-std=c23",
            "-pedantic-errors",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-O2",
            "-DRYU_ONLY_64_BIT_OPS",
            "-fsanitize=address,undefined",
            "-fno-sanitize-recover=all",
            "-I",
        ])
        .arg(&runtime)
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/float_logs.c"));
    float_runtime::link(&mut command, &runtime);
    let built = command.arg("-o").arg(&executable).output().unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );

    // 10^-6の一つ下の値は、最短表記が「1e-6」でも科学表記にする。10^21自体はbinary64で正確に表現できる。
    let expected: &[(u32, u64, &str)] = &[
        (64, 0, "0.0"),
        (64, 0x8000000000000000, "-0.0"),
        (64, 0x7ff8000000000001, "nan"),
        (64, 0xfff8000000000001, "nan"),
        (64, 0x7ff0000000000000, "inf"),
        (64, 0xfff0000000000000, "-inf"),
        (64, 1, "5.0e-324"),
        (64, 0x7fefffffffffffff, "1.7976931348623157e308"),
        (64, 0x3eb0c6f7a0b5ed8c, "9.999999999999997e-7"),
        (64, 0x3eb0c6f7a0b5ed8d, "1.0e-6"),
        (64, 0x3eb0c6f7a0b5ed8e, "0.0000010000000000000002"),
        (64, 0x444b1ae4d6e2ef4f, "999999999999999900000.0"),
        (64, 0x444b1ae4d6e2ef50, "1.0e21"),
        (64, 0x444b1ae4d6e2ef51, "1.0000000000000001e21"),
        (32, 0, "0.0"),
        (32, 0x80000000, "-0.0"),
        (32, 0x7fc00001, "nan"),
        (32, 0xffc00001, "nan"),
        (32, 0x7f800000, "inf"),
        (32, 0xff800000, "-inf"),
        (32, 1, "1.0e-45"),
        (32, 0x7f7fffff, "3.4028235e38"),
        (32, 0x358637bc, "9.999999e-7"),
        (32, 0x358637bd, "1.0e-6"),
        (32, 0x358637be, "0.0000010000001"),
        (32, 0x6258d726, "999999950000000000000.0"),
        (32, 0x6258d727, "1.0e21"),
        (32, 0x6258d728, "1.0000001e21"),
    ];
    let mut values: Vec<_> = expected
        .iter()
        .map(|&(width, bits, _)| (width, bits))
        .collect();
    // 正規化数・非正規化数と2のべき乗の境界で全指数を試し、
    // その後、仮数と符号を全ビット範囲から再現可能な方法で抽出する。
    for (width, mantissa, exponents) in [(32, 23, 255), (64, 52, 2047)] {
        for exponent in 1..exponents {
            for offset in [-1_i64, 0, 1] {
                let bits = ((exponent as u64) << mantissa).wrapping_add_signed(offset);
                values.push((width, bits));
                values.push((width, bits | (1 << (width - 1))));
            }
        }
    }
    let mut state = 0x123456789abcdef0_u64;
    for _ in 0..2048 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        values.push((64, state));
        values.push((32, state & 0xffffffff));
    }
    let mut input = String::new();
    for (width, bits) in &values {
        writeln!(input, "{width} {bits:x}").unwrap();
    }
    let input_path = directory.0.join("input");
    fs::write(&input_path, input).unwrap();
    let output = Command::new(&executable)
        .stdin(fs::File::open(&input_path).unwrap())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), values.len());
    for ((_, _, expected), actual) in expected.iter().zip(&lines) {
        assert_eq!(expected, actual);
    }
    for ((width, bits), text) in values.iter().zip(&lines) {
        let (value, reference) = if *width == 32 {
            let value = f32::from_bits(*bits as u32);
            if !value.is_nan() {
                assert_eq!(
                    text.parse::<f32>().unwrap().to_bits(),
                    *bits as u32,
                    "{text}"
                );
            }
            (f64::from(value), value.to_string())
        } else {
            let value = f64::from_bits(*bits);
            if !value.is_nan() {
                assert_eq!(text.parse::<f64>().unwrap().to_bits(), *bits, "{text}");
            }
            (value, value.to_string())
        };
        if value.is_nan() {
            assert_eq!(*text, "nan");
            continue;
        }
        if value.is_infinite() {
            assert_eq!(*text, if value < 0.0 { "-inf" } else { "inf" });
            continue;
        }
        assert_eq!(
            significant_digits(text),
            significant_digits(&reference),
            "{width}-bit {bits:x}: {text} versus {reference}"
        );
        let magnitude = value.abs();
        assert_eq!(
            text.contains('e'),
            magnitude >= 1e21
                || (magnitude > 0.0 && magnitude <= f64::from_bits(0x3eb0c6f7a0b5ed8d)),
            "{text}"
        );
        let mantissa = text.split('e').next().unwrap().trim_start_matches('-');
        let (whole, fractional) = mantissa.split_once('.').unwrap();
        assert!(!whole.is_empty() && !fractional.is_empty(), "{text}");
    }
    // 小数点記号がコンマのものも含め、利用可能な全ロケールで同じバイト列を出す。
    // 規格に従うホストではCロケールは必ず利用できる。
    let locales = Command::new("locale").arg("-a").output().unwrap();
    assert!(locales.status.success());
    for locale in String::from_utf8(locales.stdout)
        .unwrap()
        .lines()
        .filter(|name| *name == "C" || name.starts_with("de_DE") || name.starts_with("fr_FR"))
    {
        let output = Command::new(&executable)
            .arg(locale)
            .stdin(Stdio::from(fs::File::open(&input_path).unwrap()))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "locale {locale}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, text.as_bytes(), "locale {locale}");
    }
}
