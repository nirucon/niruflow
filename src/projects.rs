//! Conservative release inventory: unknown artifacts and patch chains are
//! protected, only the newest versions per family are marked keep.
use serde_json::{json, Map, Value};
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub struct Class {
    pub kind: String,
    pub version: String,
    pub family: String,
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub project: String,
    pub name: String,
    pub path: String,
    pub class: Class,
    pub status: String,
}

impl Entry {
    /// The key order of 0.x: project, name, path, the classification (patch
    /// rows list from/to first), then status.
    pub fn to_json(&self) -> Value {
        let c = &self.class;
        let mut m = Map::new();
        m.insert("project".into(), json!(self.project));
        m.insert("name".into(), json!(self.name));
        m.insert("path".into(), json!(self.path));
        m.insert("kind".into(), json!(c.kind));
        if c.kind == "patch" {
            m.insert("from".into(), json!(c.from));
            m.insert("to".into(), json!(c.to));
            m.insert("version".into(), json!(c.version));
            m.insert("family".into(), json!(c.family));
        } else {
            m.insert("version".into(), json!(c.version));
            m.insert("family".into(), json!(c.family));
            m.insert("from".into(), json!(c.from));
            m.insert("to".into(), json!(c.to));
        }
        m.insert("status".into(), json!(self.status));
        Value::Object(m)
    }
}

/// One SEMVER match: byte range, the three numbers and a pre-release tag.
#[derive(Clone, Debug)]
struct Sem {
    start: usize,
    end: usize,
    nums: (u64, u64, u64),
    pre: bool,
}

fn digits(b: &[char], i: usize) -> usize {
    let mut j = i;
    while j < b.len() && b[j].is_ascii_digit() {
        j += 1;
    }
    j
}

fn num_of(b: &[char], a: usize, e: usize) -> u64 {
    b[a..e]
        .iter()
        .filter_map(|c| c.to_digit(10))
        .fold(0u64, |n, d| n.saturating_mul(10).saturating_add(d as u64))
}

fn lower_at(b: &[char], i: usize, word: &str) -> bool {
    word.chars().enumerate().all(|(k, w)| {
        b.get(i + k)
            .is_some_and(|c| c.to_lowercase().eq(std::iter::once(w)))
    })
}

/// `(?<!\d)v?(\d+)\.(\d+)\.(\d+)(?:[-.]?(alpha|beta|rc)[.-]?(\d+)?)?` (case
/// insensitive) anchored at `i`.
fn sem_at(b: &[char], i: usize) -> Option<Sem> {
    if i > 0 && b[i - 1].is_ascii_digit() {
        return None;
    }
    let try_from = |s: usize| -> Option<Sem> {
        let e1 = digits(b, s);
        if e1 == s || b.get(e1) != Some(&'.') {
            return None;
        }
        let e2 = digits(b, e1 + 1);
        if e2 == e1 + 1 || b.get(e2) != Some(&'.') {
            return None;
        }
        let e3 = digits(b, e2 + 1);
        if e3 == e2 + 1 {
            return None;
        }
        let nums = (
            num_of(b, s, e1),
            num_of(b, e1 + 1, e2),
            num_of(b, e2 + 1, e3),
        );
        let mut end = e3;
        let mut pre = false;
        // Optional pre-release: separator, tag, separator, number.
        for sep in [true, false] {
            let mut k = e3;
            if sep {
                if matches!(b.get(k), Some('-') | Some('.')) {
                    k += 1;
                } else {
                    continue;
                }
            }
            let tag = ["alpha", "beta", "rc"].iter().find(|w| lower_at(b, k, w));
            if let Some(w) = tag {
                k += w.len();
                // `[.-]?` is greedy: a separator is part of the match even
                // when no digits follow it.
                if matches!(b.get(k), Some('-') | Some('.')) {
                    k += 1;
                }
                k = digits(b, k);
                end = k;
                pre = true;
                break;
            }
        }
        Some(Sem {
            start: i,
            end,
            nums,
            pre,
        })
    };
    if matches!(b.get(i), Some('v') | Some('V')) {
        if let Some(s) = try_from(i + 1) {
            return Some(s);
        }
    }
    try_from(i)
}

fn sem_all(s: &str) -> Vec<(Sem, String)> {
    let b: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if let Some(m) = sem_at(&b, i) {
            let text: String = b[m.start..m.end].iter().collect();
            i = m.end.max(i + 1);
            out.push((m, text));
        } else {
            i += 1;
        }
    }
    out
}

