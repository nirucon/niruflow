//! A parallel directory walk: worker threads share a stack of directories,
//! never follow symlinks, and keep per-thread results that the caller merges.
//! Cancellation is checked per directory and per entry.
use std::ffi::OsStr;
use std::fs::{DirEntry, FileType};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// Called with (entries seen, directory being read).
pub type ProgressFn<'a> = &'a (dyn Fn(usize, &Path) + Sync);

pub struct Opts<'a> {
    pub cancel: &'a AtomicBool,
    /// Called at most every 250 ms with (entries seen, directory).
    pub progress: Option<ProgressFn<'a>>,
    /// Directory names that are not entered.
    pub skip_dir: &'a (dyn Fn(&OsStr) -> bool + Sync),
}

pub struct Outcome<S> {
    pub states: Vec<S>,
    /// Directories that could not be read.
    pub errors: usize,
}

pub fn threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(2, 8)
}

/// Visit every non-directory entry below `roots`. `visit` returns false to
/// stop the whole walk early (a limit was reached).
pub fn walk<S, F>(roots: &[PathBuf], opts: &Opts, visit: F) -> Outcome<S>
where
    S: Default + Send,
    F: Fn(&mut S, &DirEntry, FileType) -> bool + Sync,
{
    let queue = Mutex::new((roots.to_vec(), 0usize));
    let cv = Condvar::new();
    let stop = AtomicBool::new(false);
    let errors = AtomicUsize::new(0);
    let seen = AtomicUsize::new(0);
    let last = Mutex::new(Instant::now() - Duration::from_secs(1));
    let halted = || stop.load(Ordering::Relaxed) || opts.cancel.load(Ordering::Relaxed);
    let states = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads())
            .map(|_| {
                scope.spawn(|| {
                    let mut state = S::default();
                    let mut found: Vec<PathBuf> = Vec::new();
                    let mut working = false;
                    loop {
                        let dir = {
                            let mut q = queue.lock().unwrap();
                            q.0.append(&mut found);
                            if working {
                                q.1 -= 1;
                            }
                            cv.notify_all();
                            loop {
                                if halted() {
                                    cv.notify_all();
                                    return state;
                                }
                                if let Some(d) = q.0.pop() {
                                    q.1 += 1;
                                    working = true;
                                    break d;
                                }
                                if q.1 == 0 {
                                    cv.notify_all();
                                    return state;
                                }
                                q = cv.wait(q).unwrap();
                            }
                        };
                        if let Some(p) = opts.progress {
                            if let Ok(mut t) = last.try_lock() {
                                if t.elapsed() >= Duration::from_millis(250) {
                                    *t = Instant::now();
                                    p(seen.load(Ordering::Relaxed), &dir);
                                }
                            }
                        }
                        let rd = match std::fs::read_dir(&dir) {
                            Ok(rd) => rd,
                            Err(_) => {
                                errors.fetch_add(1, Ordering::Relaxed);
                                continue;
                            }
                        };
                        for e in rd {
                            if halted() {
                                break;
                            }
                            let Ok(e) = e else { continue };
                            let Ok(ft) = e.file_type() else { continue };
                            if ft.is_dir() {
                                if !(opts.skip_dir)(&e.file_name()) {
                                    found.push(e.path());
                                }
                                continue;
                            }
                            seen.fetch_add(1, Ordering::Relaxed);
                            if !visit(&mut state, &e, ft) {
                                stop.store(true, Ordering::Relaxed);
                                break;
                            }
                        }
                    }
                })
            })
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    });
    Outcome {
        states,
        errors: errors.load(Ordering::Relaxed),
    }
}

/// Run `f` over `items` on the walk's thread count, results in item order.
pub fn par_map<T: Sync, U: Send, F: Fn(&T) -> U + Sync>(
    items: &[T],
    cancel: &AtomicBool,
    f: F,
) -> Vec<Option<U>> {
    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<U>>> = items.iter().map(|_| Mutex::new(None)).collect();
    std::thread::scope(|scope| {
        for _ in 0..threads().min(items.len().max(1)) {
            scope.spawn(|| loop {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= items.len() {
                    return;
                }
                let v = f(&items[i]);
                *slots[i].lock().unwrap() = Some(v);
            });
        }
    });
    slots.into_iter().map(|m| m.into_inner().unwrap()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_every_file_once_and_skips_names() {
        let d = std::env::temp_dir().join(format!("niruflow-walk-{}", std::process::id()));
        for sub in ["a/b/c", "a/skip", "x"] {
            std::fs::create_dir_all(d.join(sub)).unwrap();
        }
        for f in ["a/1", "a/b/2", "a/b/c/3", "a/skip/4", "x/5"] {
            std::fs::write(d.join(f), b"x").unwrap();
        }
        std::os::unix::fs::symlink(&d, d.join("x/loop")).unwrap();
        let cancel = AtomicBool::new(false);
        let skip = |n: &OsStr| n == "skip";
        let o = walk::<Vec<PathBuf>, _>(
            std::slice::from_ref(&d),
            &Opts {
                cancel: &cancel,
                progress: None,
                skip_dir: &skip,
            },
            |s, e, ft| {
                if ft.is_file() {
                    s.push(e.path());
                }
                true
            },
        );
        let mut all: Vec<_> = o.states.into_iter().flatten().collect();
        all.sort();
        assert_eq!(all.len(), 4, "{all:?}");
        std::fs::remove_dir_all(d).unwrap();
    }
}
