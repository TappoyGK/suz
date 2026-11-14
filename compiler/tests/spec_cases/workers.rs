use crate::support;
use std::sync::atomic::Ordering;

#[test]
fn case_worker_count_is_configurable_and_validated() {
    use support::runner::worker_count;
    assert_eq!(worker_count(None).unwrap(), 8);
    assert_eq!(worker_count(Some("1")).unwrap(), 1);
    assert_eq!(worker_count(Some("8")).unwrap(), 8);
    assert_eq!(worker_count(Some("16")).unwrap(), 16);
    for invalid in [
        "",
        "0",
        "-1",
        "+2",
        "1.5",
        "eight",
        " 8",
        "999999999999999999999999999",
    ] {
        assert!(worker_count(Some(invalid)).is_err(), "{invalid}");
    }
}

#[test]
fn case_workers_bound_concurrency_and_keep_results_after_failures() {
    use std::sync::atomic::AtomicUsize;
    let jobs: Vec<_> = (0..19).collect();
    for workers in [1, 8] {
        let seen: Vec<_> = jobs.iter().map(|_| AtomicUsize::new(0)).collect();
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let results = support::runner::parallel_map(&jobs, workers, |&index| {
            seen[index].fetch_add(1, Ordering::SeqCst);
            let concurrent = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(concurrent, Ordering::SeqCst);
            // Different durations exercise result ordering independently of completion order.
            std::thread::sleep(std::time::Duration::from_millis((index % 3) as u64));
            active.fetch_sub(1, Ordering::SeqCst);
            if index == 3 {
                Err("intentional failure")
            } else {
                Ok(index)
            }
        });
        assert_eq!(results.len(), jobs.len());
        for (index, result) in results.into_iter().enumerate() {
            assert_eq!(seen[index].load(Ordering::SeqCst), 1);
            assert_eq!(
                result,
                if index == 3 {
                    Err("intentional failure")
                } else {
                    Ok(index)
                }
            );
        }
        assert!(peak.load(Ordering::SeqCst) <= workers);
        assert_eq!(active.load(Ordering::SeqCst), 0);
    }
    assert!(support::runner::parallel_map::<usize, usize>(&[], 8, |n| *n).is_empty());
    assert_eq!(support::runner::parallel_map(&[7], 8, |n| *n), vec![7]);
}
