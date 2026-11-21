#[path = "spec_cases/allows.rs"]
mod allows;
#[path = "spec_cases/arena.rs"]
mod arena;
#[path = "spec_cases/arena_accumulation.rs"]
mod arena_accumulation;
#[path = "spec_cases/arena_functions.rs"]
mod arena_functions;
#[path = "spec_cases/arena_fuzz.rs"]
mod arena_fuzz;
#[path = "spec_cases/arithmetic.rs"]
mod arithmetic;
#[path = "spec_cases/arithmetic_handlers.rs"]
mod arithmetic_handlers;
#[path = "spec_cases/arrays.rs"]
mod arrays;
#[path = "spec_cases/batch.rs"]
mod batch;
#[path = "spec_cases/c_build.rs"]
mod c_build;
#[path = "spec_cases/cast_returns.rs"]
mod cast_returns;
#[path = "spec_cases/casts.rs"]
mod casts;
#[path = "spec_cases/characters.rs"]
mod characters;
#[path = "spec_cases/cli.rs"]
mod cli;
#[path = "spec_cases/closures.rs"]
mod closures;
#[path = "spec_cases/constants.rs"]
mod constants;
#[path = "spec_cases/diagnostics.rs"]
mod diagnostics;
#[path = "spec_cases/emit.rs"]
mod emit;
#[path = "spec_cases/enums.rs"]
mod enums;
#[path = "spec_cases/execution.rs"]
mod execution;
#[path = "spec_cases/expressions.rs"]
mod expressions;
#[path = "spec_cases/extra_flags.rs"]
mod extra_flags;
#[path = "spec_cases/ffi.rs"]
mod ffi;
#[path = "spec_cases/ffi_bytes.rs"]
mod ffi_bytes;
#[path = "spec_cases/ffi_link.rs"]
mod ffi_link;
#[path = "spec_cases/ffi_pointers.rs"]
mod ffi_pointers;
#[path = "spec_cases/ffi_structs.rs"]
mod ffi_structs;
#[path = "spec_cases/float_logs.rs"]
mod float_logs;
#[path = "spec_cases/floats.rs"]
mod floats;
#[path = "spec_cases/format.rs"]
mod format;
#[path = "spec_cases/function_values.rs"]
mod function_values;
#[path = "spec_cases/functions.rs"]
mod functions;
#[path = "spec_cases/generic_numeric.rs"]
mod generic_numeric;
#[path = "spec_cases/generic_properties.rs"]
mod generic_properties;
#[path = "spec_cases/globals.rs"]
mod globals;
#[path = "support/integer_bounds.rs"]
mod integer_bounds;
#[path = "spec_cases/invalid_inputs.rs"]
mod invalid_inputs;
#[path = "spec_cases/log_filter.rs"]
mod log_filter;
#[path = "spec_cases/logs.rs"]
mod logs;
#[path = "spec_cases/matches.rs"]
mod matches;
#[path = "spec_cases/methods.rs"]
mod methods;
#[path = "spec_cases/names.rs"]
mod names;
#[path = "spec_cases/native_cli.rs"]
mod native_cli;
#[path = "spec_cases/nesting.rs"]
mod nesting;
#[path = "spec_cases/nullable_pointers.rs"]
mod nullable_pointers;
#[path = "spec_cases/numeric.rs"]
mod numeric;
#[path = "spec_cases/panic.rs"]
mod panic;
#[path = "spec_cases/panics.rs"]
mod panics;
#[path = "spec_cases/ranges.rs"]
mod ranges;
#[path = "spec_cases/runtime.rs"]
mod runtime;
#[path = "spec_cases/shifts.rs"]
mod shifts;
#[path = "spec_cases/structs.rs"]
mod structs;
mod support;
#[path = "spec_cases/symbol.rs"]
mod symbol;
#[path = "spec_cases/symbol_casts.rs"]
mod symbol_casts;
#[path = "spec_cases/symbol_float_casts.rs"]
mod symbol_float_casts;
#[path = "spec_cases/symbol_runtime.rs"]
mod symbol_runtime;
#[path = "spec_cases/symbol_startup.rs"]
mod symbol_startup;
#[path = "spec_cases/symbol_struct_casts.rs"]
mod symbol_struct_casts;
#[path = "spec_cases/symbol_struct_runtime.rs"]
mod symbol_struct_runtime;
#[path = "spec_cases/syntax_diagnostics.rs"]
mod syntax_diagnostics;
#[path = "support/test_util.rs"]
mod test_util;
#[path = "spec_cases/text_views.rs"]
mod text_views;
#[path = "spec_cases/value_blocks.rs"]
mod value_blocks;
#[path = "spec_cases/workers.rs"]
mod workers;

use std::fs;
use test_util::TemporaryDirectory;

fn catalog() -> std::collections::BTreeMap<String, support::Source> {
    support::catalog::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs"))
        .unwrap()
}

fn spec_rules() -> std::collections::BTreeSet<&'static str> {
    include_str!("../../docs/spec.md")
        .lines()
        .filter_map(|line| {
            line.strip_prefix("**[")
                .and_then(|s| s.split_once("]**").map(|(id, _)| id))
        })
        .collect()
}

