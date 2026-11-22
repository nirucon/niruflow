//! The guards every destructive action goes through: sync-root detection,
//! a dedicated real directory below HOME, single-link regular files, and a
//! content fingerprint that must not change between scan and action.
use crate::config::{home, R};
use sha2::{Digest, Sha256};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

/// Known sync roots, compared case-insensitively per path component.
pub const SYNC_MARKERS: [&str; 15] = [
    "nextcloud",
    "owncloud",
    "dropbox",
    "onedrive",
    "google drive",
    "google-drive",
    "googledrive",
    "megasync",
    "mega",
    "pcloud",
    "icloud drive",
    "icloud-drive",
    "seafile",
    "syncthing",
    "rclone",
];

/// `Path.resolve(strict=False)`: symlinks resolved as far as the path
/// exists, the missing rest appended.
pub fn resolve_lenient(p: &Path) -> PathBuf {
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_else(|_| home()).join(p)
    };
    if let Ok(c) = std::fs::canonicalize(&abs) {
        return c;
    }
    let comps: Vec<Component> = abs.components().collect();
    for cut in (1..comps.len()).rev() {
        let head: PathBuf = comps[..cut].iter().collect();
        if let Ok(mut c) = std::fs::canonicalize(&head) {
            for comp in &comps[cut..] {
                match comp {
                    Component::CurDir => {}
                    Component::ParentDir => {
                        c.pop();
                    }
                    Component::Normal(n) => c.push(n),
                    _ => {}
                }
            }
            return c;
        }
    }
    abs
}

pub fn sync_provider(path: &Path) -> Option<&'static str> {
    let expanded = PathBuf::from(crate::config::expand_user(&path.to_string_lossy()));
    let resolved = resolve_lenient(&expanded);
    let parts: Vec<String> = resolved
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    SYNC_MARKERS
        .iter()
        .find(|m| parts.iter().any(|p| p == *m))
        .copied()
}

/// Kept under the old name: every recognised sync provider, not only Nextcloud.
pub fn is_synced(path: &Path) -> bool {
    sync_provider(path).is_some()
}

fn is_symlink(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink())
}

/// A regular, single-link file directly inside `root`, neither a symlink.
pub fn safe_file(path: &Path, root: &Path) -> bool {
    if is_symlink(root) || is_symlink(path) {
        return false;
    }
    let (Ok(rp), Ok(rr)) = (std::fs::canonicalize(path), std::fs::canonicalize(root)) else {
        return false;
    };
    if rp.parent() != Some(rr.as_path()) {
        return false;
    }
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file() && m.nlink() == 1)
}

/// A dedicated, existing, real directory below HOME and outside hidden ones.
pub fn validate_root(path: &Path) -> R<PathBuf> {
    let root = PathBuf::from(crate::config::expand_user(&path.to_string_lossy()));
    if is_symlink(&root) || !root.is_dir() {
        return Err("Root must be an existing real directory (not a symlink)".into());
    }
    let resolved = std::fs::canonicalize(&root).map_err(|e| e.to_string())?;
    let h = resolve_lenient(&home());
    if resolved == h || resolved == Path::new("/") || !resolved.starts_with(&h) {
        return Err("Only a dedicated directory below HOME is permitted".into());
    }
    let rel = resolved.strip_prefix(&h).unwrap_or(&resolved);
    if rel
        .components()
        .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
    {
        return Err("Hidden/system directories are protected".into());
    }
    Ok(resolved)
}

pub const FULL_LIMIT: u64 = 64 * 1024 * 1024;
const BLOCK: usize = 128 * 1024;

/// Bounded content fingerprint: SHA-256 of the size and the whole file up
/// to 64 MiB, else of the first and last 128 KiB.
pub fn fingerprint(path: &Path) -> std::io::Result<String> {
    let size = std::fs::metadata(path)?.len();
    let mut d = Sha256::new();
    d.update(size.to_string().as_bytes());
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; BLOCK];
    if size <= FULL_LIMIT {
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            d.update(&buf[..n]);
        }
    } else {
        let n = read_up_to(&mut f, &mut buf)?;
        d.update(&buf[..n]);
        f.seek(SeekFrom::End(-(BLOCK as i64)))?;
        let n = read_up_to(&mut f, &mut buf)?;
        d.update(&buf[..n]);
    }
    Ok(hex(&d.finalize()))
}

fn read_up_to(f: &mut std::fs::File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut got = 0;
    while got < buf.len() {
        let n = f.read(&mut buf[got..])?;
        if n == 0 {
            break;
        }
        got += n;
    }
    Ok(got)
}

pub fn fingerprint_full(path: &Path) -> std::io::Result<bool> {
    Ok(std::fs::metadata(path)?.len() <= FULL_LIMIT)
}

pub fn hex(b: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push(H[(x >> 4) as usize] as char);
        s.push(H[(x & 15) as usize] as char);
    }
    s
}

/// Python's float `st_mtime`/`st_ctime` from seconds and nanoseconds.
pub fn ftime(sec: i64, nsec: i64) -> f64 {
    sec as f64 + nsec as f64 * 1e-9
}

pub fn ctime_ns(md: &std::fs::Metadata) -> i64 {
    md.ctime() * 1_000_000_000 + md.ctime_nsec()
}

/// `Path.suffix`: the last dot part of the name, not for dot files.
pub fn suffix(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 && i < name.len() - 1 => &name[i..],
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_roots_case_insensitive_and_unrelated_names() {
        for (p, m) in [
            ("~/Nextcloud/file", "nextcloud"),
            ("~/Dropbox/file", "dropbox"),
            ("~/OneDrive/file", "onedrive"),
            ("~/Google Drive/file", "google drive"),
            ("~/MEGAsync/file", "megasync"),
            ("~/rclone/file", "rclone"),
        ] {
            assert_eq!(sync_provider(Path::new(p)), Some(m));
        }
        assert_eq!(
            sync_provider(Path::new("~/projects/my-sync-notes/file.txt")),
            None
        );
    }

    #[test]
    fn home_and_root_are_refused() {
        assert!(validate_root(&home()).is_err());
        assert!(validate_root(Path::new("/")).is_err());
    }

    #[test]
    fn fingerprints_are_bounded() {
        let d = std::env::temp_dir().join(format!("niruflow-fp-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let small = d.join("small");
        std::fs::write(&small, b"abc").unwrap();
        assert!(fingerprint_full(&small).unwrap());
        // Python: sha256(b"3" + b"abc")
        assert_eq!(
            fingerprint(&small).unwrap(),
            "7b0393befdc28ff2a7c5e08fa8a38a4c845da5c600476eb01ae7b865fa344f18"
        );
        let large = d.join("large");
        let f = std::fs::File::create(&large).unwrap();
        f.set_len(65 * 1024 * 1024).unwrap();
        assert!(!fingerprint_full(&large).unwrap());
        assert_eq!(fingerprint(&large).unwrap().len(), 64);
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn suffix_like_pathlib() {
        assert_eq!(suffix("a.zip"), ".zip");
        assert_eq!(suffix(".zip"), "");
        assert_eq!(suffix("a."), "");
        assert_eq!(suffix("a.tar.gz"), ".gz");
    }
}
