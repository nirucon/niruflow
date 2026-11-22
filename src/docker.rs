//! Docker Engine adapter: the `docker` CLI without a shell, read-only
//! inspection, explicit non-deleting container actions, and the
//! conservative image cleanup (preview, then revalidated removal by tag).
use crate::config::R;
use serde_json::{json, Map, Value};
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub fn have(cmd: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|d| {
            let p = d.join(cmd);
            std::fs::metadata(&p).is_ok_and(|m| {
                use std::os::unix::fs::PermissionsExt;
                m.is_file() && m.permissions().mode() & 0o111 != 0
            })
        })
    })
}

/// Run a command with a timeout; stdout on success, stderr (or stdout) as
/// the error otherwise.
pub fn run_cmd(
    program: &str,
    args: &[&str],
    timeout: u64,
) -> std::io::Result<(i32, String, String)> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let t_out = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = out.read_to_end(&mut s);
        s
    });
    let t_err = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = err.read_to_end(&mut s);
        s
    });
    let start = Instant::now();
    let status = loop {
        if let Some(st) = child.try_wait()? {
            break st;
        }
        if start.elapsed() > Duration::from_secs(timeout) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"));
        }
        std::thread::sleep(Duration::from_millis(15));
    };
    let o = String::from_utf8_lossy(&t_out.join().unwrap_or_default()).into_owned();
    let e = String::from_utf8_lossy(&t_err.join().unwrap_or_default()).into_owned();
    Ok((status.code().unwrap_or(-1), o, e))
}

pub fn real_docker(args: &[&str], timeout: u64) -> R<String> {
    if !have("docker") {
        return Err("Docker CLI is missing. Install Docker and check PATH.".into());
    }
    match run_cmd("docker", args, timeout) {
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
            Err("Docker did not answer within the time limit".into())
        }
        Err(e) => Err(e.to_string()),
        Ok((0, o, _)) => Ok(o),
        Ok((_, o, e)) => {
            let msg = if e.trim().is_empty() {
                o.trim()
            } else {
                e.trim()
            };
            Err(if msg.is_empty() {
                "The Docker command failed".into()
            } else {
                msg.into()
            })
        }
    }
}

pub struct Docker<'a> {
    pub run: &'a (dyn Fn(&[&str], u64) -> R<String> + Sync + 'a),
}

impl Default for Docker<'static> {
    fn default() -> Self {
        Docker { run: &real_docker }
    }
}

