//! Storage review: largest files, duplicates and forgotten (long unchanged)
//! files with a parallel walk, plus the stat- and content-verified
//! quarantine and permanent deletion of explicitly selected files.
use crate::config::R;
use crate::safety::{
    ctime_ns, fingerprint, fingerprint_full, ftime, is_synced, safe_file, validate_root,
};
use crate::walk::{self, Opts};
use serde_json::{json, Map, Value};
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::ffi::OsStr;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub const SKIP: [&str; 9] = [
    ".git",
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
    ".cache",
    ".local",
    ".config",
    ".next",
];

pub const FORGOTTEN_EXCLUDED: [&str; 14] = [
    ".cache",
    ".local",
    ".config",
    ".git",
    ".ssh",
    ".gnupg",
    ".mozilla",
    ".thunderbird",
    ".var",
    "node_modules",
    "venv",
    ".venv",
    "proc",
    "sys",
];

/// One scanned file and the identity a later action must find unchanged.
#[derive(Clone, Debug, Default)]
pub struct FileRow {
    pub path: String,
    pub bytes: u64,
    pub mtime: f64,
    pub device: u64,
    pub inode: u64,
    pub ctime_ns: i64,
    pub fingerprint: Option<String>,
    pub fingerprint_full: Option<bool>,
    pub modified: Option<String>,
}

impl FileRow {
    pub fn of(path: String, md: &std::fs::Metadata) -> Self {
        FileRow {
            path,
            bytes: md.size(),
            mtime: ftime(md.mtime(), md.mtime_nsec()),
            device: md.dev(),
            inode: md.ino(),
            ctime_ns: ctime_ns(md),
            ..Default::default()
        }
    }

    /// The key order of NIRUFLOW 0.x (`large-files`, `forgotten`).
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("path".into(), json!(self.path));
        m.insert("bytes".into(), json!(self.bytes));
        m.insert("mtime".into(), crate::json::num(self.mtime));
        if let Some(t) = &self.modified {
            m.insert("modified".into(), json!(t));
        }
        m.insert("device".into(), json!(self.device));
        m.insert("inode".into(), json!(self.inode));
        m.insert("ctime_ns".into(), json!(self.ctime_ns));
        if let Some(f) = &self.fingerprint {
            m.insert("fingerprint".into(), json!(f));
        }
        if let Some(f) = self.fingerprint_full {
            m.insert("fingerprint_full".into(), json!(f));
        }
        Value::Object(m)
    }

    fn fingerprinted(mut self) -> Option<Self> {
        let p = Path::new(&self.path);
        self.fingerprint = Some(fingerprint(p).ok()?);
        self.fingerprint_full = Some(fingerprint_full(p).ok()?);
        Some(self)
    }
}

pub type Progress<'a> = Option<walk::ProgressFn<'a>>;

fn skip_storage(n: &OsStr) -> bool {
    n.to_str().is_some_and(|s| SKIP.contains(&s))
}

fn real_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    roots.iter().filter(|r| r.is_dir()).cloned().collect()
}

/// Regular single-link files below `roots`; each file once even when the
/// roots overlap (home and ~/Nextcloud).
fn each_file<S: Default + Send>(
    roots: &[PathBuf],
    cancel: &AtomicBool,
    progress: Progress,
    visit: impl Fn(&mut S, FileRow) + Sync,
) -> Vec<S> {
    let roots = real_roots(roots);
    let seen = std::sync::Mutex::new(HashSet::new());
    let overlap = roots.len() > 1;
    let opts = Opts {
        cancel,
        progress,
        skip_dir: &skip_storage,
    };
    walk::walk::<S, _>(&roots, &opts, |s, e, ft| {
        if !ft.is_file() {
            return true;
        }
        let Ok(md) = e.metadata() else { return true };
        if md.nlink() != 1 {
            return true;
        }
        if overlap && !seen.lock().unwrap().insert((md.dev(), md.ino())) {
            return true;
        }
        visit(s, FileRow::of(e.path().to_string_lossy().into_owned(), &md));
        true
    })
    .states
}

