//! Read-only media inventory (audio, video, photos) with a parallel walk.
use crate::walk::{self, Opts};
use serde_json::{json, Map, Value};
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub const KINDS: [&str; 3] = ["Audio", "Video", "Photos"];

pub fn kind_of(ext: &str) -> Option<usize> {
    match ext {
        ".mp3" | ".flac" | ".ogg" | ".opus" | ".wav" | ".m4a" | ".aac" | ".wma" | ".aiff"
        | ".alac" | ".ape" => Some(0),
        ".mp4" | ".mkv" | ".mov" | ".avi" | ".webm" | ".m4v" | ".wmv" | ".mpeg" | ".mpg"
        | ".ts" => Some(1),
        ".jpg" | ".jpeg" | ".png" | ".webp" | ".gif" | ".tif" | ".tiff" | ".bmp" | ".heic"
        | ".heif" | ".avif" | ".raw" | ".cr2" | ".nef" | ".arw" | ".dng" => Some(2),
        _ => None,
    }
}

const EXCLUDED: [&str; 11] = [
    ".git",
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
    ".cache",
    ".next",
    ".local",
    ".config",
    ".npm",
    "backups",
];

pub fn default_roots() -> Vec<String> {
    let h = crate::config::home();
    ["Music", "Videos", "Pictures"]
        .iter()
        .map(|n| h.join(n).to_string_lossy().into_owned())
        .collect()
}

#[derive(Clone, Debug, Default)]
pub struct KindStats {
    pub count: u64,
    pub bytes: u64,
    /// Extension counts in first-seen order.
    pub extensions: Vec<(String, u64)>,
}

#[derive(Clone, Debug, Default)]
pub struct Inventory {
    pub types: [KindStats; 3],
    pub total: u64,
    pub bytes: u64,
    pub errors: u64,
    pub duration_s: f64,
    pub truncated: bool,
}

impl Inventory {
    pub fn to_json(&self) -> Value {
        let mut types = Map::new();
        for (i, k) in KINDS.iter().enumerate() {
            let t = &self.types[i];
            let mut ext = Map::new();
            for (e, n) in &t.extensions {
                ext.insert(e.clone(), json!(n));
            }
            types.insert(
                (*k).into(),
                json!({"count": t.count, "bytes": t.bytes, "extensions": Value::Object(ext)}),
            );
        }
        json!({"types": Value::Object(types), "total": self.total, "bytes": self.bytes,
               "errors": self.errors, "duration_s": crate::json::num(self.duration_s),
               "truncated": self.truncated})
    }
}

fn add_ext(v: &mut Vec<(String, u64)>, ext: &str, n: u64) {
    match v.iter_mut().find(|(e, _)| e == ext) {
        Some((_, c)) => *c += n,
        None => v.push((ext.to_string(), n)),
    }
}

/// Real directories only; hidden and cache folders and nested duplicate
/// roots are skipped.
pub fn inventory(roots: &[String], max_files: u64, cancel: &AtomicBool) -> Inventory {
    let started = std::time::Instant::now();
    let mut inv = Inventory::default();
    let mut seen: Vec<PathBuf> = Vec::new();
    let skip = |n: &OsStr| {
        n.to_str()
            .is_none_or(|s| s.starts_with('.') || EXCLUDED.contains(&s))
    };
    let total = AtomicUsize::new(0);
    let stat_errors = AtomicUsize::new(0);
    for raw in roots {
        let root = PathBuf::from(crate::config::expand_user(raw));
        let link = std::fs::symlink_metadata(&root).is_ok_and(|m| m.file_type().is_symlink());
        if !root.is_dir() || link {
            inv.errors += 1;
            continue;
        }
        let root = std::fs::canonicalize(&root).unwrap_or(root);
        if seen.iter().any(|p| root == *p || root.starts_with(p)) {
            continue;
        }
        seen.push(root.clone());
        let opts = Opts {
            cancel,
            progress: None,
            skip_dir: &skip,
        };
        #[derive(Default)]
        struct S {
            types: [KindStats; 3],
        }
        let out = walk::walk::<S, _>(&[root], &opts, |s, e, ft| {
            if ft.is_symlink() {
                return true;
            }
            let name = e.file_name();
            let name = name.to_string_lossy();
            let ext = crate::safety::suffix(&name).to_lowercase();
            let Some(k) = kind_of(&ext) else { return true };
            let md = match std::fs::metadata(e.path()) {
                Ok(m) => m,
                Err(_) => {
                    stat_errors.fetch_add(1, Ordering::Relaxed);
                    return true;
                }
            };
            if !md.is_file() {
                return true;
            }
            let t = &mut s.types[k];
            t.count += 1;
            t.bytes += md.len();
            add_ext(&mut t.extensions, &ext, 1);
            (total.fetch_add(1, Ordering::Relaxed) as u64 + 1) < max_files
        });
        inv.errors += out.errors as u64;
        for s in out.states {
            for (i, t) in s.types.into_iter().enumerate() {
                let d = &mut inv.types[i];
                d.count += t.count;
                d.bytes += t.bytes;
                for (e, n) in t.extensions {
                    add_ext(&mut d.extensions, &e, n);
                }
            }
        }
        if total.load(Ordering::Relaxed) as u64 >= max_files {
            inv.truncated = true;
            break;
        }
    }
    inv.errors += stat_errors.load(Ordering::Relaxed) as u64;
    inv.total = inv.types.iter().map(|t| t.count).sum();
    inv.bytes = inv.types.iter().map(|t| t.bytes).sum();
    inv.duration_s = (started.elapsed().as_secs_f64() * 100.0).round() / 100.0;
    inv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_skips() {
        let d = std::env::temp_dir().join(format!("niruflow-media-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join(".git")).unwrap();
        std::fs::write(d.join("song.FLAC"), b"abc").unwrap();
        std::fs::write(d.join("photo.jpg"), b"abcde").unwrap();
        std::fs::write(d.join("video.mp4"), b"ab").unwrap();
        std::fs::write(d.join("ignored.txt"), b"abcdef").unwrap();
        std::fs::write(d.join(".git/hidden.mp3"), b"abc").unwrap();
        std::os::unix::fs::symlink(d.join("song.FLAC"), d.join("link.mp3")).unwrap();
        let no = AtomicBool::new(false);
        let r = inventory(&[d.to_string_lossy().into_owned()], 1_000_000, &no);
        assert_eq!(r.total, 3);
        assert_eq!(r.bytes, 10);
        assert_eq!(
            [r.types[0].count, r.types[1].count, r.types[2].count],
            [1, 1, 1]
        );
        assert_eq!(r.types[0].extensions, vec![(".flac".to_string(), 1)]);
        std::fs::remove_dir_all(d).unwrap();
    }
}
