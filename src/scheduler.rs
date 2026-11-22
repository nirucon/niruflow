//! The optional hourly systemd user timer that runs `niruflow run-enabled`.
use crate::config::{home, R};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const UNIT: &str = "niruflow.timer";

pub fn supported() -> bool {
    crate::docker::have("systemctl") && Path::new("/run/systemd/system").exists()
}

fn call(args: &[&str]) -> R<(i32, String, String)> {
    if !supported() {
        return Err("Systemd user services are unavailable on this machine".into());
    }
    let mut a = vec!["--user"];
    a.extend_from_slice(args);
    crate::docker::run_cmd("systemctl", &a, 12).map_err(|e| e.to_string())
}

fn unit_dir() -> PathBuf {
    home().join(".config/systemd/user")
}

pub fn status() -> R<Value> {
    if !supported() {
        return Ok(
            json!({"supported": false, "installed": false, "enabled": false, "active": false,
                         "detail": "Systemd user timer unavailable (e.g. Void/runit)"}),
        );
    }
    let installed = unit_dir().join(UNIT).exists();
    let enabled = call(&["is-enabled", UNIT])?;
    let active = call(&["is-active", UNIT])?;
    let detail = if active.1.trim().is_empty() {
        active.2.trim()
    } else {
        active.1.trim()
    };
    Ok(
        json!({"supported": true, "installed": installed, "enabled": enabled.0 == 0,
              "active": active.0 == 0, "detail": detail}),
    )
}

fn which(cmd: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(cmd))
        .find(|p| p.is_file())
}

pub fn install() -> R<Value> {
    if !supported() {
        return Err(
            "This scheduler requires systemd; CLI run-enabled works on other systems".into(),
        );
    }
    let exe = which("niruflow").unwrap_or_else(|| home().join(".local/bin/niruflow"));
    if !exe.exists() {
        return Err("Install NIRUFLOW first, so the CLI executable exists".into());
    }
    let dir = unit_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let service = dir.join("niruflow.service");
    let timer = dir.join(UNIT);
    if service.exists() || timer.exists() {
        return Err("Scheduler units already exist; inspect them before replacing".into());
    }
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    std::fs::write(
        &service,
        format!(
            "[Unit]\nDescription=NIRUFLOW scheduled rules\n[Service]\nType=oneshot\nExecStart={} run-enabled\nNoNewPrivileges=true\n",
            exe.display()
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(
        &timer,
        "[Unit]\nDescription=NIRUFLOW hourly scheduler\n[Timer]\nOnCalendar=hourly\nPersistent=true\nRandomizedDelaySec=3min\n[Install]\nWantedBy=timers.target\n",
    )
    .map_err(|e| e.to_string())?;
    for args in [&["daemon-reload"][..], &["enable", "--now", UNIT][..]] {
        let r = call(args)?;
        if r.0 != 0 {
            return Err(if r.2.trim().is_empty() {
                r.1.trim().into()
            } else {
                r.2.trim().into()
            });
        }
    }
    status()
}

pub fn set_enabled(enabled: bool) -> R<Value> {
    if !status()?["installed"].as_bool().unwrap_or(false) {
        return Err("Install scheduler first".into());
    }
    let r = call(&[if enabled { "enable" } else { "disable" }, "--now", UNIT])?;
    if r.0 != 0 {
        return Err(if r.2.trim().is_empty() {
            r.1.trim().into()
        } else {
            r.2.trim().into()
        });
    }
    status()
}