/// The `limit` largest files, fingerprinted.
pub fn largest(
    roots: &[PathBuf],
    limit: usize,
    cancel: &AtomicBool,
    progress: Progress,
) -> Vec<FileRow> {
    #[derive(Default)]
    struct Top {
        heap: BinaryHeap<std::cmp::Reverse<(u64, String)>>,
        rows: HashMap<String, FileRow>,
    }
    let states = each_file::<Top>(roots, cancel, progress, |s, row| {
        if s.heap.len() < limit {
            s.heap
                .push(std::cmp::Reverse((row.bytes, row.path.clone())));
            s.rows.insert(row.path.clone(), row);
        } else if s.heap.peek().is_some_and(|m| m.0 .0 < row.bytes) {
            if let Some(std::cmp::Reverse((_, p))) = s.heap.pop() {
                s.rows.remove(&p);
            }
            s.heap
                .push(std::cmp::Reverse((row.bytes, row.path.clone())));
            s.rows.insert(row.path.clone(), row);
        }
    });
    if cancel.load(Ordering::Relaxed) {
        return Vec::new();
    }
    let mut all: Vec<FileRow> = states
        .into_iter()
        .flat_map(|s| s.rows.into_values())
        .collect();
    all.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));
    all.truncate(limit);
    let done: Vec<FileRow> = walk::par_map(&all, cancel, |r| r.clone().fingerprinted())
        .into_iter()
        .flatten()
        .flatten()
        .collect();
    if cancel.load(Ordering::Relaxed) {
        return Vec::new();
    }
    done
}

#[derive(Clone, Debug)]
pub struct DupGroup {
    pub paths: Vec<String>,
}

fn sha256_file(path: &str, cancel: &AtomicBool, head_only: bool) -> Option<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut d = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut total = 0usize;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let want = if head_only {
            (65536 - total).min(buf.len())
        } else {
            buf.len()
        };
        if want == 0 {
            break;
        }
        let n = f.read(&mut buf[..want]).ok()?;
        if n == 0 {
            break;
        }
        total += n;
        d.update(&buf[..n]);
    }
    Some(crate::safety::hex(&d.finalize()))
}

/// Groups of identical files: same size, then same first 64 KiB, then the
/// same SHA-256 of the whole file. Nothing is removed or ranked.
pub fn duplicates(
    roots: &[PathBuf],
    limit: usize,
    cancel: &AtomicBool,
    progress: Progress,
) -> Vec<DupGroup> {
    // Pass 1: only sizes, so a home with millions of files stays small.
    let counts = each_file::<HashMap<u64, u32>>(roots, cancel, progress, |s, row| {
        if row.bytes > 0 {
            *s.entry(row.bytes).or_default() += 1;
        }
    });
    if cancel.load(Ordering::Relaxed) {
        return Vec::new();
    }
    let mut sizes: HashMap<u64, u32> = HashMap::new();
    for m in counts {
        for (k, v) in m {
            *sizes.entry(k).or_default() += v;
        }
    }
    sizes.retain(|_, v| *v > 1);
    if sizes.is_empty() {
        return Vec::new();
    }
    // Pass 2: the paths of files whose size occurs more than once.
    let found = each_file::<Vec<(u64, String)>>(roots, cancel, progress, |s, row| {
        if sizes.contains_key(&row.bytes) {
            s.push((row.bytes, row.path));
        }
    });
    if cancel.load(Ordering::Relaxed) {
        return Vec::new();
    }
    let mut by_size: HashMap<u64, Vec<String>> = HashMap::new();
    for v in found {
        for (size, p) in v {
            by_size.entry(size).or_default().push(p);
        }
    }
    let mut groups: Vec<(u64, Vec<String>)> =
        by_size.into_iter().filter(|(_, v)| v.len() > 1).collect();
    // Largest first: the most space is where the review pays off.
    groups.sort_by_key(|g| std::cmp::Reverse(g.0));
    let mut results = Vec::new();
    for (size, mut paths) in groups {
        if cancel.load(Ordering::Relaxed) || results.len() >= limit {
            break;
        }
        paths.sort();
        let heads = walk::par_map(&paths, cancel, |p| sha256_file(p, cancel, true));
        let mut by_head: HashMap<String, Vec<String>> = HashMap::new();
        for (p, h) in paths.iter().zip(heads) {
            if let Some(Some(h)) = h {
                by_head.entry(h).or_default().push(p.clone());
            }
        }
        let mut heads: Vec<(String, Vec<String>)> =
            by_head.into_iter().filter(|(_, v)| v.len() > 1).collect();
        heads.sort();
        for (head, same) in heads {
            // Up to 64 KiB the head hash is the hash of the whole file.
            let full = if size <= 65536 {
                same.iter()
                    .map(|_| Some(Some(head.clone())))
                    .collect::<Vec<_>>()
            } else {
                walk::par_map(&same, cancel, |p| sha256_file(p, cancel, false))
            };
            let mut by_hash: Vec<(String, Vec<String>)> = Vec::new();
            for (p, h) in same.iter().zip(full) {
                if let Some(Some(h)) = h {
                    match by_hash.iter_mut().find(|(k, _)| *k == h) {
                        Some((_, v)) => v.push(p.clone()),
                        None => by_hash.push((h, vec![p.clone()])),
                    }
                }
            }
            for (_, paths) in by_hash {
                if paths.len() > 1 {
                    results.push(DupGroup { paths });
                    if results.len() >= limit {
                        break;
                    }
                }
            }
            if results.len() >= limit {
                break;
            }
        }
    }
    if cancel.load(Ordering::Relaxed) {
        return Vec::new();
    }
    results
}

