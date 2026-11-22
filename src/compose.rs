//! Discover Compose manifests without executing them.
use serde_json::{json, Value};
use std::path::PathBuf;

const NAMES: [&str; 4] = [
    "compose.yaml",
    "compose.yml",
    "docker-compose.yaml",
    "docker-compose.yml",
];
const SKIP: [&str; 20] = [
    ".git",
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
    ".next",
    ".cache",
    "vendor",
    "backups",
    "backup",
    "data",
    "volumes",
    "db",
    "database",
    "patches",
    "patch",
    "payload",
    "archive",
    "archives",
    "old",
];

#[derive(Clone, Debug)]
pub struct Compose {
    pub project: String,
    pub path: String,
    pub root: String,
}

impl Compose {
    pub fn to_json(&self) -> Value {
        json!({"project": self.project, "path": self.path, "root": self.root})
    }
}

fn is_link(p: &std::path::Path) -> bool {
    std::fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink())
}

pub fn discover(roots: &[PathBuf], max_depth: usize, limit: usize) -> Vec<Compose> {
    let mut found = Vec::new();
    for root in roots {
        let Ok(root) = crate::config::expand_path(&root.to_string_lossy()) else {
            continue;
        };
        if !root.is_dir() || is_link(&root) {
            continue;
        }
        let mut stack = vec![(root.clone(), 0usize)];
        while let Some((folder, depth)) = stack.pop() {
            if found.len() >= limit {
                break;
            }
            for name in NAMES {
                let c = folder.join(name);
                if c.is_file() && !is_link(&c) {
                    found.push(Compose {
                        project: folder
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                        path: c.to_string_lossy().into_owned(),
                        root: root.to_string_lossy().into_owned(),
                    });
                }
            }
            if depth < max_depth {
                let Ok(rd) = std::fs::read_dir(&folder) else {
                    continue;
                };
                for e in rd.flatten() {
                    let p = e.path();
                    let name = e.file_name();
                    if p.is_dir()
                        && !is_link(&p)
                        && !name.to_str().is_some_and(|n| SKIP.contains(&n))
                    {
                        stack.push((p, depth + 1));
                    }
                }
            }
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_projects_and_ignores_payloads() {
        let d = std::env::temp_dir().join(format!("niruflow-compose-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let project = d.join("project");
        std::fs::create_dir_all(project.join("patches/0.1/payload")).unwrap();
        std::fs::write(project.join("compose.yaml"), "services: {}").unwrap();
        std::fs::write(
            project.join("patches/0.1/payload/compose.yaml"),
            "services: {}",
        )
        .unwrap();
        let r = discover(std::slice::from_ref(&d), 4, 300);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].project, "project");
        std::fs::remove_dir_all(d).unwrap();
    }
}
