//! `~/.config/niruflow/config.json`: the same file, keys and key order as
//! NIRUFLOW 0.x, with the one-time profile migration (schema 3) and atomic
//! saves that keep `config.previous.json`.
use serde_json::{json, Map, Value};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub type R<T> = Result<T, String>;

pub const KINDS: [&str; 4] = ["cleanup", "projects", "docker", "storage"];

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(fallback))
}

pub fn base_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("niruflow")
}

pub fn state_dir() -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state").join("niruflow")
}

pub fn config_file() -> PathBuf {
    base_dir().join("config.json")
}

pub fn quarantine_dir() -> PathBuf {
    state_dir().join("quarantine")
}

pub fn defaults() -> Value {
    let h = home();
    let p = |rel: &str| h.join(rel).to_string_lossy().into_owned();
    let mut v = json!({
        "version": 2,
        "language": "en",
        "locale": "sv_SE",
        "date_format": "yyyy-MM-dd",
        "time_format": "HH:mm",
        "show_weekday": true,
        "theme": "NIRUX desktop",
        "quarantine_days": 30,
        "rules": [
            {"id":"downloads","name":"Downloads","path":p("Downloads"),
             "hours":24,"enabled":false,"recursive":false,"automatic":true},
            {"id":"screenshots","name":"Screenshots","path":p("Nextcloud/Screenshots"),
             "hours":24,"enabled":false,"recursive":false,"automatic":true}
        ],
        "protected_names": ["Important","Fun","SAVE"],
        "project_path": p("Nextcloud/Projects-Dev"),
    });
    migrate(&mut v);
    v
}

pub fn load() -> R<Value> {
    load_from(&config_file())
}

pub fn load_from(file: &Path) -> R<Value> {
    if !file.exists() {
        return Ok(defaults());
    }
    let fail = |e: String| format!("Cannot load {}: {e}", file.display());
    let text = std::fs::read_to_string(file).map_err(|e| fail(e.to_string()))?;
    let mut raw: Value = serde_json::from_str(&text).map_err(|e| fail(e.to_string()))?;
    if !raw.is_object() || !raw.get("rules").is_some_and(Value::is_array) {
        return Err(fail("Invalid configuration".into()));
    }
    let before = raw
        .get("profile_schema")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    migrate(&mut raw);
    if before < 3 {
        // Keep the exact legacy bytes before persisting the migration.
        let backup = file.with_file_name("config.pre-0.4.0.json");
        if !backup.exists() {
            copy2(file, &backup).map_err(|e| fail(e.to_string()))?;
        }
        save_to(file, &raw).map_err(fail)?;
    }
    Ok(raw)
}

pub fn save(cfg: &Value) -> R<()> {
    save_to(&config_file(), cfg)
}

