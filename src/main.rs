//! NIRUFLOW: a local-first maintenance cockpit for the NIRU suite. The same
//! command line as 0.x (JSON on stdout), and an X11 window.
mod app;
mod backup;
mod canvas;
mod cleanup;
mod compose;
mod config;
mod docker;
mod fmt;
mod json;
mod logo;
mod media;
mod projects;
mod safety;
mod scheduler;
mod storage;
mod store;
mod text;
mod theme;
mod ui;
mod walk;
mod x;

use serde_json::{json, Value};
use std::sync::atomic::AtomicBool;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const USAGE: &str = "usage: niruflow [-h] [--version]
                {desktop,status,rules,history,docker,docker-cleanup-preview,profile,docker-containers,locations,media,compose,large-files,forgotten,doctor,projects,scan,run,restore,run-enabled,scheduler-status,backup-export,backup-import}
                ...";

const HELP: &str = "NIRUFLOW: local maintenance and automation

  niruflow                         the window (also: niruflow desktop)
  niruflow status                  version and rule counts
  niruflow rules                   configured rules
  niruflow scan RULE               dry run: what a rule would move
  niruflow run RULE [--apply] [--allow-synced]
  niruflow restore ID              restore a quarantined item from history
  niruflow history
  niruflow run-enabled             enabled automatic rules (what the timer runs)
  niruflow scheduler-status
  niruflow large-files
  niruflow forgotten [--months 12] [--limit 100]
  niruflow projects [--keep 2]
  niruflow media
  niruflow locations
  niruflow profile
  niruflow docker
  niruflow docker-containers [--short]
  niruflow docker-cleanup-preview [--days 30] [--include-unused]
  niruflow compose
  niruflow doctor
  niruflow backup-export DESTINATION
  niruflow backup-import SOURCE
  niruflow --version

Output is JSON (indent 2), so it pipes into jq.";

/// One option of a subcommand: name, takes an integer value, default.
struct Opt {
    name: &'static str,
    int: Option<i64>,
}

fn spec(cmd: &str) -> Option<(Vec<&'static str>, Vec<Opt>)> {
    let int = |name, d| Opt { name, int: Some(d) };
    let flag = |name| Opt { name, int: None };
    Some(match cmd {
        "desktop" | "status" | "rules" | "history" | "docker" | "profile" | "locations"
        | "media" | "compose" | "large-files" | "doctor" | "run-enabled" | "scheduler-status" => {
            (vec![], vec![])
        }
        "docker-cleanup-preview" => (vec![], vec![int("--days", 30), flag("--include-unused")]),
        "docker-containers" => (vec![], vec![flag("--short")]),
        "forgotten" => (vec![], vec![int("--months", 12), int("--limit", 100)]),
        "projects" => (vec![], vec![int("--keep", 2)]),
        "scan" => (vec!["rule"], vec![]),
        "run" => (vec!["rule"], vec![flag("--apply"), flag("--allow-synced")]),
        "restore" => (vec!["id"], vec![]),
        "backup-export" => (vec!["destination"], vec![]),
        "backup-import" => (vec!["source"], vec![]),
        _ => return None,
    })
}

#[derive(Default, Debug)]
struct Args {
    cmd: Option<String>,
    pos: Vec<String>,
    ints: Vec<(String, i64)>,
    flags: Vec<String>,
}

impl Args {
    fn int(&self, n: &str) -> i64 {
        self.ints
            .iter()
            .find(|(k, _)| k == n)
            .map(|x| x.1)
            .unwrap_or(0)
    }
    fn flag(&self, n: &str) -> bool {
        self.flags.iter().any(|f| f == n)
    }
}

fn usage_error(cmd: Option<&str>, msg: &str) -> ! {
    match cmd {
        Some(c) => eprintln!("usage: niruflow {c} [-h] ...\nniruflow {c}: error: {msg}"),
        None => eprintln!("{USAGE}\nniruflow: error: {msg}"),
    }
    std::process::exit(2);
}

fn parse_int(cmd: &str, what: &str, v: &str) -> i64 {
    v.trim().parse().unwrap_or_else(|_| {
        usage_error(
            Some(cmd),
            &format!("argument {what}: invalid int value: '{v}'"),
        )
    })
}