/// Long-unchanged files below `root`: the `limit` oldest, fingerprinted.
pub fn find_old_files(
    root: &Path,
    months: i64,
    limit: usize,
    cancel: &AtomicBool,
    progress: Progress,
) -> R<Vec<FileRow>> {
    if !(1..=120).contains(&months) || !(1..=10000).contains(&limit) {
        return Err("Invalid age or limit".into());
    }
    let root = PathBuf::from(crate::config::expand_user(&root.to_string_lossy()));
    let is_link = std::fs::symlink_metadata(&root).is_ok_and(|m| m.file_type().is_symlink());
    if !root.is_dir() || is_link {
        return Err("Invalid root".into());
    }
    let threshold = crate::fmt::now_f64() - months as f64 * 30.4375 * 86400.0;
    let skip = |n: &OsStr| {
        n.to_str()
            .is_none_or(|s| s.starts_with('.') || FORGOTTEN_EXCLUDED.contains(&s))
    };
    let opts = Opts {
        cancel,
        progress,
        skip_dir: &skip,
    };
    let out = walk::walk::<Vec<FileRow>, _>(&[root], &opts, |s, e, _ft| {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.')
            || [".part", ".crdownload", ".tmp", ".lock"]
                .iter()
                .any(|x| name.ends_with(x))
        {
            return true;
        }
        let Ok(md) = e.metadata() else { return true };
        if !md.file_type().is_file() || md.nlink() != 1 {
            return true;
        }
        let row = FileRow::of(e.path().to_string_lossy().into_owned(), &md);
        if row.mtime > threshold {
            return true;
        }
        s.push(row);
        true
    });
    if cancel.load(Ordering::Relaxed) {
        return Ok(Vec::new());
    }
    let mut all: Vec<FileRow> = out.states.into_iter().flatten().collect();
    all.sort_by(|a, b| {
        a.mtime
            .total_cmp(&b.mtime)
            .then_with(|| a.path.cmp(&b.path))
    });
    all.truncate(limit);
    let mut done: Vec<FileRow> = walk::par_map(&all, cancel, |r| {
        let mut r = r.clone().fingerprinted()?;
        r.modified = Some(crate::fmt::iso_utc(r.mtime));
        Some(r)
    })
    .into_iter()
    .flatten()
    .flatten()
    .collect();
    if cancel.load(Ordering::Relaxed) {
        return Ok(Vec::new());
    }
    done.sort_by(|a, b| a.modified.cmp(&b.modified));
    Ok(done)
}

