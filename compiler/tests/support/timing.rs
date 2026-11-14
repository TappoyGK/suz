use std::{
    process::{Command, Output},
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
    time::Instant,
};

#[derive(Default)]
pub(super) struct Timing {
    calls: AtomicUsize,
    micros: AtomicU64,
}
pub(super) static SUZ_TIME: Timing = Timing {
    calls: AtomicUsize::new(0),
    micros: AtomicU64::new(0),
};
pub(super) static C_TIME: Timing = Timing {
    calls: AtomicUsize::new(0),
    micros: AtomicU64::new(0),
};
pub(super) static RUN_TIME: Timing = Timing {
    calls: AtomicUsize::new(0),
    micros: AtomicU64::new(0),
};
pub(super) fn timed_output(command: &mut Command, timing: &Timing) -> std::io::Result<Output> {
    timed(timing, || command.output())
}
pub(super) fn timed<T>(timing: &Timing, action: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let output = action();
    timing.calls.fetch_add(1, Ordering::Relaxed);
    timing
        .micros
        .fetch_add(start.elapsed().as_micros() as u64, Ordering::Relaxed);
    output
}
pub fn print_timings() {
    if std::env::var_os("SUZ_TIMINGS").is_none() {
        return;
    }
    for (name, timing) in [
        ("Suz compilation", &SUZ_TIME),
        ("C compilation/linking", &C_TIME),
        ("Program execution", &RUN_TIME),
    ] {
        eprintln!(
            "{name}: {} calls, {:.3}s cumulative worker time",
            timing.calls.load(Ordering::Relaxed),
            timing.micros.load(Ordering::Relaxed) as f64 / 1_000_000.0
        );
    }
}