/// `(?:^|[-_])patch[-_](?:from[-_])?v?(X.Y.Z)[-_](?:to[-_])?v?(X.Y.Z)`.
fn patch_search(stem: &str) -> Option<(usize, String, String)> {
    let b: Vec<char> = stem.chars().collect();
    let ver = |i: usize| -> Option<(usize, String)> {
        let i = if matches!(b.get(i), Some('v') | Some('V')) {
            i + 1
        } else {
            i
        };
        let e1 = digits(&b, i);
        if e1 == i || b.get(e1) != Some(&'.') {
            return None;
        }
        let e2 = digits(&b, e1 + 1);
        if e2 == e1 + 1 || b.get(e2) != Some(&'.') {
            return None;
        }
        let e3 = digits(&b, e2 + 1);
        (e3 > e2 + 1).then(|| (e3, b[i..e3].iter().collect()))
    };
    let sep = |i: usize| matches!(b.get(i), Some('-') | Some('_'));
    for start in 0..b.len() {
        let mut k = start;
        if start > 0 || !lower_at(&b, 0, "patch") {
            if !sep(start) {
                continue;
            }
            k += 1;
        }
        if !lower_at(&b, k, "patch") || !sep(k + 5) {
            continue;
        }
        let k = k + 6;
        for skip_from in [true, false] {
            let mut j = k;
            if skip_from {
                if lower_at(&b, j, "from") && sep(j + 4) {
                    j += 5;
                } else {
                    continue;
                }
            }
            let Some((e, from)) = ver(j) else { continue };
            if !sep(e) {
                continue;
            }
            for skip_to in [true, false] {
                let mut t = e + 1;
                if skip_to {
                    if lower_at(&b, t, "to") && sep(t + 2) {
                        t += 3;
                    } else {
                        continue;
                    }
                }
                if let Some((_, to)) = ver(t) {
                    let byte_start: usize = b[..start].iter().map(|c| c.len_utf8()).sum();
                    return Some((byte_start, from, to));
                }
            }
        }
    }
    None
}

fn strip_sep(s: &str) -> &str {
    s.trim_matches(|c| c == '-' || c == '_')
}

pub fn classify(name: &str) -> Class {
    let stem = if name.to_lowercase().ends_with(".zip") {
        &name[..name.len() - 4]
    } else {
        name
    };
    let unknown = |kind: &str| Class {
        kind: kind.into(),
        version: String::new(),
        family: stem.into(),
        from: String::new(),
        to: String::new(),
    };
    if let Some((start, from, to)) = patch_search(stem) {
        let fam = strip_sep(&stem[..start]);
        return Class {
            kind: "patch".into(),
            version: to.clone(),
            family: if fam.is_empty() {
                "unknown".into()
            } else {
                fam.into()
            },
            from,
            to,
        };
    }
    if stem.to_lowercase().contains("patch") {
        return unknown("unknown patch");
    }
    let all = sem_all(stem);
    if all.len() != 1 {
        return unknown("unknown");
    }
    let (m, text) = &all[0];
    let chars: Vec<char> = stem.chars().collect();
    let tail: String = chars[m.end..].iter().collect::<String>().to_lowercase();
    let tail = strip_sep(&tail);
    if !tail.is_empty()
        && !["clean", "release", "source", "full"]
            .iter()
            .any(|x| tail.contains(x))
    {
        return unknown("unknown");
    }
    let head: String = chars[..m.start].iter().collect();
    let fam = strip_sep(&head);
    Class {
        kind: if m.pre { "prerelease" } else { "release" }.into(),
        version: text.trim_start_matches(['v', 'V']).into(),
        family: if fam.is_empty() {
            "unknown".into()
        } else {
            fam.into()
        },
        from: String::new(),
        to: String::new(),
    }
}

fn version_key(v: &str) -> (u64, u64, u64) {
    sem_all(v).first().map(|(m, _)| m.nums).unwrap_or((0, 0, 0))
}

fn is_link(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink())
}

fn is_zip(name: &str) -> bool {
    crate::safety::suffix(name).to_lowercase() == ".zip"
}

fn entry(project: &str, child: &Path) -> Entry {
    let name = child
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let class = classify(&name);
    let status = if class.kind == "release" || class.kind == "prerelease" {
        "review"
    } else {
        "protected"
    };
    Entry {
        project: project.into(),
        path: child.to_string_lossy().into_owned(),
        name,
        class,
        status: status.into(),
    }
}