pub fn save_to(file: &Path, cfg: &Value) -> R<()> {
    let dir = file.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = file.with_extension("tmp");
    {
        let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        f.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
        f.write_all(crate::json::pretty(cfg, false).as_bytes())
            .map_err(|e| e.to_string())?;
        f.flush().map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
    }
    if file.exists() {
        let backup = file.with_file_name("config.previous.json");
        copy2(file, &backup).map_err(|e| e.to_string())?;
        let _ = std::fs::set_permissions(&backup, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, file).map_err(|e| e.to_string())
}

/// `shutil.copy2`: contents, mode and timestamps.
pub fn copy2(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::copy(src, dst)?;
    let md = std::fs::metadata(src)?;
    let times = [
        libc::timespec {
            tv_sec: libc_atime(&md).0,
            tv_nsec: libc_atime(&md).1,
        },
        libc::timespec {
            tv_sec: libc_mtime(&md).0,
            tv_nsec: libc_mtime(&md).1,
        },
    ];
    if let Ok(c) = std::ffi::CString::new(dst.as_os_str().as_encoded_bytes()) {
        unsafe { libc::utimensat(libc::AT_FDCWD, c.as_ptr(), times.as_ptr(), 0) };
    }
    Ok(())
}

fn libc_atime(md: &std::fs::Metadata) -> (libc::time_t, libc::c_long) {
    use std::os::unix::fs::MetadataExt;
    (md.atime() as libc::time_t, md.atime_nsec() as libc::c_long)
}

fn libc_mtime(md: &std::fs::Metadata) -> (libc::time_t, libc::c_long) {
    use std::os::unix::fs::MetadataExt;
    (md.mtime() as libc::time_t, md.mtime_nsec() as libc::c_long)
}

// ------------------------------------------------------------- locations

/// `os.path.expandvars`, `expanduser` and `absolute()` (no symlink
/// resolution, no normalisation).
pub fn expand_path(value: &str) -> R<PathBuf> {
    if value.trim().is_empty() {
        return Err("Path is missing".into());
    }
    let expanded = expand_vars(value);
    let user = expand_user(&expanded);
    let p = PathBuf::from(user);
    if p.is_absolute() {
        Ok(p)
    } else {
        Ok(std::env::current_dir().unwrap_or_else(|_| home()).join(p))
    }
}

pub fn expand_user(s: &str) -> String {
    if s == "~" {
        return home().to_string_lossy().into_owned();
    }
    if let Some(rest) = s.strip_prefix("~/") {
        let h = home().to_string_lossy().into_owned();
        return format!("{}/{}", h.trim_end_matches('/'), rest);
    }
    if let Some(rest) = s.strip_prefix('~') {
        let (user, tail) = rest.split_once('/').map_or((rest, ""), |(u, t)| (u, t));
        if let Some(dir) = user_home(user) {
            return if tail.is_empty() && !rest.contains('/') {
                dir
            } else {
                format!("{}/{}", dir.trim_end_matches('/'), tail)
            };
        }
    }
    s.to_string()
}

fn user_home(user: &str) -> Option<String> {
    let c = std::ffi::CString::new(user).ok()?;
    let pw = unsafe { libc::getpwnam(c.as_ptr()) };
    if pw.is_null() {
        return None;
    }
    let dir = unsafe { std::ffi::CStr::from_ptr((*pw).pw_dir) };
    Some(dir.to_string_lossy().into_owned())
}

/// `$NAME` and `${NAME}` replaced when NAME is set; anything else is kept.
pub fn expand_vars(s: &str) -> String {
    if !s.contains('$') {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$' {
            if chars.get(i + 1) == Some(&'{') {
                if let Some(end) = chars[i + 2..].iter().position(|c| *c == '}') {
                    let name: String = chars[i + 2..i + 2 + end].iter().collect();
                    if let Some(v) = std::env::var_os(&name) {
                        out.push_str(&v.to_string_lossy());
                        i += end + 3;
                        continue;
                    }
                }
            } else {
                let mut j = i + 1;
                while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                if j > i + 1 {
                    let name: String = chars[i + 1..j].iter().collect();
                    if let Some(v) = std::env::var_os(&name) {
                        out.push_str(&v.to_string_lossy());
                        i = j;
                        continue;
                    }
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

pub fn active_profile(cfg: &Value) -> String {
    cfg.get("active_profile")
        .and_then(Value::as_str)
        .unwrap_or("default")
        .to_string()
}

/// The active profile's location list (created when missing).
pub fn profile_locations_mut(cfg: &mut Value) -> Option<&mut Vec<Value>> {
    let active = active_profile(cfg);
    let profiles = cfg.get_mut("profiles")?.as_object_mut()?;
    let prof = profiles
        .entry(active)
        .or_insert_with(|| json!({"locations": []}));
    let obj = prof.as_object_mut()?;
    obj.entry("locations")
        .or_insert_with(|| json!([]))
        .as_array_mut()
}

pub fn locations(cfg: &Value, kind: Option<&str>) -> Vec<Value> {
    let active = active_profile(cfg);
    cfg.get("profiles")
        .and_then(|p| p.get(&active))
        .and_then(|p| p.get("locations"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter(|x| kind.is_none_or(|k| x.get("kind").and_then(Value::as_str) == Some(k)))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

pub fn location_paths(cfg: &Value, kind: &str) -> R<Vec<PathBuf>> {
    let mut out = Vec::new();
    for x in locations(cfg, Some(kind)) {
        if x.get("enabled").and_then(Value::as_bool).unwrap_or(true) {
            let p = x
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "'path'".to_string())?;
            out.push(expand_path(p)?);
        }
    }
    Ok(out)
}

fn sanitize(s: &str, fallback: &str) -> String {
    let s: String = s
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches('-');
    if s.is_empty() {
        fallback.to_string()
    } else {
        s.to_string()
    }
}

/// `getpass.getuser()`: LOGNAME, USER, LNAME, USERNAME, then the passwd entry.
pub fn username() -> String {
    for var in ["LOGNAME", "USER", "LNAME", "USERNAME"] {
        if let Some(v) = std::env::var_os(var).filter(|v| !v.is_empty()) {
            return v.to_string_lossy().into_owned();
        }
    }
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    if pw.is_null() {
        return String::new();
    }
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

pub fn hostname() -> String {
    let mut buf = [0u8; 256];
    unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
    let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

/// A stable per-machine profile key, `user-host`.
pub fn machine_profile_name() -> String {
    let host = hostname();
    let host = host.to_lowercase();
    let host = host.split('.').next().unwrap_or("");
    format!(
        "{}-{}",
        sanitize(&username(), "user"),
        sanitize(host, "host")
    )
}

/// Idempotently keep legacy locations in a machine profile (schema 3).
pub fn migrate(cfg: &mut Value) {
    if cfg
        .get("profile_schema")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        >= 3
    {
        return;
    }
    let Some(obj) = cfg.as_object_mut() else {
        return;
    };
    let old_profiles: Map<String, Value> = obj
        .get("profiles")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let old_active = obj
        .get("active_profile")
        .and_then(Value::as_str)
        .unwrap_or("default")
        .to_string();
    let machine = machine_profile_name();
    let mut existing: Vec<Value> = old_profiles
        .get("default")
        .and_then(|d| d.get("locations"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if old_profiles.is_empty() {
        let project = obj
            .get("project_path")
            .cloned()
            .unwrap_or_else(|| json!("~/Nextcloud/Projects-Dev"));
        existing = vec![
            json!({"id":"projects-dev","name":"Projects-Dev","kind":"projects","path":project,"enabled":true}),
            json!({"id":"projects-web","name":"Projects-web","kind":"projects","path":"~/Nextcloud/Projects-web","enabled":true}),
            json!({"id":"docker","name":"Docker Compose","kind":"docker","path":"~/Docker","enabled":true}),
            json!({"id":"home","name":"Home","kind":"storage","path":"~","enabled":true}),
            json!({"id":"nextcloud","name":"Nextcloud","kind":"storage","path":"~/Nextcloud","enabled":true}),
        ];
    }
    let template = json!({"name":"Standard","locations":[
        {"id":"downloads-location","name":"Downloads","kind":"cleanup","path":"~/Downloads","enabled":true}
    ]});
    let mut profiles = old_profiles;
    if !existing.is_empty() && !profiles.contains_key(&machine) {
        profiles.insert(
            machine.clone(),
            json!({"name": machine, "locations": existing}),
        );
    }
    profiles.insert("default".into(), template);
    let has_machine = !existing.is_empty();
    let active = if old_active == "default" && has_machine {
        machine
    } else if profiles.contains_key(&old_active) {
        old_active
    } else {
        "default".into()
    };
    obj.insert("profiles".into(), Value::Object(profiles));
    obj.insert("active_profile".into(), json!(active));
    obj.insert("profile_schema".into(), json!(3));
    obj.entry("docker_context").or_insert_with(|| json!(""));
}

pub fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    let mut filled = 0;
    while filled < bytes {
        let n = unsafe {
            libc::getrandom(
                buf[filled..].as_mut_ptr() as *mut libc::c_void,
                bytes - filled,
                0,
            )
        };
        if n <= 0 {
            // No getrandom: mix the clock and the pid.
            let t = crate::fmt::now_f64().to_bits() ^ ((std::process::id() as u64) << 32);
            for (i, b) in buf.iter_mut().enumerate().skip(filled) {
                *b = (t.rotate_left(i as u32 * 7) & 0xff) as u8;
            }
            break;
        }
        filled += n as usize;
    }
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn add_location(cfg: &mut Value, name: &str, kind: &str, path: &str) -> R<Value> {
    if !KINDS.contains(&kind) {
        return Err("Unknown location type".into());
    }
    if name.trim().is_empty() {
        return Err("Name is required".into());
    }
    let entry = json!({"id": random_hex(6), "name": name.trim(), "kind": kind, "path": path.trim(), "enabled": true});
    expand_path(path.trim())?;
    profile_locations_mut(cfg)
        .ok_or("No active profile")?
        .push(entry.clone());
    Ok(entry)
}

pub fn rules(cfg: &Value) -> Vec<Value> {
    cfg.get("rules")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

pub fn str_of<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

pub fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// `float(rule.get("hours", 24))`.
pub fn hours(rule: &Value) -> R<f64> {
    match rule.get("hours") {
        None => Ok(24.0),
        Some(Value::Number(n)) => Ok(n.as_f64().unwrap_or(24.0)),
        Some(Value::Bool(b)) => Ok(if *b { 1.0 } else { 0.0 }),
        Some(Value::String(s)) => s
            .trim()
            .parse()
            .map_err(|_| format!("could not convert string to float: '{s}'")),
        Some(_) => Err("Invalid age".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_keeps_rules_and_is_idempotent() {
        let mut c = json!({"rules":[{"id":"a","path":"~/Downloads"}],"project_path":"~/Code"});
        migrate(&mut c);
        assert_eq!(c["rules"][0]["id"], "a");
        let active = active_profile(&c);
        let paths: Vec<String> = c["profiles"][&active]["locations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["path"].as_str().unwrap().to_string())
            .collect();
        assert!(paths.contains(&"~/Code".to_string()));
        let again = {
            let mut d = c.clone();
            migrate(&mut d);
            d
        };
        assert_eq!(again, c);
    }

    #[test]
    fn default_profile_is_only_downloads() {
        let mut c = json!({"rules":[],"profiles":{"default":{"locations":[{"id":"a","path":"~/Docker","kind":"docker"}]}}});
        migrate(&mut c);
        assert_eq!(
            c["profiles"]["default"]["locations"][0]["name"],
            "Downloads"
        );
        assert_eq!(
            c["profiles"][machine_profile_name()]["locations"][0]["path"],
            "~/Docker"
        );
    }

    #[test]
    fn add_location_and_paths() {
        let mut c = json!({"rules":[]});
        migrate(&mut c);
        add_location(&mut c, "Test", "projects", "~/Test").unwrap();
        let ps = location_paths(&c, "projects").unwrap();
        assert!(ps.iter().any(|p| p.file_name().unwrap() == "Test"));
        assert!(add_location(&mut c, "X", "nope", "~/x").is_err());
    }

    #[test]
    fn vars_and_user_expand() {
        std::env::set_var("NIRUFLOW_TEST_VAR", "abc");
        assert_eq!(
            expand_vars("$NIRUFLOW_TEST_VAR/x${NIRUFLOW_TEST_VAR}"),
            "abc/xabc"
        );
        assert_eq!(expand_vars("$NO_SUCH_VAR_X/y"), "$NO_SUCH_VAR_X/y");
        assert!(expand_user("~/a").ends_with("/a"));
    }

    #[test]
    fn save_is_private_and_keeps_previous() {
        let d = std::env::temp_dir().join(format!("niruflow-cfg-{}", std::process::id()));
        let f = d.join("config.json");
        let cfg = defaults();
        save_to(&f, &cfg).unwrap();
        save_to(&f, &cfg).unwrap();
        use std::os::unix::fs::MetadataExt;
        assert_eq!(std::fs::metadata(&f).unwrap().mode() & 0o777, 0o600);
        assert!(d.join("config.previous.json").exists());
        assert_eq!(load_from(&f).unwrap()["theme"], "NIRUX desktop");
        std::fs::remove_dir_all(d).unwrap();
    }
}
