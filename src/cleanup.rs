//! Cleanup rules: a non-recursive dry run, quarantine by atomic same-file
//! system rename (never overwriting), and restore from the history.
use crate::config::{quarantine_dir, random_hex, str_of, R};
use crate::safety::{ftime, is_synced, safe_file, validate_root};
use crate::store;
use serde_json::{json, Value};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub path: String,
    pub bytes: u64,
    pub mtime: f64,
    pub inode: u64,
    pub device: u64,
    pub ctime: f64,
}

impl Candidate {
    pub fn to_json(&self) -> Value {
        json!({"path": self.path, "bytes": self.bytes, "mtime": crate::json::num(self.mtime),
               "inode": self.inode, "device": self.device, "ctime": crate::json::num(self.ctime)})
    }
    pub fn of(path: &Path, md: &std::fs::Metadata) -> Self {
        Candidate {
            path: path.to_string_lossy().into_owned(),
            bytes: md.size(),
            mtime: ftime(md.mtime(), md.mtime_nsec()),
            inode: md.ino(),
            device: md.dev(),
            ctime: ftime(md.ctime(), md.ctime_nsec()),
        }
    }
}

/// Files directly in the rule's folder that are older than its age by both
/// mtime and ctime (a fresh download can carry an old mtime).
pub fn scan(rule: &Value, now: Option<f64>) -> R<Vec<Candidate>> {
    let root = validate_root(Path::new(str_of(rule, "path")))?;
    let age = crate::config::hours(rule)? * 3600.0;
    if age <= 0.0 {
        return Err("Age must be positive".into());
    }
    let now = now.unwrap_or_else(crate::fmt::now_f64);
    let mut found = Vec::new();
    for e in std::fs::read_dir(&root).map_err(|e| e.to_string())? {
        let Ok(e) = e else { continue };
        let item = e.path();
        if !safe_file(&item, &root) {
            continue;
        }
        let Ok(md) = std::fs::metadata(&item) else {
            continue;
        };
        let c = Candidate::of(&item, &md);
        if now - c.mtime >= age && now - c.ctime >= age && now - c.mtime >= 0.0 {
            found.push(c);
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}

/// rename(2) that refuses to replace an existing target.
pub fn rename_noreplace(src: &Path, dst: &Path) -> std::io::Result<()> {
    use std::ffi::CString;
    let s = CString::new(src.as_os_str().as_encoded_bytes())?;
    let d = CString::new(dst.as_os_str().as_encoded_bytes())?;
    let r = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            s.as_ptr(),
            libc::AT_FDCWD,
            d.as_ptr(),
            1 as libc::c_uint, // RENAME_NOREPLACE
        )
    };
    if r == 0 {
        return Ok(());
    }
    let err = std::io::Error::last_os_error();
    match err.raw_os_error() {
        Some(libc::ENOSYS) | Some(libc::EINVAL) => {
            if std::fs::symlink_metadata(dst).is_ok() {
                return Err(std::io::Error::from_raw_os_error(libc::EEXIST));
            }
            std::fs::rename(src, dst)
        }
        _ => Err(err),
    }
}

pub fn quarantine(rule: &Value, items: &[Candidate], allow_synced: bool) -> R<Vec<String>> {
    let root = validate_root(Path::new(str_of(rule, "path")))?;
    if is_synced(&root) && !allow_synced {
        return Err("Synced location requires explicit --allow-synced confirmation".into());
    }
    let dest = quarantine_dir();
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let dev = |p: &Path| {
        std::fs::metadata(p)
            .map(|m| m.dev())
            .map_err(|e| e.to_string())
    };
    if dev(&root)? != dev(&dest)? {
        return Err("Quarantine must be on same filesystem for atomic moves".into());
    }
    let rule_id = str_of(rule, "id").to_string();
    let mut moved = Vec::new();
    for entry in items {
        let src = PathBuf::from(&entry.path);
        if !safe_file(&src, &root) {
            continue;
        }
        let Ok(md) = std::fs::metadata(&src) else {
            continue;
        };
        if Candidate::of(&src, &md) != *entry {
            continue;
        }
        let name = src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let target = dest.join(format!("{}_{name}", random_hex(16)));
        rename_noreplace(&src, &target).map_err(|e| e.to_string())?;
        store::record(
            &rule_id,
            &entry.path,
            &target.to_string_lossy(),
            "quarantined",
            md.size() as i64,
            "",
        )?;
        moved.push(entry.path.clone());
    }
    Ok(moved)
}

pub fn restore(action_id: i64) -> R<String> {
    let all = store::history(100000)?;
    let Some(a) = all.iter().find(|r| r.id == action_id) else {
        return Err("Action not found".into());
    };
    let note = format!("restored action {action_id}");
    if all
        .iter()
        .any(|r| r.status == "restored" && r.message == note)
    {
        return Err("Already restored".into());
    }
    if a.status != "quarantined" {
        return Err("Action cannot be restored".into());
    }
    let src = PathBuf::from(&a.source);
    let target = PathBuf::from(&a.destination);
    let tmd = std::fs::symlink_metadata(&target);
    if !tmd.as_ref().is_ok_and(|m| m.file_type().is_file()) {
        return Err("Quarantined file missing".into());
    }
    if std::fs::symlink_metadata(&src).is_ok() {
        return Err("Original path already exists".into());
    }
    let parent = src.parent().unwrap_or(Path::new("/"));
    if !parent.is_dir() {
        return Err("Original directory missing".into());
    }
    let pdev = std::fs::metadata(parent).map_err(|e| e.to_string())?.dev();
    if pdev != tmd.map_err(|e| e.to_string())?.dev() {
        return Err("Restore requires same filesystem".into());
    }
    // An atomic no-clobber create; the quarantined name goes afterwards.
    std::fs::hard_link(&target, &src).map_err(|e| e.to_string())?;
    std::fs::remove_file(&target).map_err(|e| e.to_string())?;
    store::record(&a.rule, &a.source, &a.destination, "restored", 0, &note)?;
    Ok(a.source.clone())
}