pub fn analyze(path: &Path, keep: i64) -> Result<Vec<Entry>, String> {
    let root = std::path::PathBuf::from(crate::config::expand_user(&path.to_string_lossy()));
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    if !(1..=20).contains(&keep) {
        return Err("keep must be between 1 and 20".into());
    }
    let root_name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let children: Vec<std::path::PathBuf> = std::fs::read_dir(&root)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .collect();
    let mut entries = Vec::new();
    // Release ZIPs at the top level (Projects-web) count too.
    for child in &children {
        let name = child
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if is_link(child) || !child.is_file() || !is_zip(&name) {
            continue;
        }
        entries.push(entry(&root_name, child));
    }
    for project in &children {
        if !project.is_dir() || is_link(project) {
            continue;
        }
        let Ok(rd) = std::fs::read_dir(project) else {
            continue;
        };
        let pname = project
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        for child in rd.flatten().map(|e| e.path()) {
            let name = child
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if is_link(&child) || (!child.is_dir() && !(child.is_file() && is_zip(&name))) {
                continue;
            }
            entries.push(entry(&pname, &child));
        }
    }
    // Keep the newest stable and the newest pre-release versions per family.
    let mut groups: Vec<((String, String, String), Vec<usize>)> = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        if e.class.kind == "release" || e.class.kind == "prerelease" {
            let key = (
                e.project.clone(),
                e.class.family.to_lowercase(),
                e.class.kind.clone(),
            );
            match groups.iter_mut().find(|(k, _)| *k == key) {
                Some((_, v)) => v.push(i),
                None => groups.push((key, vec![i])),
            }
        }
    }
    for (_, idx) in &groups {
        let mut versions: Vec<String> = Vec::new();
        for &i in idx {
            if !versions.contains(&entries[i].class.version) {
                versions.push(entries[i].class.version.clone());
            }
        }
        versions.sort_by_key(|v| std::cmp::Reverse(version_key(v)));
        let top: Vec<&String> = versions.iter().take(keep as usize).collect();
        let keepers: Vec<String> = top.into_iter().cloned().collect();
        for &i in idx {
            entries[i].status = if keepers.contains(&entries[i].class.version) {
                "keep"
            } else {
                "review"
            }
            .into();
        }
    }
    entries.sort_by(|a, b| {
        (
            a.project.to_lowercase(),
            a.class.family.to_lowercase(),
            a.name.to_lowercase(),
        )
            .cmp(&(
                b.project.to_lowercase(),
                b.class.family.to_lowercase(),
                b.name.to_lowercase(),
            ))
    });
    Ok(entries)
}