fn is_hex(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn sha_id(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn strip_sha(v: Option<&Value>) -> String {
    let s = match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(o) => o.to_string(),
    };
    s.strip_prefix("sha256:").map(str::to_string).unwrap_or(s)
}

impl Docker<'_> {
    fn run(&self, args: &[&str]) -> R<String> {
        (self.run)(args, 25)
    }

    fn json(&self, args: &[&str]) -> R<Value> {
        let out = (self.run)(args, 90)?;
        serde_json::from_str(&out).map_err(|_| "Docker returned invalid JSON".to_string())
    }

    fn lines(&self, args: &[&str]) -> R<Vec<Value>> {
        let mut out = Vec::new();
        for line in self.run(args)?.lines() {
            if !line.trim().is_empty() {
                out.push(
                    serde_json::from_str(line)
                        .map_err(|_| "Docker returned invalid JSON".to_string())?,
                );
            }
        }
        Ok(out)
    }

    pub fn context(&self) -> R<String> {
        Ok(self.run(&["context", "show"])?.trim().to_string())
    }

    pub fn inspect(&self) -> R<Value> {
        let context = self.context()?;
        let containers = self.lines(&["ps", "-a", "--no-trunc", "--format", "{{json .}}"])?;
        let images = self.lines(&["image", "ls", "-a", "--no-trunc", "--format", "{{json .}}"])?;
        let disk = self.run(&["system", "df"])?;
        let field = |x: &Value, k: &str| -> String {
            match x.get(k) {
                Some(Value::String(s)) => s.to_lowercase(),
                Some(v) => v.to_string().to_lowercase(),
                None => String::new(),
            }
        };
        let running = containers
            .iter()
            .filter(|x| field(x, "State") == "running")
            .count();
        let unhealthy = containers
            .iter()
            .filter(|x| field(x, "Status").contains("unhealthy"))
            .count();
        Ok(
            json!({"context": context, "containers": containers.clone(), "images": images.clone(),
            "disk": disk,
            "summary": {"containers": containers.len(), "running": running, "unhealthy": unhealthy,
                        "images": images.len()},
            "readonly": true}),
        )
    }

    /// start, stop, restart, pause or unpause one exact container.
    pub fn container_action(&self, id: &str, action: &str) -> R<String> {
        if !["start", "stop", "restart", "pause", "unpause"].contains(&action) {
            return Err("Container action not allowed".into());
        }
        if id.len() < 12 || !is_hex(id) {
            return Err("A valid Docker container ID is required".into());
        }
        Ok((self.run)(&["container", action, id], 70)?
            .trim()
            .to_string())
    }

    pub fn logs(&self, id: &str, tail: u32) -> R<String> {
        if id.len() < 12 || !is_hex(id) {
            return Err("Invalid container ID".into());
        }
        let tail = tail.clamp(1, 500).to_string();
        (self.run)(&["logs", "--tail", &tail, id], 30)
    }

    pub fn mounts(&self, id: &str) -> R<Value> {
        if id.len() < 12 || !is_hex(id) {
            return Err("Invalid container ID".into());
        }
        let v: Value = serde_json::from_str(&self.run(&["container", "inspect", id])?)
            .map_err(|_| "Docker returned invalid JSON".to_string())?;
        Ok(v.get(0)
            .and_then(|c| c.get("Mounts"))
            .cloned()
            .unwrap_or_else(|| json!([])))
    }

    /// Remove one explicitly approved stopped container, never its volumes.
    pub fn container_remove(
        &self,
        id: &str,
        expected_context: &str,
        confirmation: &str,
    ) -> R<String> {
        if confirmation != "REMOVE CONTAINER" {
            return Err("Explicit confirmation required".into());
        }
        if id.len() != 64 || !is_hex(id) {
            return Err("A full Docker container ID is required".into());
        }
        if expected_context.is_empty() || self.context()? != expected_context {
            return Err("Docker context changed; refresh first".into());
        }
        let v: Value = serde_json::from_str(&self.run(&["container", "inspect", id])?)
            .map_err(|_| "Docker returned invalid JSON".to_string())?;
        let arr = v.as_array().cloned().unwrap_or_default();
        if arr.len() != 1
            || arr[0]
                .get("Id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_lowercase()
                != id.to_lowercase()
        {
            return Err("Container identity changed".into());
        }
        let state = arr[0].get("State");
        if state
            .and_then(|s| s.get("Running"))
            .and_then(Value::as_bool)
            .unwrap_or(true)
        {
            return Err("Container is running; stop it explicitly first".into());
        }
        if state
            .and_then(|s| s.get("Paused"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err("Paused containers are protected".into());
        }
        // No -f and no -v: running containers are refused, volumes remain.
        Ok((self.run)(&["container", "rm", id], 90)?.trim().to_string())
    }

    fn referenced(&self, ids: &[String]) -> R<Vec<String>> {
        let mut out = Vec::new();
        for chunk in ids.chunks(40) {
            let mut args = vec!["container", "inspect"];
            args.extend(chunk.iter().map(String::as_str));
            if let Some(a) = self.json(&args)?.as_array() {
                for c in a {
                    out.push(strip_sha(c.get("Image")));
                }
            }
        }
        Ok(out)
    }

    /// Image cleanup candidates: old, unreferenced and (unless opted in)
    /// untagged images. Sizes are summed image sizes, not reclaimable space.
    pub fn cleanup_preview(&self, days: i64, include_unused: bool) -> R<Value> {
        if !(1..=3650).contains(&days) {
            return Err("Age must be between 1 and 3650 days".into());
        }
        let context = self.context()?;
        let ids: Vec<String> = self
            .run(&["container", "ls", "-aq", "--no-trunc"])?
            .split_whitespace()
            .map(str::to_string)
            .collect();
        let referenced = if ids.is_empty() {
            Vec::new()
        } else {
            let mut args = vec!["container", "inspect"];
            args.extend(ids.iter().map(String::as_str));
            self.json(&args)?
                .as_array()
                .map(|a| a.iter().map(|c| strip_sha(c.get("Image"))).collect())
                .unwrap_or_default()
        };
        let mut image_ids: Vec<String> = Vec::new();
        for id in self
            .run(&["image", "ls", "-aq", "--no-trunc"])?
            .split_whitespace()
        {
            if !image_ids.iter().any(|x| x == id) {
                image_ids.push(id.to_string());
            }
        }
        let mut candidates = Vec::new();
        let (mut in_use, mut tagged, mut too_new, mut invalid) = (0, 0, 0, 0);
        let cutoff = crate::fmt::now_f64() - days as f64 * 86400.0;
        for chunk in image_ids.chunks(40) {
            let mut args = vec!["image", "inspect"];
            args.extend(chunk.iter().map(String::as_str));
            let v = self.json(&args)?;
            for e in v.as_array().cloned().unwrap_or_default() {
                let id = strip_sha(e.get("Id"));
                if !sha_id(&id) {
                    invalid += 1;
                    continue;
                }
                if referenced.contains(&id) {
                    in_use += 1;
                    continue;
                }
                let tags: Vec<String> = e
                    .get("RepoTags")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .filter(|t| !t.is_empty() && *t != "<none>:<none>")
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                if !tags.is_empty() && !include_unused {
                    tagged += 1;
                    continue;
                }
                let created = e
                    .get("Created")
                    .and_then(Value::as_str)
                    .and_then(crate::fmt::parse_iso)
                    .filter(|c| c.offset.is_some());
                let Some(created) = created else {
                    invalid += 1;
                    continue;
                };
                if created.epoch() > cutoff {
                    too_new += 1;
                    continue;
                }
                let bytes = e.get("Size").and_then(Value::as_i64).unwrap_or(0);
                candidates.push(
                    json!({"id": id, "tags": tags.clone(), "created": created.isoformat(),
                                       "bytes": bytes, "dangling": tags.is_empty()}),
                );
            }
        }
        let apparent: i64 = candidates
            .iter()
            .map(|c| c["bytes"].as_i64().unwrap_or(0))
            .sum();
        Ok(
            json!({"context": context, "age_days": days, "include_unused": include_unused,
            "candidates": candidates.clone(), "count": candidates.len(),
            "excluded": {"in_use": in_use, "tagged_opt_out": tagged, "too_new": too_new, "invalid": invalid},
            "apparent_bytes": apparent}),
        )
    }

    /// Preview again against the live engine and remove only exact,
    /// unchanged candidates, tag by tag, never with --force.
    pub fn cleanup_apply(&self, plan: &Value, approved: &[String], confirmation: &str) -> R<Value> {
        if confirmation != "DELETE IMAGES" {
            return Err("Type DELETE IMAGES to confirm".into());
        }
        let context = plan.get("context").and_then(Value::as_str).unwrap_or("");
        if self.context()? != context {
            return Err("Docker context changed; preview again".into());
        }
        let days = plan.get("age_days").and_then(Value::as_i64).unwrap_or(30);
        let include = plan
            .get("include_unused")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let current = self.cleanup_preview(days, include)?;
        let by_id = |v: &Value| -> Map<String, Value> {
            v.get("candidates")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|x| (x["id"].as_str().unwrap_or("").to_string(), x.clone()))
                        .collect()
                })
                .unwrap_or_default()
        };
        let allowed = by_id(&current);
        let previous = by_id(plan);
        let mut ids: Vec<String> = Vec::new();
        for i in approved {
            if !ids.contains(i) {
                ids.push(i.clone());
            }
        }
        if ids.iter().any(|i| !sha_id(i)) {
            return Err("Invalid Docker image ID".into());
        }
        if ids.is_empty()
            || ids.iter().any(|i| {
                !allowed.contains_key(i) || !previous.contains_key(i) || allowed[i] != previous[i]
            })
        {
            return Err("Candidates changed; preview again".into());
        }
        let mut removed = Vec::new();
        let mut errors = Vec::new();
        let mut removed_tags = 0;
        for id in &ids {
            let r = (|| -> R<()> {
                let live: Vec<String> = self
                    .run(&["container", "ls", "-aq", "--no-trunc"])?
                    .split_whitespace()
                    .map(str::to_string)
                    .collect();
                if self.referenced(&live)?.contains(id) {
                    return Err("Image is now referenced by a container".into());
                }
                let tags: Vec<String> = previous[id]["tags"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                if tags.is_empty() {
                    (self.run)(&["image", "rm", id], 90)?;
                } else {
                    for tag in &tags {
                        if self.context()? != context {
                            return Err("Docker context changed during cleanup".into());
                        }
                        let live: Vec<String> = self
                            .run(&["container", "ls", "-aq", "--no-trunc"])?
                            .split_whitespace()
                            .map(str::to_string)
                            .collect();
                        if self.referenced(&live)?.contains(id) {
                            return Err("Image is now referenced by a container".into());
                        }
                        let img = self.json(&["image", "inspect", tag])?;
                        let arr = img.as_array().cloned().unwrap_or_default();
                        if arr.len() != 1 || strip_sha(arr[0].get("Id")) != *id {
                            return Err(format!("Tag changed since preview: {tag}"));
                        }
                        (self.run)(&["image", "rm", tag], 90)?;
                        removed_tags += 1;
                    }
                }
                Ok(())
            })();
            match r {
                Ok(()) => removed.push(id.clone()),
                Err(e) => errors.push(json!({"id": id, "error": e})),
            }
        }
        Ok(json!({"removed": removed, "errors": errors, "removed_tags": removed_tags}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn days_ago(d: f64) -> String {
        crate::fmt::iso_utc(crate::fmt::now_f64() - d * 86400.0)
    }

    #[test]
    fn tagged_images_are_removed_by_tag_not_id() {
        let id = "d".repeat(64);
        let created = days_ago(90.0);
        let calls = Mutex::new(Vec::new());
        let run = |a: &[&str], _t: u64| -> R<String> {
            calls.lock().unwrap().push(a.join(" "));
            match a {
                ["context", "show"] => Ok("default".into()),
                ["container", "ls", ..] => Ok(String::new()),
                ["image", "ls", ..] => Ok(id.clone()),
                ["image", "inspect", ..] => Ok(
                    json!([{"Id": format!("sha256:{id}"), "RepoTags": ["app:1", "mirror:1"], "Created": created, "Size": 123}]).to_string(),
                ),
                ["image", "rm", x] if *x == id => Err("must be forced - multiple repositories".into()),
                ["image", "rm", x] => Ok(x.to_string()),
                _ => Err(format!("unexpected {a:?}")),
            }
        };
        let d = Docker { run: &run };
        let plan = d.cleanup_preview(7, true).unwrap();
        let r = d
            .cleanup_apply(&plan, std::slice::from_ref(&id), "DELETE IMAGES")
            .unwrap();
        assert_eq!(r["removed"], json!([id]));
        assert_eq!(r["removed_tags"], 2);
        assert_eq!(r["errors"], json!([]));
        assert!(!calls.lock().unwrap().contains(&format!("image rm {id}")));
    }

    #[test]
    fn references_opt_out_and_confirmation() {
        let (a, b, c) = ("a".repeat(64), "b".repeat(64), "c".repeat(64));
        let old = days_ago(90.0);
        let run = |args: &[&str], _t: u64| -> R<String> {
            match args {
                ["context", "show"] => Ok("default".into()),
                ["container", "ls", ..] => Ok(c.clone()),
                ["container", "inspect", ..] => Ok(json!([{"Image": format!("sha256:{c}")}]).to_string()),
                ["image", "ls", ..] => Ok(format!("{a}\n{b}\n{c}")),
                ["image", "inspect", "project:v1"] => Ok(json!([{"Id": format!("sha256:{a}"), "RepoTags": ["project:v1"], "Created": old, "Size": 100}]).to_string()),
                ["image", "inspect", ..] => Ok(json!([a.clone(), b.clone(), c.clone()].iter().map(|i| json!({"Id": format!("sha256:{i}"),
                    "RepoTags": [if *i == a { "project:v1" } else { "<none>:<none>" }], "Created": old, "Size": 100})).collect::<Vec<_>>()).to_string()),
                ["image", "rm", x] => Ok(x.to_string()),
                _ => Err(format!("unexpected {args:?}")),
            }
        };
        let d = Docker { run: &run };
        let p = d.cleanup_preview(1, true).unwrap();
        assert_eq!(p["count"], 2);
        assert_eq!(p["excluded"]["in_use"], 1);
        let p2 = d.cleanup_preview(1, false).unwrap();
        assert_eq!(p2["candidates"][0]["id"], json!(b));
        assert_eq!(p2["excluded"]["tagged_opt_out"], 1);
        assert!(d
            .cleanup_apply(&p, std::slice::from_ref(&a), "yes")
            .is_err());
        let r = d
            .cleanup_apply(&p, std::slice::from_ref(&a), "DELETE IMAGES")
            .unwrap();
        assert_eq!(r["removed"], json!([a]));
        assert!(d
            .cleanup_apply(&p, &["not-an-id".into()], "DELETE IMAGES")
            .is_err());
        assert!(d.cleanup_preview(0, false).is_err());
    }

    #[test]
    fn container_guards() {
        let run = |args: &[&str], _t: u64| -> R<String> {
            match args {
                ["context", "show"] => Ok("default".into()),
                ["container", "inspect", id] => {
                    Ok(json!([{"Id": id, "State": {"Running": id.starts_with('b')}}]).to_string())
                }
                ["container", "rm", id] => Ok(id.to_string()),
                ["container", "stop", id] => Ok(id.to_string()),
                _ => Err("unexpected".into()),
            }
        };
        let d = Docker { run: &run };
        assert!(d.container_action("abcdef123456", "rm").is_err());
        assert!(d.container_action("x", "stop").is_err());
        assert_eq!(
            d.container_action("abcdef123456", "stop").unwrap(),
            "abcdef123456"
        );
        let a = "a".repeat(64);
        assert!(d.container_remove(&a, "default", "").is_err());
        assert!(d
            .container_remove("abc", "default", "REMOVE CONTAINER")
            .is_err());
        assert_eq!(
            d.container_remove(&a, "default", "REMOVE CONTAINER")
                .unwrap(),
            a
        );
        assert!(d
            .container_remove(&"b".repeat(64), "default", "REMOVE CONTAINER")
            .is_err());
    }
}
