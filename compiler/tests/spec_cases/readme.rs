use crate::test_util::compile_and_run_c23_batch;

#[test]
fn readme_suz_examples_build_and_run() {
    // 文書から直接取り出すことで、掲載例だけが古くなったり壊れたりする変更を検出する。
    let sources = include_str!("../../../README.md")
        .split("```suz\n")
        .skip(1)
        .map(|block| block.split_once("\n```").expect("unclosed Suz example").0)
        .collect::<Vec<_>>();
    let expected: &[&[&str]] = &[&["30", "10"], &["Hello, Suz!"]];
    // 例の追加・削除時にも、実行結果の確認を忘れないようにする。
    assert_eq!(
        sources.len(),
        expected.len(),
        "README example count changed"
    );
    let names = (1..=sources.len())
        .map(|index| format!("README-example-{index}.suz"))
        .collect::<Vec<_>>();
    let inputs = sources
        .iter()
        .zip(&names)
        .map(|(source, name)| (*source, name.as_str()))
        .collect::<Vec<_>>();
    for ((output, expected), name) in compile_and_run_c23_batch(&inputs)
        .into_iter()
        .zip(expected)
        .zip(&names)
    {
        assert_eq!(output.status.code(), Some(0), "{name}: {output:?}");
        assert!(output.stdout.is_empty(), "{name}: {output:?}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        // 説明用コメントの増減による行番号の変化では失敗させず、ログの内容を確認する。
        let messages = stderr
            .lines()
            .map(|line| line.split_once(": ").expect("expected log output").1)
            .collect::<Vec<_>>();
        assert_eq!(messages, *expected, "{name}: {stderr}");
    }
}