/// Inventory counts for the status line; "review" never means safe to delete.
pub fn insights(entries: &[Entry]) -> (usize, usize, usize) {
    let protected = entries
        .iter()
        .filter(|x| {
            ["patch", "unknown patch", "unknown"].contains(&x.class.kind.as_str())
                || x.status == "protected"
        })
        .count();
    let review = entries.iter().filter(|x| x.status == "review").count();
    (entries.len(), protected, review)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_matches_0x() {
        let x = classify("lokai-0.1.1-patch-from-0.0.30.zip");
        assert_ne!(x.kind, "release");
        let x = classify("ingwaz-ai-orkester-patch-0.4.0-to-0.5.0.zip");
        assert_eq!(
            (x.kind.as_str(), x.from.as_str(), x.to.as_str()),
            ("patch", "0.4.0", "0.5.0")
        );
        assert_eq!(x.family, "ingwaz-ai-orkester");
        assert_eq!(classify("nirupres-2.0.0-rc.2.zip").kind, "prerelease");
        assert_eq!(classify("nirupres-2.0.0-rc.2.zip").version, "2.0.0-rc.2");
        assert_eq!(classify("ordning-backup-2026-10-08.zip").kind, "unknown");
        let r = classify("nplay-v1.2.3.zip");
        assert_eq!(
            (r.kind.as_str(), r.version.as_str(), r.family.as_str()),
            ("release", "1.2.3", "nplay")
        );
        assert_eq!(classify("site-1.2.3-clean.zip").kind, "release");
        assert_eq!(classify("site-1.2.3-docs.zip").kind, "unknown");
        assert_eq!(classify("a-1.2.3-b-1.2.4").kind, "unknown");
        assert_eq!(classify("x-1.2.3beta").version, "1.2.3beta");
        assert_eq!(classify("x-11.2.3").version, "11.2.3");
    }

    #[test]
    fn keeps_two_newest() {
        let d = std::env::temp_dir().join(format!("niruflow-proj-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let root = d.join("nplay");
        std::fs::create_dir_all(&root).unwrap();
        for v in ["0.8.0", "0.9.0", "0.9.1"] {
            std::fs::write(root.join(format!("nplay-{v}.zip")), "x").unwrap();
        }
        std::fs::write(d.join("site-1.2.3.zip"), "zip").unwrap();
        let out = analyze(&d, 2).unwrap();
        let st: Vec<(String, String)> = out
            .iter()
            .map(|e| (e.class.version.clone(), e.status.clone()))
            .collect();
        assert!(st.contains(&("0.8.0".into(), "review".into())));
        assert!(st.contains(&("0.9.0".into(), "keep".into())));
        assert!(st.contains(&("0.9.1".into(), "keep".into())));
        assert!(st.contains(&("1.2.3".into(), "keep".into())));
        let j = out[0].to_json();
        let keys: Vec<&String> = j.as_object().unwrap().keys().collect();
        assert_eq!(
            keys,
            ["project", "name", "path", "kind", "version", "family", "from", "to", "status"]
        );
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn classification_matches_python_reference() {
        // Generated with NIRUFLOW 0.6.1 `classify`.
        let table = [
            (
                "lokai-0.1.1-patch-from-0.0.30.zip",
                "unknown patch",
                "",
                "lokai-0.1.1-patch-from-0.0.30",
                "",
                "",
            ),
            (
                "ingwaz-ai-orkester-patch-0.4.0-to-0.5.0.zip",
                "patch",
                "0.5.0",
                "ingwaz-ai-orkester",
                "0.4.0",
                "0.5.0",
            ),
            (
                "nirupres-2.0.0-rc.2.zip",
                "prerelease",
                "2.0.0-rc.2",
                "nirupres",
                "",
                "",
            ),
            (
                "ordning-backup-2026-10-08.zip",
                "unknown",
                "",
                "ordning-backup-2026-10-08",
                "",
                "",
            ),
            ("nplay-v1.2.3.zip", "release", "1.2.3", "nplay", "", ""),
            ("site-1.2.3-clean.zip", "release", "1.2.3", "site", "", ""),
            (
                "site-1.2.3-docs.zip",
                "unknown",
                "",
                "site-1.2.3-docs",
                "",
                "",
            ),
            ("a-1.2.3-b-1.2.4", "unknown", "", "a-1.2.3-b-1.2.4", "", ""),
            ("x-1.2.3beta", "prerelease", "1.2.3beta", "x", "", ""),
            ("x-11.2.3", "release", "11.2.3", "x", "", ""),
            (
                "patch-1.0.0-1.0.1.zip",
                "patch",
                "1.0.1",
                "unknown",
                "1.0.0",
                "1.0.1",
            ),
            (
                "App_Patch_v1.2.3_to_v1.2.4.ZIP",
                "patch",
                "1.2.4",
                "App",
                "1.2.3",
                "1.2.4",
            ),
            ("proj-1.2.3-RC1", "prerelease", "1.2.3-RC1", "proj", "", ""),
            ("proj-1.2.3.rc.", "prerelease", "1.2.3.rc.", "proj", "", ""),
            ("foo1.2.3", "release", "1.2.3", "foo", "", ""),
            ("v1.2.3", "release", "1.2.3", "unknown", "", ""),
            ("1.2.3.4", "unknown", "", "1.2.3.4", "", ""),
            (
                "thing-2.0.0-alpha",
                "prerelease",
                "2.0.0-alpha",
                "thing",
                "",
                "",
            ),
            (
                "x-1.2.3-beta-2-full",
                "prerelease",
                "1.2.3-beta-2",
                "x",
                "",
                "",
            ),
            (
                "PATCHED-1.0.0",
                "unknown patch",
                "",
                "PATCHED-1.0.0",
                "",
                "",
            ),
            (
                "x_patch_from_1.0.0_1.1.0",
                "patch",
                "1.1.0",
                "x",
                "1.0.0",
                "1.1.0",
            ),
            ("abc", "unknown", "", "abc", "", ""),
            (
                "release-v2.10.0-source",
                "release",
                "2.10.0",
                "release",
                "",
                "",
            ),
            ("12.3.4.zip", "release", "12.3.4", "unknown", "", ""),
        ];
        for (n, kind, version, family, from, to) in table {
            let c = classify(n);
            assert_eq!(
                (
                    c.kind.as_str(),
                    c.version.as_str(),
                    c.family.as_str(),
                    c.from.as_str(),
                    c.to.as_str()
                ),
                (kind, version, family, from, to),
                "{n}"
            );
        }
    }
}