pub fn file_category(path: &str) -> &'static str {
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = crate::safety::suffix(&name).to_lowercase();
    let is = |l: &[&str]| l.contains(&ext.as_str());
    if is(&[".zip", ".7z", ".rar", ".tar", ".gz", ".xz", ".bz2", ".zst"]) {
        "Archives"
    } else if is(&[".mp4", ".m4v", ".mkv", ".mov", ".avi", ".webm", ".wmv"]) {
        "Video"
    } else if is(&[".mp3", ".flac", ".wav", ".ogg", ".opus", ".m4a", ".aac"]) {
        "Audio"
    } else if is(&[
        ".jpg", ".jpeg", ".png", ".gif", ".webp", ".tif", ".tiff", ".heic", ".psd", ".svg",
    ]) {
        "Images"
    } else if is(&[
        ".pdf", ".doc", ".docx", ".txt", ".md", ".odt", ".xls", ".xlsx", ".ppt", ".pptx",
    ]) {
        "Documents"
    } else {
        "Other"
    }
}

fn identity_changed(md: &std::fs::Metadata, row: &FileRow) -> bool {
    md.dev() != row.device || md.ino() != row.inode || ctime_ns(md) != row.ctime_ns
}

#[derive(Clone, Debug, Default)]
pub struct Outcome {
    pub done: Vec<String>,
    pub skipped: Vec<(String, String)>,
}

/// Move explicitly selected, re-verified files to the quarantine.
pub fn storage_quarantine(items: &[FileRow], allow_synced: bool) -> Outcome {
    let mut out = Outcome::default();
    for row in items {
        let src = PathBuf::from(&row.path);
        let r = (|| -> R<Vec<String>> {
            let parent = src.parent().ok_or("Unsafe file")?;
            let root = validate_root(parent)?;
            if is_synced(&src) && !allow_synced {
                return Err("Synced file requires confirmation".into());
            }
            if !safe_file(&src, &root) {
                return Err("Unsafe file".into());
            }
            let md = std::fs::metadata(&src).map_err(|e| e.to_string())?;
            if identity_changed(&md, row)
                || md.size() != row.bytes
                || ftime(md.mtime(), md.mtime_nsec()) != row.mtime
            {
                return Err("File changed since scan".into());
            }
            if let Some(f) = &row.fingerprint {
                if fingerprint(&src).map_err(|e| e.to_string())? != *f {
                    return Err("File content changed since scan".into());
                }
            }
            let cand = crate::cleanup::Candidate::of(&src, &md);
            let rule = json!({"id": "storage-manual", "path": root.to_string_lossy()});
            crate::cleanup::quarantine(&rule, &[cand], allow_synced)
        })();
        match r {
            Ok(m) => out.done.extend(m),
            Err(e) => out.skipped.push((row.path.clone(), e)),
        }
    }
    out
}

