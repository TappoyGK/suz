use std::collections::BTreeSet;

fn check(input: &[u8], case: &str, edit: &str, offset: usize, rules: &BTreeSet<&str>) {
    let result = std::panic::catch_unwind(|| suz::compile(input));
    let context = || {
        format!(
            "{case}, {edit} at byte {offset}: {:?}",
            String::from_utf8_lossy(input)
        )
    };
    let result = result.unwrap_or_else(|_| panic!("compiler panicked: {}", context()));
    if let Err(error) = result {
        assert_eq!(
            error.kind,
            suz::ErrorKind::InvalidSource,
            "{}: {error}",
            context()
        );
        assert!(error.line > 0 && error.column > 0, "{}: {error}", context());
        let rule = error
            .message()
            .rsplit_once(" [")
            .and_then(|(_, rule)| rule.strip_suffix(']'));
        assert!(
            rule.is_some_and(|rule| rules.contains(rule)),
            "{}: missing or unknown specification rule: {error}",
            context()
        );
    }
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn damaged_catalog_inputs_do_not_panic_or_lose_diagnostic_rules() {
    let rules = crate::spec_rules();
    let mut checked = 0;
    for (id, case) in crate::catalog() {
        for offset in 0..case.bytes.len() {
            // バイト削除により、コメントやテンプレート内の壊れたUTF-8も試す。
            let mut input = case.bytes.clone();
            input.remove(offset);
            check(&input, &id, "delete", offset, &rules);
            checked += 1;
            let opposite = match case.bytes[offset] {
                b'(' => b')',
                b')' => b'(',
                b'{' => b'}',
                b'}' => b'{',
                b'`' => b'"',
                b',' => b':',
                b':' => b',',
                b'\n' => b'\r',
                _ => continue,
            };
            let mut input = case.bytes.clone();
            input[offset] = opposite;
            check(&input, &id, "replace", offset, &rules);
            checked += 1;
            let mut input = case.bytes.clone();
            input.insert(offset, case.bytes[offset]);
            check(&input, &id, "duplicate", offset, &rules);
            checked += 1;
        }
    }
    eprintln!("Checked {checked} mutated inputs.");
}
