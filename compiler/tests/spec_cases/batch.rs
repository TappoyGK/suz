use crate::{support::batch::Batch, test_util::TemporaryDirectory};
use std::fs;

#[test]
fn batch_keeps_private_symbols_and_process_state_separate() {
    let directory = TemporaryDirectory::new();
    let sources = [
        r#"
#include <stdio.h>
static int state = 0;
static int step(void) { return ++state; }
int main(void) { puts("first"); return step(); }
"#,
        r#"
#include <stdio.h>
#include <stdlib.h>
static int state = 100;
static int step(void) { return ++state; }
static void finish(void) { fputs("finished\n", stderr); }
int main(void) { atexit(finish); exit(step()); }
"#,
    ];
    let inputs: Vec<_> = sources
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let path = directory.0.join(format!("{index}.c"));
            fs::write(&path, source).unwrap();
            (index, path)
        })
        .collect();
    let batch = Batch::build(&directory.0, &inputs, 2).unwrap();
    for index in [0, 1, 0, 1] {
        let output = batch.run(index).unwrap();
        if index == 0 {
            assert_eq!(output.status.code(), Some(1));
            assert_eq!(output.stdout, b"first\n");
            assert!(output.stderr.is_empty());
        } else {
            assert_eq!(output.status.code(), Some(101));
            assert!(output.stdout.is_empty());
            assert_eq!(output.stderr, b"finished\n");
        }
    }
}

#[test]
fn batch_preserves_compilation_failures_and_runs_remaining_cases() {
    let directory = TemporaryDirectory::new();
    let valid = directory.0.join("valid.c");
    let invalid = directory.0.join("invalid.c");
    fs::write(&valid, "int main(void) { return 42; }\n").unwrap();
    fs::write(&invalid, "#error deliberate compilation failure\n").unwrap();
    let batch = Batch::build(&directory.0, &[(0, invalid), (1, valid)], 2).unwrap();
    assert!(
        batch
            .run(0)
            .unwrap_err()
            .contains("deliberate compilation failure")
    );
    assert_eq!(batch.run(1).unwrap().status.code(), Some(42));
}