/// argparse-compatible parsing: `--opt V`, `--opt=V`, unique prefixes.
fn parse(argv: &[String]) -> Args {
    let mut a = Args::default();
    let mut i = 0;
    while i < argv.len() {
        let s = argv[i].as_str();
        match s {
            "--version" | "-V" if a.cmd.is_none() => {
                println!("NIRUFLOW {VERSION}");
                std::process::exit(0);
            }
            "-h" | "--help" => {
                println!("{HELP}");
                std::process::exit(0);
            }
            _ => {}
        }
        if a.cmd.is_none() {
            if s.starts_with('-') {
                usage_error(None, &format!("unrecognized arguments: {s}"));
            }
            if spec(s).is_none() {
                usage_error(
                    None,
                    &format!("argument cmd: invalid choice: '{s}' (choose from 'desktop', 'status', 'rules', 'history', 'docker', 'docker-cleanup-preview', 'profile', 'docker-containers', 'locations', 'media', 'compose', 'large-files', 'forgotten', 'doctor', 'projects', 'scan', 'run', 'restore', 'run-enabled', 'scheduler-status', 'backup-export', 'backup-import')"),
                );
            }
            a.cmd = Some(s.to_string());
            i += 1;
            continue;
        }
        let cmd = a.cmd.clone().unwrap();
        let (_, opts) = spec(&cmd).unwrap();
        if s.starts_with("--") && s.len() > 2 {
            let (name, inline) = s.split_once('=').map_or((s, None), |(n, v)| (n, Some(v)));
            let hits: Vec<&Opt> = opts.iter().filter(|o| o.name.starts_with(name)).collect();
            let opt = match hits.iter().find(|o| o.name == name) {
                Some(o) => *o,
                None if hits.len() == 1 => hits[0],
                None if hits.len() > 1 => {
                    usage_error(Some(&cmd), &format!("ambiguous option: {name}"))
                }
                None => usage_error(None, &format!("unrecognized arguments: {s}")),
            };
            match opt.int {
                Some(_) => {
                    let v = match inline {
                        Some(v) => v.to_string(),
                        None => {
                            i += 1;
                            argv.get(i).cloned().unwrap_or_else(|| {
                                usage_error(
                                    Some(&cmd),
                                    &format!("argument {}: expected one argument", opt.name),
                                )
                            })
                        }
                    };
                    let n = parse_int(&cmd, opt.name, &v);
                    a.ints.retain(|(k, _)| k != opt.name);
                    a.ints.push((opt.name.to_string(), n));
                }
                None => {
                    if inline.is_some() {
                        usage_error(
                            Some(&cmd),
                            &format!("argument {}: ignored explicit argument", opt.name),
                        );
                    }
                    a.flags.push(opt.name.to_string());
                }
            }
        } else if s == "--" {
            a.pos.extend(argv[i + 1..].iter().cloned());
            break;
        } else {
            a.pos.push(s.to_string());
        }
        i += 1;
    }
    if let Some(cmd) = a.cmd.clone() {
        let (pos, opts) = spec(&cmd).unwrap();
        for o in &opts {
            if let Some(d) = o.int {
                if !a.ints.iter().any(|(k, _)| k == o.name) {
                    a.ints.push((o.name.to_string(), d));
                }
            }
        }
        if a.pos.len() < pos.len() {
            usage_error(
                Some(&cmd),
                &format!(
                    "the following arguments are required: {}",
                    pos[a.pos.len()..].join(", ")
                ),
            );
        }
        if a.pos.len() > pos.len() {
            usage_error(
                None,
                &format!("unrecognized arguments: {}", a.pos[pos.len()..].join(" ")),
            );
        }
        if cmd == "restore" {
            parse_int(&cmd, "id", &a.pos[0]);
        }
    }
    a
}

fn print(v: &Value) {
    println!("{}", json::pretty(v, true));
}

fn print_utf8(v: &Value) {
    println!("{}", json::pretty(v, false));
}

fn doctor() -> Value {
    let cfg = config::config_file();
    let state = config::state_dir();
    json!({"python": "not required", "pyside6": "not required", "docker": docker::have("docker"),
           "systemctl": docker::have("systemctl"), "config": cfg.to_string_lossy(),
           "config_exists": cfg.exists(), "state_dir": state.to_string_lossy(),
           "quarantine_exists": state.join("quarantine").exists(), "message": "Read-only diagnostic",
           "niruflow": VERSION})
}

fn find_rule(cfg: &Value, id: &str) -> config::R<Value> {
    config::rules(cfg)
        .into_iter()
        .find(|r| r.get("id").and_then(Value::as_str) == Some(id))
        .ok_or_else(|| "Unknown rule".into())
}

