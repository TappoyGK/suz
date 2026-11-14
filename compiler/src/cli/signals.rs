use std::{
    io,
    process::{Command, ExitStatus},
    sync::atomic::{AtomicI32, Ordering},
};

static CHILD: AtomicI32 = AtomicI32::new(0);
static PENDING: AtomicI32 = AtomicI32::new(0);
const FORWARDED: [i32; 4] = [1, 2, 3, 15]; // HUP, INT, QUIT, TERM on supported Unix targets.
unsafe extern "C" {
    fn signal(number: i32, handler: usize) -> usize;
    fn kill(pid: i32, number: i32) -> i32;
}

extern "C" fn forward(number: i32) {
    PENDING.store(number, Ordering::SeqCst);
    let pid = CHILD.load(Ordering::SeqCst);
    if pid > 0 {
        unsafe {
            kill(pid, number);
        }
    }
}

struct Handlers(Vec<(i32, usize)>);
impl Drop for Handlers {
    fn drop(&mut self) {
        CHILD.store(0, Ordering::SeqCst);
        for &(number, previous) in &self.0 {
            unsafe {
                signal(number, previous);
            }
        }
        PENDING.store(0, Ordering::SeqCst);
    }
}

pub fn execute(command: &mut Command) -> io::Result<ExitStatus> {
    use std::os::unix::process::CommandExt;
    let mut handlers = Handlers(Vec::new());
    for number in FORWARDED {
        let previous = unsafe { signal(number, forward as *const () as usize) };
        if previous == usize::MAX {
            return Err(io::Error::last_os_error());
        }
        handlers.0.push((number, previous));
    }
    unsafe {
        command.pre_exec(|| {
            for number in FORWARDED {
                if signal(number, 0) == usize::MAX {
                    return Err(io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    let mut child = command.spawn()?;
    CHILD.store(child.id() as i32, Ordering::SeqCst);
    let pending = PENDING.load(Ordering::SeqCst);
    if pending != 0 {
        unsafe {
            kill(child.id() as i32, pending);
        }
    }
    let result = child.wait();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

pub fn terminate(number: i32) -> ! {
    unsafe {
        signal(number, 0);
        kill(std::process::id() as i32, number);
    }
    std::process::exit(128 + number);
}