#[test]
fn catalog_is_complete_and_sources_round_trip() {
    let sources = catalog();
    let rules = spec_rules();
    for (id, case) in &sources {
        assert!(
            rules.contains(case.rule.as_str()),
            "{id}: unknown primary rule {}",
            case.rule
        );
    }
    for (id, case) in &sources {
        for run in &case.runs {
            for diagnostic in run
                .diagnostic
                .iter()
                .chain(&run.panic_diagnostic)
                .chain(&run.warnings)
            {
                assert!(
                    rules.contains(diagnostic.rule.as_str()),
                    "{id}: unknown diagnostic rule {}",
                    diagnostic.rule
                );
            }
        }
    }
    let directory = TemporaryDirectory::new();
    support::write_sources(&sources, &directory.0).unwrap();
    for (id, input) in &sources {
        assert_eq!(
            fs::read(directory.0.join(format!("{id}.suz"))).unwrap(),
            input.bytes,
            "{id}"
        );
    }
    assert!(
        support::write_sources(&sources, &directory.0).is_err(),
        "must not overwrite files"
    );
}

#[test]
fn execute_spec_cases() {
    run_spec_cases(false);
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn execute_optimized_spec_cases() {
    run_spec_cases(true);
}

fn run_spec_cases(optimized: bool) {
    let jobs_env = std::env::var("SUZ_JOBS")
        .map(Some)
        .unwrap_or_else(|error| match error {
            std::env::VarError::NotPresent => None,
            _ => panic!("SUZ_JOBS must be valid Unicode"),
        });
    let workers = support::runner::worker_count(jobs_env.as_deref()).unwrap();
    let mut sources = catalog();
    if let Some(selected) = std::env::var_os("SUZ_CASES") {
        let selected = selected
            .into_string()
            .expect("SUZ_CASES must be valid Unicode");
        let ids: std::collections::BTreeSet<_> = selected.split(',').map(str::trim).collect();
        for id in &ids {
            assert!(sources.contains_key(*id), "unknown case ID: {id}");
        }
        sources.retain(|id, _| ids.contains(id.as_str()));
    }
    let directory = TemporaryDirectory::new();
    let jobs: Vec<_> = sources
        .iter()
        .flat_map(|(id, case)| {
            case.runs
                .iter()
                .enumerate()
                .map(move |(index, run)| (id.as_str(), index, run))
        })
        .collect();
    assert!(!jobs.is_empty(), "no cases were selected");
    let prepared = support::runner::parallel_map(&jobs, workers, |&(id, index, run)| {
        support::runner::compile(&directory.0, id, index, run, &sources[id].bytes)
    });
    let inputs: Vec<_> = prepared
        .iter()
        .enumerate()
        .filter_map(|(index, result)| match result {
            Ok(Some(path)) => Some((index, path.clone())),
            _ => None,
        })
        .collect();
    let batch = if optimized {
        support::batch::Batch::build_optimized(&directory.0, &inputs, workers)
    } else {
        support::batch::Batch::build(&directory.0, &inputs, workers)
    };
    let indices: Vec<_> = (0..jobs.len()).collect();
    let results =
        support::runner::parallel_map(&indices, workers, |&index| match &prepared[index] {
            Err(error) => Err(error.clone()),
            Ok(None) => Ok(()),
            Ok(Some(_)) => {
                let batch = batch.as_ref().map_err(Clone::clone)?;
                let output = batch.run(index)?;
                let (id, _, run) = jobs[index];
                support::runner::check_output(
                    run,
                    output.status.code(),
                    &output.stdout,
                    &output.stderr,
                    &directory.0.join(format!("{id}.suz")),
                )
            }
        });
    let failures: Vec<_> = jobs
        .iter()
        .zip(&results)
        .filter_map(|((id, _, run), result)| {
            result
                .as_ref()
                .err()
                .map(|error| format!("{id} ({:?}): {error}", run.mode))
        })
        .collect();
    if optimized {
        eprintln!("Generated C optimization: -O2");
    }
    eprintln!(
        "Executed {} runs from {} cases with {} workers: {} passed, {} failed.",
        jobs.len(),
        sources.len(),
        workers.min(jobs.len()),
        jobs.len() - failures.len(),
        failures.len()
    );
    support::runner::print_timings();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[path = "spec_cases/length.rs"]
mod length;

#[path = "spec_cases/errors.rs"]
mod errors;

#[path = "spec_cases/log_ignore.rs"]
mod log_ignore;

#[path = "spec_cases/objection.rs"]
mod objection;

#[path = "spec_cases/operator_methods.rs"]
mod operator_methods;

#[path = "spec_cases/type_parameters.rs"]
mod type_parameters;

#[path = "spec_cases/generic_structs.rs"]
mod generic_structs;

#[path = "spec_cases/generic_function_values.rs"]
mod generic_function_values;
#[path = "spec_cases/need_groups.rs"]
mod need_groups;