fn run(a: &Args) -> config::R<()> {
    let cfg = config::load()?;
    let cmd = a.cmd.as_deref().unwrap_or("desktop");
    let no = AtomicBool::new(false);
    match cmd {
        "desktop" => {
            let started = std::time::Instant::now();
            app::launch(cfg, started).map_err(|e| e.to_string())?;
        }
        "status" => {
            let rules = config::rules(&cfg);
            let enabled = rules
                .iter()
                .filter(|r| config::truthy(r.get("enabled")))
                .count();
            print(&json!({"version": VERSION, "rules": rules.len(), "enabled": enabled}));
        }
        "rules" => print(cfg.get("rules").unwrap_or(&json!([]))),
        "history" => print(&Value::Array(
            store::history(100)?.iter().map(|x| x.to_json()).collect(),
        )),
        "doctor" => print(&doctor()),
        "forgotten" => {
            let rows = storage::find_old_files(
                &config::home(),
                a.int("--months"),
                a.int("--limit").clamp(0, 100_000) as usize,
                &no,
                None,
            )?;
            print(&Value::Array(rows.iter().map(|r| r.to_json()).collect()));
        }
        "media" => {
            let roots: Vec<String> = match cfg.get("media_roots").and_then(Value::as_array) {
                Some(a) => a
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
                None => media::default_roots(),
            };
            print(&media::inventory(&roots, 1_000_000, &no).to_json());
        }
        "locations" => print_utf8(&Value::Array(config::locations(&cfg, None))),
        "compose" => {
            let roots = config::location_paths(&cfg, "docker")?;
            print_utf8(&Value::Array(
                compose::discover(&roots, 4, 300)
                    .iter()
                    .map(|c| c.to_json())
                    .collect(),
            ));
        }
        "large-files" => {
            let roots = config::location_paths(&cfg, "storage")?;
            let rows = storage::largest(&roots, 100, &no, None);
            print_utf8(&Value::Array(rows.iter().map(|r| r.to_json()).collect()));
        }
        "docker-cleanup-preview" => print(
            &docker::Docker::default()
                .cleanup_preview(a.int("--days"), a.flag("--include-unused"))?,
        ),
        "docker" => print(&docker::Docker::default().inspect()?),
        "docker-containers" => {
            let data = docker::Docker::default().inspect()?;
            let containers = data["containers"].clone();
            if a.flag("--short") {
                let s = |x: &Value, k: &str| match x.get(k) {
                    Some(Value::String(s)) => s.clone(),
                    Some(v) => v.to_string(),
                    None => "?".into(),
                };
                for x in containers.as_array().into_iter().flatten() {
                    println!("{}\t{}\t{}", s(x, "Names"), s(x, "Status"), s(x, "Image"));
                }
            } else {
                print(&containers);
            }
        }
        "profile" => {
            let profiles: Vec<Value> = cfg
                .get("profiles")
                .and_then(Value::as_object)
                .map(|m| m.keys().map(|k| json!(k)).collect())
                .unwrap_or_default();
            print(
                &json!({"active": cfg.get("active_profile").cloned().unwrap_or(Value::Null), "profiles": profiles}),
            );
        }
        "projects" => {
            let mut out = Vec::new();
            for p in config::location_paths(&cfg, "projects")? {
                out.extend(
                    projects::analyze(&p, a.int("--keep"))?
                        .iter()
                        .map(|e| e.to_json()),
                );
            }
            print(&Value::Array(out));
        }
        "restore" => println!(
            "{}",
            cleanup::restore(a.pos[0].trim().parse().unwrap_or(0))?
        ),
        "scan" | "run" => {
            let rule = find_rule(&cfg, &a.pos[0])?;
            let candidates = cleanup::scan(&rule, None)?;
            if cmd == "run" && a.flag("--apply") {
                if !config::truthy(rule.get("enabled")) {
                    return Err("Rule is disabled".into());
                }
                let moved = cleanup::quarantine(&rule, &candidates, a.flag("--allow-synced"))?;
                print(&json!({"moved": moved}));
            } else {
                print(
                    &json!({"dry_run": true, "candidates": candidates.iter().map(|c| c.to_json()).collect::<Vec<_>>()}),
                );
            }
        }
        "scheduler-status" => print(&scheduler::status()?),
        "backup-export" => println!("{}", backup::export_config(&a.pos[0])?.display()),
        "backup-import" => {
            backup::import_config(&a.pos[0])?;
            println!("Configuration imported. Restart GUI to reload.");
        }
        "run-enabled" => {
            for rule in config::rules(&cfg) {
                if !(config::truthy(rule.get("enabled")) && config::truthy(rule.get("automatic"))) {
                    continue;
                }
                let id = config::str_of(&rule, "id");
                if safety::is_synced(std::path::Path::new(config::str_of(&rule, "path"))) {
                    println!("{id}: skipped synced path; manual confirmation required");
                    continue;
                }
                match cleanup::scan(&rule, None).and_then(|c| cleanup::quarantine(&rule, &c, false))
                {
                    Ok(moved) => println!("{id}: quarantined {}", moved.len()),
                    Err(e) => eprintln!("{id}: {e}"),
                }
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

extern "C" {
    fn tzset();
}

fn main() {
    unsafe { tzset() };
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let a = parse(&argv);
    if let Err(e) = run(&a) {
        eprintln!("NIRUFLOW: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn parses_like_argparse() {
        let a = parse(&v(&["forgotten", "--mon", "3", "--limit=5"]));
        assert_eq!((a.int("--months"), a.int("--limit")), (3, 5));
        let a = parse(&v(&["run", "downloads", "--apply"]));
        assert!(a.flag("--apply") && !a.flag("--allow-synced"));
        assert_eq!(a.pos, ["downloads"]);
        let a = parse(&v(&["docker-cleanup-preview"]));
        assert_eq!(a.int("--days"), 30);
        assert!(parse(&v(&[])).cmd.is_none());
    }
}