/// Permanently unlink explicitly selected, unchanged regular files: no
/// directories, symlinks or hard links, and every unlink is audited.
pub fn storage_delete(items: &[FileRow], allow_synced: bool) -> Outcome {
    let mut out = Outcome::default();
    for row in items {
        let src = PathBuf::from(&row.path);
        let r = (|| -> R<i64> {
            let parent = src.parent().ok_or("Unsafe file")?;
            let root = validate_root(parent)?;
            if is_synced(&src) && !allow_synced {
                return Err("Synced file requires explicit confirmation".into());
            }
            if !safe_file(&src, &root) {
                return Err("Unsafe file".into());
            }
            let before = std::fs::symlink_metadata(&src).map_err(|e| e.to_string())?;
            if !before.file_type().is_file()
                || before.nlink() != 1
                || identity_changed(&before, row)
                || before.size() != row.bytes
                || (ftime(before.mtime(), before.mtime_nsec()) - row.mtime).abs() > 0.000001
            {
                return Err("File changed since scan".into());
            }
            if let Some(f) = &row.fingerprint {
                if fingerprint(&src).map_err(|e| e.to_string())? != *f {
                    return Err("File content changed since scan".into());
                }
            }
            // Opened without following symlinks; the inode is checked again
            // right before the unlink.
            use std::os::unix::fs::OpenOptionsExt;
            let f = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(&src)
                .map_err(|e| e.to_string())?;
            let opened = f.metadata().map_err(|e| e.to_string())?;
            let now = std::fs::symlink_metadata(&src).map_err(|e| e.to_string())?;
            if (opened.dev(), opened.ino()) != (now.dev(), now.ino())
                || (now.dev(), now.ino()) != (before.dev(), before.ino())
            {
                return Err("File replaced during deletion".into());
            }
            if now.size() != before.size()
                || (now.mtime(), now.mtime_nsec()) != (before.mtime(), before.mtime_nsec())
            {
                return Err("File changed during deletion".into());
            }
            std::fs::remove_file(&src).map_err(|e| e.to_string())?;
            drop(f);
            Ok(before.size() as i64)
        })();
        match r {
            Ok(size) => {
                let synced = if is_synced(&src) { "True" } else { "False" };
                let _ = crate::store::record(
                    "storage-manual",
                    &row.path,
                    "",
                    "permanently_deleted",
                    size,
                    &format!("Explicit GUI confirmation; synced={synced}"),
                );
                out.done.push(row.path.clone());
            }
            Err(e) => out.skipped.push((row.path.clone(), e)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("niruflow-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn duplicates_and_largest() {
        let d = tmp("dup");
        std::fs::write(d.join("a.txt"), "same").unwrap();
        std::fs::write(d.join("b.txt"), "same").unwrap();
        std::fs::write(d.join("c.txt"), "different").unwrap();
        let no = AtomicBool::new(false);
        let g = duplicates(std::slice::from_ref(&d), 100, &no, None);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].paths.len(), 2);
        let l = largest(&[d.clone(), d.clone()], 100, &no, None);
        assert_eq!(l.len(), 3);
        assert_eq!(l[0].bytes, 9);
        assert!(l[0].fingerprint.is_some());
        let yes = AtomicBool::new(true);
        assert!(largest(std::slice::from_ref(&d), 100, &yes, None).is_empty());
        assert!(duplicates(std::slice::from_ref(&d), 100, &yes, None).is_empty());
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn symlinked_dirs_are_not_walked() {
        let d = tmp("nosym");
        let root = d.join("root");
        std::fs::create_dir_all(&root).unwrap();
        std::os::unix::fs::symlink(&d, root.join("link")).unwrap();
        let no = AtomicBool::new(false);
        assert!(largest(&[root], 100, &no, None).is_empty());
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn old_files_carry_identity_and_skip_symlinks() {
        let d = tmp("old");
        let f = d.join("old.txt");
        std::fs::write(&f, "old").unwrap();
        let t = libc::timespec {
            tv_sec: 1_600_000_000,
            tv_nsec: 0,
        };
        let c = std::ffi::CString::new(f.to_str().unwrap()).unwrap();
        unsafe { libc::utimensat(libc::AT_FDCWD, c.as_ptr(), [t, t].as_ptr(), 0) };
        std::os::unix::fs::symlink(&f, d.join("sym")).unwrap();
        let no = AtomicBool::new(false);
        let rows = find_old_files(&d, 1, 100, &no, None).unwrap();
        assert_eq!(rows.len(), 1);
        let j = rows[0].to_json();
        for k in [
            "device",
            "inode",
            "ctime_ns",
            "fingerprint",
            "fingerprint_full",
            "mtime",
            "modified",
        ] {
            assert!(j.get(k).is_some(), "{k}");
        }
        assert_eq!(j["modified"], "2020-09-13T12:26:40+00:00");
        let yes = AtomicBool::new(true);
        assert!(find_old_files(&d, 1, 100, &yes, None).unwrap().is_empty());
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn categories() {
        assert_eq!(file_category("song.flac"), "Audio");
        assert_eq!(file_category("movie.mkv"), "Video");
        assert_eq!(file_category("archive.zip"), "Archives");
        assert_eq!(file_category("cover.png"), "Images");
    }
}
