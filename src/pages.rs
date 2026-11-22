//! The nine pages and their actions (the views of NIRUFLOW 0.x).
use super::*;
use crate::cleanup::{self, Candidate};
use crate::config::{self, str_of, truthy};
use crate::fmt::{human_size, DateStyle, DATE_PATTERNS, REGIONS, TIME_PATTERNS};
use crate::safety::is_synced;
use crate::storage::{self, FileRow};
use crate::ui::{Key, Row};
use serde_json::json;

const AGES: [(i64, &str); 12] = [
    (1, "1 hour"),
    (6, "6 hours"),
    (12, "12 hours"),
    (24, "24 hours"),
    (48, "2 days"),
    (72, "3 days"),
    (168, "7 days"),
    (336, "14 days"),
    (720, "30 days"),
    (2160, "90 days"),
    (4320, "180 days"),
    (8760, "365 days"),
];
const IMAGE_DAYS: [i64; 10] = [1, 3, 7, 10, 14, 30, 60, 90, 180, 365];
const MONTHS: [i64; 9] = [1, 3, 6, 12, 18, 24, 36, 60, 120];
const TYPES: [&str; 7] = [
    "All types",
    "Archives",
    "Video",
    "Audio",
    "Images",
    "Documents",
    "Other",
];
const SORTS: [&str; 5] = [
    "Largest first",
    "Smallest first",
    "Newest first",
    "Oldest first",
    "Name A–Z",
];
const ACTIONS: [(&str, &str); 5] = [
    ("start", "Start"),
    ("stop", "Stop"),
    ("restart", "Restart"),
    ("pause", "Pause"),
    ("unpause", "Resume"),
];

fn name_of(p: &str) -> String {
    Path::new(p)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_string())
}

fn parent_of(p: &str) -> String {
    Path::new(p)
        .parent()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn vstr(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(o) => o.to_string(),
    }
}

fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

impl App {
    fn style(&self) -> DateStyle {
        DateStyle {
            region: self
                .cfg
                .get("locale")
                .and_then(Value::as_str)
                .unwrap_or("sv_SE")
                .into(),
            date: self
                .cfg
                .get("date_format")
                .and_then(Value::as_str)
                .unwrap_or("yyyy-MM-dd")
                .into(),
            time: self
                .cfg
                .get("time_format")
                .and_then(Value::as_str)
                .unwrap_or("HH:mm")
                .into(),
            weekday: self
                .cfg
                .get("show_weekday")
                .and_then(Value::as_bool)
                .unwrap_or(true),
        }
    }

    fn rules(&self) -> Vec<Value> {
        config::rules(&self.cfg)
    }

    fn rule_mut(&mut self, idx: usize) -> Option<&mut serde_json::Map<String, Value>> {
        self.cfg
            .get_mut("rules")?
            .as_array_mut()?
            .get_mut(idx)?
            .as_object_mut()
    }

    pub(super) fn page_fields(&self) -> Vec<FieldId> {
        match self.page {
            3 => vec![FieldId::ProjectFilter],
            4 => match self.docker_tab {
                0 => vec![FieldId::ContainerSearch],
                1 => vec![FieldId::ImageSearch],
                _ => vec![],
            },
            5 => vec![FieldId::StorageSearch],
            6 => vec![FieldId::MediaRoots],
            _ => vec![],
        }
    }

    pub(super) fn page_tables(&self) -> Vec<TableId> {
        match self.page {
            0 => vec![TableId::Overview],
            1 => vec![TableId::Rules],
            2 => vec![TableId::Cleanup],
            3 => vec![TableId::Projects],
            4 => vec![
                [
                    TableId::Containers,
                    TableId::Images,
                    TableId::Compose,
                    TableId::ImageCleanup,
                ][self.docker_tab],
            ],
            5 => vec![TableId::Storage],
            6 => vec![TableId::Media],
            7 => vec![TableId::History],
            8 => vec![TableId::Locations],
            _ => vec![],
        }
    }

    pub(super) fn navigate(&mut self, i: usize) {
        if i >= PAGES.len() {
            return;
        }
        self.page = i;
        self.focus = None;
        match i {
            0 => self.refresh_overview(),
            1 => self.refresh_automations(),
            4 if self.docker_data.is_none() && !self.docker_loading => self.refresh_docker(),
            7 => self.refresh_history(),
            8 if !self.sched_loaded => self.refresh_scheduler(),
            _ => {}
        }
    }

    pub(super) fn refresh_page(&mut self) {
        match self.page {
            0 => self.refresh_overview(),
            1 => self.refresh_automations(),
            4 => self.refresh_docker(),
            7 => self.refresh_history(),
            8 => self.refresh_scheduler(),
            _ => {}
        }
    }

    // ------------------------------------------------------------ combos

    pub(super) fn combo_items(&self, id: ComboId) -> Vec<String> {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect();
        match id {
            ComboId::Theme => s(&theme::NAMES),
            ComboId::CleanupRule => self
                .rules()
                .iter()
                .map(|r| str_of(r, "id").to_string())
                .collect(),
            ComboId::CleanupAge => {
                let mut v: Vec<String> = AGES.iter().map(|(_, c)| c.to_string()).collect();
                if !AGES.iter().any(|(h, _)| *h == self.cl_hours) {
                    v.push(format!("{} hours (custom)", self.cl_hours));
                }
                v
            }
            ComboId::ContainerFilter => s(&["All statuses", "Running", "Stopped"]),
            ComboId::ImageAge => IMAGE_DAYS.iter().map(|d| format!("{d} days")).collect(),
            ComboId::StorageScope => s(&["Home", "Nextcloud", "Choose directory…"]),
            ComboId::StorageMonths => MONTHS.iter().map(|d| format!("{d} months")).collect(),
            ComboId::StorageType => s(&TYPES),
            ComboId::StorageSort => s(&SORTS),
            ComboId::Locale => s(&REGIONS),
            ComboId::DateFmt => s(&DATE_PATTERNS),
            ComboId::TimeFmt => s(&TIME_PATTERNS),
            ComboId::Profile => self
                .cfg
                .get("profiles")
                .and_then(Value::as_object)
                .map(|m| m.keys().cloned().collect())
                .unwrap_or_default(),
        }
    }

    pub(super) fn combo_index(&self, id: ComboId) -> usize {
        let items = self.combo_items(id);
        let find = |s: &str| items.iter().position(|x| x == s).unwrap_or(0);
        match id {
            ComboId::Theme => find(&self.theme_name()),
            ComboId::CleanupRule => self.cl_rule,
            ComboId::CleanupAge => AGES
                .iter()
                .position(|(h, _)| *h == self.cl_hours)
                .unwrap_or(AGES.len()),
            ComboId::ContainerFilter => self.container_filter,
            ComboId::ImageAge => IMAGE_DAYS
                .iter()
                .position(|d| *d == self.image_days)
                .unwrap_or(2),
            ComboId::StorageScope => self.st_scope,
            ComboId::StorageMonths => MONTHS
                .iter()
                .position(|d| *d == self.st_months)
                .unwrap_or(3),
            ComboId::StorageType => self.st_type,
            ComboId::StorageSort => self.st_sort,
            ComboId::Locale => find(&self.style().region),
            ComboId::DateFmt => find(&self.style().date),
            ComboId::TimeFmt => find(&self.style().time),
            ComboId::Profile => find(&config::active_profile(&self.cfg)),
        }
    }

    pub(super) fn combo_pick(&mut self, id: ComboId, i: usize) {
        let items = self.combo_items(id);
        let Some(item) = items.get(i).cloned() else {
            return;
        };
        match id {
            ComboId::Theme => {
                self.cfg["theme"] = json!(item);
                self.save_cfg();
                self.reload_theme();
            }
            ComboId::CleanupRule => {
                self.cl_rule = i;
                self.show_rule();
            }
            ComboId::CleanupAge => {
                if let Some((h, _)) = AGES.get(i) {
                    self.cl_hours = *h;
                }
            }
            ComboId::ContainerFilter => {
                self.container_filter = i;
                self.fill_containers();
            }
            ComboId::ImageAge => self.image_days = IMAGE_DAYS[i.min(IMAGE_DAYS.len() - 1)],
            ComboId::StorageScope => self.st_scope = i,
            ComboId::StorageMonths => self.st_months = MONTHS[i.min(MONTHS.len() - 1)],
            ComboId::StorageType => {
                self.st_type = i;
                self.filter_storage();
            }
            ComboId::StorageSort => {
                self.st_sort = i;
                self.filter_storage();
            }
            ComboId::Locale | ComboId::DateFmt | ComboId::TimeFmt => {
                let key = match id {
                    ComboId::Locale => "locale",
                    ComboId::DateFmt => "date_format",
                    _ => "time_format",
                };
                self.cfg[key] = json!(item);
                self.save_cfg();
                self.refresh_dates();
            }
            ComboId::Profile => {
                if self
                    .cfg
                    .get("profiles")
                    .and_then(|p| p.get(&item))
                    .is_some()
                {
                    self.cfg["active_profile"] = json!(item);
                    self.save_cfg();
                    self.refresh_locations();
                }
            }
        }
    }

    pub(super) fn toggle_check(&mut self, id: CheckId) {
        match id {
            CheckId::CleanupEnabled => self.cl_enabled = !self.cl_enabled,
            CheckId::IncludeUnused => self.include_unused = !self.include_unused,
            CheckId::Weekday => {
                let v = !self.style().weekday;
                self.cfg["show_weekday"] = json!(v);
                self.save_cfg();
                self.refresh_dates();
            }
        }
    }

    fn refresh_dates(&mut self) {
        self.fill_history();
        self.filter_storage();
        self.fill_cleanup();
        self.refresh_overview();
    }

    pub(super) fn field_changed(&mut self, f: FieldId) {
        match f {
            FieldId::ProjectFilter => self.filter_projects(),
            FieldId::ContainerSearch => self.fill_containers(),
            FieldId::ImageSearch => self.fill_images(),
            FieldId::StorageSearch => self.filter_storage(),
            FieldId::MediaRoots => {}
        }
    }

    pub(super) fn table_selection_changed(&mut self, _t: TableId) {}

    // ----------------------------------------------------------- buttons

    pub(super) fn button(&mut self, b: Btn) {
        match b {
            Btn::Close => self.try_close(),
            Btn::OverviewRefresh => self.refresh_overview(),
            Btn::RuleToggle => {
                if let Some(i) = self.selected_rule() {
                    if let Some(r) = self.rule_mut(i) {
                        let on = !truthy(r.get("enabled"));
                        r.insert("enabled".into(), json!(on));
                    }
                    self.save_cfg();
                    self.refresh_automations();
                    self.show_rule();
                }
            }
            Btn::RuleEdit => self.edit_rule(),
            Btn::RulePreview => {
                if let Some(i) = self.selected_rule() {
                    let rule = self.rules()[i].clone();
                    self.task(
                        false,
                        move |_| cleanup::scan(&rule, None),
                        |app, rows| {
                            app.info(
                                "Dry run",
                                &format!("{} candidate files. No changes made.", rows.len()),
                            );
                        },
                    );
                }
            }
            Btn::RuleRun => self.run_rule(),
            Btn::CleanupChooseDir => {
                let Some(rule) = self.rules().get(self.cl_rule).cloned() else {
                    return;
                };
                let start = PathBuf::from(config::expand_user(str_of(&rule, "path")));
                let idx = self.cl_rule;
                self.pick(
                    PickMode::Dir,
                    "Choose cleanup directory",
                    &start,
                    "",
                    move |app, p| {
                        if let Some(r) = app.rule_mut(idx) {
                            r.insert("path".into(), json!(p.to_string_lossy()));
                        }
                        app.save_cfg();
                        app.show_rule();
                    },
                );
            }
            Btn::CleanupSave => {
                let (idx, on, hours) = (self.cl_rule, self.cl_enabled, self.cl_hours);
                if let Some(r) = self.rule_mut(idx) {
                    r.insert("enabled".into(), json!(on));
                    r.insert("hours".into(), json!(hours));
                    self.save_cfg();
                    self.status = "Rule saved".into();
                }
            }
            Btn::CleanupAnalyze => self.cleanup_preview(),
            Btn::CleanupSelectAll => self.tables[TableId::Cleanup as usize].select_all(),
            Btn::CleanupQuarantine => self.apply_cleanup(),
            Btn::ProjectsAnalyze => {
                let paths = config::location_paths(&self.cfg, "projects");
                self.task(
                    false,
                    move |_| {
                        let mut out = Vec::new();
                        for p in paths? {
                            out.extend(crate::projects::analyze(&p, 2)?);
                        }
                        Ok(out)
                    },
                    |app, rows| app.projects_done(rows),
                );
            }
            Btn::DockerRefresh => self.refresh_docker(),
            Btn::DockerAction(i) => self.docker_action(i),
            Btn::DockerLogs => {
                let Some(c) = self.selected_container() else {
                    return;
                };
                let id = vstr(&c, "ID");
                self.task(
                    false,
                    move |_| crate::docker::Docker::default().logs(&id, 200),
                    |app, text| {
                        let tail: String = {
                            let chars: Vec<char> = text.chars().collect();
                            chars[chars.len().saturating_sub(25000)..].iter().collect()
                        };
                        let body = if tail.is_empty() {
                            "No information".to_string()
                        } else {
                            tail
                        };
                        app.info_detail("Container logs", "Container logs", &body);
                    },
                );
            }
            Btn::DockerMounts => {
                let Some(c) = self.selected_container() else {
                    return;
                };
                let id = vstr(&c, "ID");
                self.task(
                    false,
                    move |_| crate::docker::Docker::default().mounts(&id),
                    |app, data| {
                        let body = crate::json::pretty(&data, false);
                        app.info_detail("Persistent data mounts", "Persistent data mounts", &body);
                    },
                );
            }
            Btn::DockerRemove => self.docker_remove(),
            Btn::ComposeDiscover => {
                let roots = config::location_paths(&self.cfg, "docker");
                self.task(
                    false,
                    move |_| Ok(crate::compose::discover(&roots?, 4, 300)),
                    |app, rows| {
                        let n = rows.len();
                        app.compose_rows = rows;
                        let table: Vec<Row> = app
                            .compose_rows
                            .iter()
                            .enumerate()
                            .map(|(i, c)| {
                                Row::new(i, vec![c.project.clone(), c.path.clone(), c.root.clone()])
                            })
                            .collect();
                        app.tables[TableId::Compose as usize].set_rows(table);
                        app.status = format!("{n} Compose files discovered");
                    },
                );
            }
            Btn::ImagesPreview => {
                self.image_plan = None;
                self.tables[TableId::ImageCleanup as usize].clear();
                let (days, include) = (self.image_days, self.include_unused);
                self.image_info = "Scanning Docker images and container references...".into();
                self.task(
                    false,
                    move |_| crate::docker::Docker::default().cleanup_preview(days, include),
                    |app, plan| app.image_preview_done(plan),
                );
            }
            Btn::ImagesDelete => self.delete_images(),
            Btn::ImagesSelectAll => self.tables[TableId::ImageCleanup as usize].select_all(),
            Btn::StorageOld => self.storage_old(),
            Btn::StorageLargest => {
                let roots = config::location_paths(&self.cfg, "storage");
                self.storage_scan(
                    move |ctx| {
                        let roots = roots?;
                        let p = progress_fn(ctx);
                        Ok(storage::largest(&roots, 500, &ctx.cancel, Some(&p)))
                    },
                    |app, rows| app.storage_done(rows, "largest files"),
                );
            }
            Btn::StorageDup => {
                let roots = config::location_paths(&self.cfg, "storage");
                self.storage_scan(
                    move |ctx| {
                        let roots = roots?;
                        let p = progress_fn(ctx);
                        Ok(storage::duplicates(&roots, 50, &ctx.cancel, Some(&p)))
                    },
                    |app, groups| app.duplicates_done(groups),
                );
            }
            Btn::StorageCancel => {
                if let Some(c) = &self.st_cancel {
                    c.store(true, Ordering::Relaxed);
                    self.st_info = "Cancelling scan safely…".into();
                }
            }
            Btn::StorageOpenDir => {
                if let Some(r) = self.storage_selected().first() {
                    open_folder(Path::new(&parent_of(&r.path)));
                }
            }
            Btn::StorageSelectVisible => self.tables[TableId::Storage as usize].select_all(),
            Btn::StorageQuarantine => self.storage_move(),
            Btn::StorageDelete => self.storage_delete(),
            Btn::MediaAdd => {
                self.pick(
                    PickMode::Dir,
                    "Choose media directory",
                    &config::home(),
                    "",
                    |app, p| {
                        let f = &mut app.fields[FieldId::MediaRoots as usize];
                        let mut roots: Vec<String> = f
                            .text
                            .split(';')
                            .map(|x| x.trim().to_string())
                            .filter(|x| !x.is_empty())
                            .collect();
                        let p = p.to_string_lossy().into_owned();
                        if !roots.contains(&p) {
                            roots.push(p);
                        }
                        f.set(&roots.join("; "));
                    },
                );
            }
            Btn::MediaScan => {
                let roots: Vec<String> = self.fields[FieldId::MediaRoots as usize]
                    .text
                    .split(';')
                    .map(|x| x.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .collect();
                self.cfg["media_roots"] = json!(roots);
                self.save_cfg();
                self.media_summary = "Scanning...".into();
                self.task(
                    false,
                    move |ctx| Ok(crate::media::inventory(&roots, 1_000_000, &ctx.cancel)),
                    |app, inv| app.media_done(inv),
                );
            }
            Btn::HistoryRefresh => self.refresh_history(),
            Btn::HistoryRestore => self.restore_selected(),
            Btn::SafetyPreview => {
                let lines: Vec<String> = self
                    .preview_rules()
                    .iter()
                    .map(|(name, path, status)| format!("{name}: {status}, {path}"))
                    .collect();
                let text = if lines.is_empty() {
                    "No automation rules configured.".to_string()
                } else {
                    lines.join("\n")
                };
                self.info("Automation safety preview", &text);
            }
            Btn::SchedRefresh => self.refresh_scheduler(),
            Btn::SchedInstall => self.confirm(
                "Install scheduler",
                "Install an hourly systemd user timer? Existing rules will not change.",
                |app| {
                    app.task(
                        false,
                        |_| crate::scheduler::install(),
                        |app, v| app.sched_done(v),
                    );
                },
            ),
            Btn::SchedEnable | Btn::SchedPause => {
                let on = b == Btn::SchedEnable;
                self.task(
                    false,
                    move |_| crate::scheduler::set_enabled(on),
                    |app, v| app.sched_done(v),
                );
            }
            Btn::BackupExport => {
                self.pick(
                    PickMode::SaveZip,
                    "Export configuration",
                    &config::home(),
                    "niruflow-config-backup.zip",
                    |app, p| match crate::backup::export_config(&p.to_string_lossy()) {
                        Ok(d) => app.info(
                            "Backup created",
                            &format!("The configuration was exported to:\n{}", d.display()),
                        ),
                        Err(e) => app.info("Export failed", &e),
                    },
                );
            }
            Btn::BackupImport => {
                self.pick(PickMode::OpenZip, "Import configuration", &config::home(), "", |app, p| {
                    app.confirm(
                        "Confirm import",
                        "Import replaces profiles, rules and settings. The current configuration is backed up first. Continue?",
                        move |app| match crate::backup::import_config(&p.to_string_lossy()) {
                            Ok(_) => app.info(
                                "Import complete",
                                "Configuration imported. Restart NIRUFLOW to load the updated profiles and rules.",
                            ),
                            Err(e) => app.info("Import failed", &e),
                        },
                    );
                });
            }
            Btn::NewProfile => {
                self.ask_text("New profile", "Profile name", "", |app, name| {
                    let name = name.trim().to_string();
                    if name.is_empty() {
                        return;
                    }
                    let key = name.to_lowercase().replace(' ', "-");
                    if app.cfg.get("profiles").and_then(|p| p.get(&key)).is_some() {
                        app.info("Already exists", "Profile name is already in use");
                        return;
                    }
                    if let Some(p) = app.cfg.get_mut("profiles").and_then(Value::as_object_mut) {
                        p.insert(key.clone(), json!({"name": name, "locations": []}));
                    }
                    app.cfg["active_profile"] = json!(key);
                    app.save_cfg();
                    app.refresh_locations();
                });
            }
            Btn::LocAdd => {
                self.ask_text("New location", "Name", "", |app, name| {
                    if name.trim().is_empty() {
                        return;
                    }
                    app.ask_choice(
                        "Location type",
                        "Select the purpose of this location",
                        &["projects", "docker", "storage"],
                        move |app, i| {
                            let kind = ["projects", "docker", "storage"][i.min(2)];
                            app.pick(
                                PickMode::Dir,
                                "Choose directory",
                                &config::home(),
                                "",
                                move |app, p| match config::add_location(
                                    &mut app.cfg,
                                    &name,
                                    kind,
                                    &p.to_string_lossy(),
                                ) {
                                    Ok(_) => {
                                        app.save_cfg();
                                        app.refresh_locations();
                                    }
                                    Err(e) => app.info("Error", &e),
                                },
                            );
                        },
                    );
                });
            }
            Btn::LocEdit => {
                let Some(i) = self.selected_location() else {
                    return;
                };
                let cur = config::locations(&self.cfg, None)[i].clone();
                let start = PathBuf::from(config::expand_user(str_of(&cur, "path")));
                self.pick(
                    PickMode::Dir,
                    "Choose directory",
                    &start,
                    "",
                    move |app, p| {
                        if let Some(l) =
                            config::profile_locations_mut(&mut app.cfg).and_then(|v| v.get_mut(i))
                        {
                            l["path"] = json!(p.to_string_lossy());
                        }
                        app.save_cfg();
                        app.refresh_locations();
                    },
                );
            }
            Btn::LocToggle => {
                let Some(i) = self.selected_location() else {
                    return;
                };
                if let Some(l) =
                    config::profile_locations_mut(&mut self.cfg).and_then(|v| v.get_mut(i))
                {
                    let on = l.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                    l["enabled"] = json!(!on);
                }
                self.save_cfg();
                self.refresh_locations();
            }
            Btn::LocRemove => {
                let Some(i) = self.selected_location() else {
                    return;
                };
                self.confirm(
                    "Remove location",
                    "Remove this location from the configuration? No files will be deleted.",
                    move |app| {
                        if let Some(v) = config::profile_locations_mut(&mut app.cfg) {
                            if i < v.len() {
                                v.remove(i);
                            }
                        }
                        app.save_cfg();
                        app.refresh_locations();
                    },
                );
            }
        }
    }

    // -------------------------------------------------------- overview

    pub(super) fn refresh_overview(&mut self) {
        let rules = self.rules();
        let enabled = rules.iter().filter(|r| truthy(r.get("enabled"))).count();
        let rows = crate::store::history(10).unwrap_or_default();
        self.ov_status = format!(
            "Automations: {enabled}/{} enabled    ·    Recent actions: {}    ·    Synced folders: manual review",
            rules.len(),
            rows.len()
        );
        self.ov_disk = match disk_usage(&config::home()) {
            Some((total, used, free)) => format!(
                "Home filesystem    ·    Free: {}    ·    Used: {}    ·    Capacity: {}",
                human_size(free as f64),
                human_size(used as f64),
                human_size(total as f64)
            ),
            None => "Filesystem usage unavailable".into(),
        };
        let st = self.style();
        let table: Vec<Row> = rows
            .iter()
            .enumerate()
            .map(|(i, a)| {
                Row::new(
                    i,
                    vec![
                        st.iso(&a.ts),
                        a.rule.clone(),
                        a.status.clone(),
                        name_of(&a.source),
                    ],
                )
            })
            .collect();
        self.ov_empty = if rows.is_empty() {
            "No recent actions. Run a preview in File Cleanup, Storage or Docker to get started."
                .into()
        } else {
            String::new()
        };
        self.tables[TableId::Overview as usize].set_rows(table);
        self.dirty = true;
    }

    // ------------------------------------------------------ automations

    pub(super) fn refresh_automations(&mut self) {
        let rows: Vec<Row> = self
            .rules()
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let hours = r
                    .get("hours")
                    .map(|h| crate::json::pretty(h, false))
                    .unwrap_or_else(|| "24".into());
                Row::new(
                    i,
                    vec![
                        str_of(r, "id").into(),
                        str_of(r, "path").into(),
                        format!("{hours} h"),
                        if truthy(r.get("enabled")) {
                            "Yes"
                        } else {
                            "No"
                        }
                        .into(),
                        if is_synced(Path::new(str_of(r, "path"))) {
                            "Manual"
                        } else {
                            "Allowed"
                        }
                        .into(),
                    ],
                )
            })
            .collect();
        self.tables[TableId::Rules as usize].set_rows(rows);
    }

    fn selected_rule(&self) -> Option<usize> {
        self.tables[TableId::Rules as usize].first_selected()
    }

    fn edit_rule(&mut self) {
        let Some(i) = self.selected_rule() else {
            return;
        };
        let rule = self.rules()[i].clone();
        let current = config::hours(&rule).unwrap_or(24.0) as i64;
        self.ask_age(current.max(1), move |app, hours| {
            if let Some(r) = app.rule_mut(i) {
                r.insert("hours".into(), json!(hours));
            }
            app.save_cfg();
            app.refresh_automations();
            app.confirm("Directory", "Change the monitored directory?", move |app| {
                let start = PathBuf::from(config::expand_user(str_of(&rule, "path")));
                app.pick(
                    PickMode::Dir,
                    "Select directory",
                    &start,
                    "",
                    move |app, p| {
                        if let Some(r) = app.rule_mut(i) {
                            r.insert("path".into(), json!(p.to_string_lossy()));
                        }
                        app.save_cfg();
                        app.refresh_automations();
                        app.show_rule();
                    },
                );
            });
        });
    }

    fn run_rule(&mut self) {
        let Some(i) = self.selected_rule() else {
            return;
        };
        let rule = self.rules()[i].clone();
        if is_synced(Path::new(str_of(&rule, "path"))) {
            return self.info(
                "Synced directory",
                "Automatic actions in synced folders are blocked. Use manual File Cleanup review.",
            );
        }
        if !truthy(rule.get("enabled")) {
            return self.info("Disabled", "Enable this rule first.");
        }
        let msg = format!(
            "Move eligible files from {} into quarantine?",
            str_of(&rule, "path")
        );
        self.confirm("Confirm quarantine", &msg, move |app| {
            app.task(
                false,
                move |_| {
                    let c = cleanup::scan(&rule, None)?;
                    cleanup::quarantine(&rule, &c, false)
                },
                |app, moved| {
                    app.refresh_automations();
                    app.info(
                        "Completed",
                        &format!("{} files moved to quarantine.", moved.len()),
                    );
                },
            );
        });
    }

    fn preview_rules(&self) -> Vec<(String, String, String)> {
        self.rules()
            .iter()
            .map(|r| {
                let target = PathBuf::from(config::expand_user(str_of(r, "path")));
                let enabled = truthy(r.get("enabled"));
                let status = if !enabled {
                    "disabled"
                } else if is_synced(&target) {
                    "blocked: synced location"
                } else if !target.is_dir() {
                    "unavailable directory"
                } else if !truthy(r.get("automatic")) {
                    "manual only"
                } else {
                    "eligible for scheduler"
                };
                let name = r
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("Unnamed rule");
                (
                    name.to_string(),
                    target.to_string_lossy().into_owned(),
                    status.to_string(),
                )
            })
            .collect()
    }

    // ---------------------------------------------------------- cleanup

    pub(super) fn show_rule(&mut self) {
        let rules = self.rules();
        if rules.is_empty() {
            self.cl_info = "No cleanup rules configured.".into();
            return;
        }
        self.cl_rule = self.cl_rule.min(rules.len() - 1);
        let r = &rules[self.cl_rule];
        self.cl_enabled = truthy(r.get("enabled"));
        self.cl_hours = config::hours(r).unwrap_or(24.0) as i64;
        self.candidates.clear();
        self.tables[TableId::Cleanup as usize].clear();
        self.cl_info = format!(
            "Directory: {}  ·  Run a scan to show candidates",
            str_of(r, "path")
        );
    }

    fn cleanup_preview(&mut self) {
        let Some(mut rule) = self.rules().get(self.cl_rule).cloned() else {
            return;
        };
        rule["hours"] = json!(self.cl_hours);
        self.task(
            false,
            move |_| cleanup::scan(&rule, None),
            |app, rows| app.cleanup_done(rows),
        );
    }

    fn cleanup_done(&mut self, rows: Vec<Candidate>) {
        let total: u64 = rows.iter().map(|c| c.bytes).sum();
        self.cl_info = format!(
            "{} candidates · {:.1} MiB · preview only",
            rows.len(),
            total as f64 / 1048576.0
        );
        self.candidates = rows;
        self.fill_cleanup();
        self.status = "Analysis complete; no files changed".into();
    }

    fn fill_cleanup(&mut self) {
        let st = self.style();
        let rows: Vec<Row> = self
            .candidates
            .iter()
            .enumerate()
            .map(|(i, c)| {
                Row::keyed(
                    i,
                    vec![
                        name_of(&c.path),
                        human_size(c.bytes as f64),
                        st.timestamp(c.mtime),
                        c.path.clone(),
                    ],
                    vec![
                        Key::S(name_of(&c.path)),
                        Key::N(c.bytes as f64),
                        Key::N(c.mtime),
                        Key::S(c.path.clone()),
                    ],
                )
            })
            .collect();
        self.tables[TableId::Cleanup as usize].set_rows(rows);
    }

    fn apply_cleanup(&mut self) {
        let Some(rule) = self.rules().get(self.cl_rule).cloned() else {
            return;
        };
        if !truthy(rule.get("enabled")) {
            return self.info("Rule is disabled", "Enable and save the rule first.");
        }
        let chosen: Vec<Candidate> = self.tables[TableId::Cleanup as usize]
            .selected()
            .into_iter()
            .filter_map(|i| self.candidates.get(i).cloned())
            .collect();
        if chosen.is_empty() {
            return self.info("Nothing selected", "Select files in the table first.");
        }
        let synced = is_synced(Path::new(str_of(&rule, "path")));
        let mut msg = format!("Move {} selected files to local quarantine?", chosen.len());
        if synced {
            msg.push_str("\n\nWARNING: a sync client may propagate this move as a deletion to other devices.");
        }
        self.confirm(
            "Confirm action",
            &msg,
            move |app| match cleanup::quarantine(&rule, &chosen, synced) {
                Ok(moved) => {
                    app.status = format!("{} files moved to quarantine", moved.len());
                    app.cleanup_preview();
                }
                Err(e) => app.info("Action failed", &e),
            },
        );
    }

    // --------------------------------------------------------- projects

    fn projects_done(&mut self, rows: Vec<crate::projects::Entry>) {
        let (total, protected, review) = crate::projects::insights(&rows);
        self.projects_status = if rows.is_empty() {
            "No project artifacts found in configured directories.".into()
        } else {
            format!("{total} artifacts · {protected} protected · {review} to review · read-only")
        };
        let n = rows.len();
        self.project_rows = rows;
        self.filter_projects();
        self.status = format!("{n} project artifacts inventoried");
    }

    fn filter_projects(&mut self) {
        let q = self.fields[FieldId::ProjectFilter as usize]
            .text
            .to_lowercase();
        let rows: Vec<Row> = self
            .project_rows
            .iter()
            .enumerate()
            .filter(|(_, x)| {
                format!("{} {}", x.project, x.name)
                    .to_lowercase()
                    .contains(&q)
            })
            .map(|(i, x)| {
                let ft = if x.class.from.is_empty() {
                    String::new()
                } else {
                    format!("{} → {}", x.class.from, x.class.to)
                };
                Row::new(
                    i,
                    vec![
                        x.project.clone(),
                        x.name.clone(),
                        x.class.kind.clone(),
                        x.class.version.clone(),
                        ft,
                        x.status.clone(),
                    ],
                )
            })
            .collect();
        self.tables[TableId::Projects as usize].set_rows(rows);
    }

    // ----------------------------------------------------------- docker

    fn refresh_docker(&mut self) {
        self.docker_loading = true;
        self.task(
            false,
            |_| crate::docker::Docker::default().inspect(),
            |app, data| app.docker_done(data),
        );
    }

    fn docker_done(&mut self, data: Value) {
        let s = &data["summary"];
        self.docker_summary = format!(
            "Context: {} · {} containers · {} running · {} unhealthy · {} images",
            vstr(&data, "context"),
            s["containers"],
            s["running"],
            s["unhealthy"],
            s["images"]
        );
        self.docker_disk = vstr(&data, "disk");
        self.docker_data = Some(data);
        self.docker_loading = false;
        self.fill_containers();
        self.fill_images();
        self.status = "Docker status updated · no changes made".into();
    }

    fn containers(&self) -> Vec<Value> {
        self.docker_data
            .as_ref()
            .and_then(|d| d.get("containers"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }

    fn fill_containers(&mut self) {
        let q = self.fields[FieldId::ContainerSearch as usize]
            .text
            .to_lowercase();
        let filter = self.container_filter;
        let rows: Vec<Row> = self
            .containers()
            .iter()
            .enumerate()
            .filter(|(_, x)| {
                let (name, image, status) = (vstr(x, "Names"), vstr(x, "Image"), vstr(x, "Status"));
                let hay = format!("{name} {image} {status}").to_lowercase();
                let up = status.to_lowercase().starts_with("up");
                hay.contains(&q) && !(filter == 1 && !up) && !(filter == 2 && up)
            })
            .map(|(i, x)| {
                let status = vstr(x, "Status");
                let health = if status.to_lowercase().contains("unhealthy") {
                    "Unhealthy"
                } else {
                    ""
                };
                Row::new(
                    i,
                    vec![
                        vstr(x, "Names"),
                        vstr(x, "Image"),
                        status.clone(),
                        health.into(),
                        vstr(x, "Size"),
                    ],
                )
            })
            .collect();
        self.tables[TableId::Containers as usize].set_rows(rows);
    }

    fn fill_images(&mut self) {
        let q = self.fields[FieldId::ImageSearch as usize]
            .text
            .to_lowercase();
        let images = self
            .docker_data
            .as_ref()
            .and_then(|d| d.get("images"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let rows: Vec<Row> = images
            .iter()
            .enumerate()
            .filter(|(_, x)| {
                format!("{} {}", vstr(x, "Repository"), vstr(x, "Tag"))
                    .to_lowercase()
                    .contains(&q)
            })
            .map(|(i, x)| {
                Row::new(
                    i,
                    vec![
                        vstr(x, "Repository"),
                        vstr(x, "Tag"),
                        vstr(x, "ID"),
                        vstr(x, "Size"),
                    ],
                )
            })
            .collect();
        self.tables[TableId::Images as usize].set_rows(rows);
    }

    fn selected_container(&self) -> Option<Value> {
        let i = self.tables[TableId::Containers as usize].first_selected()?;
        self.containers().get(i).cloned()
    }

    fn docker_action(&mut self, i: usize) {
        let Some(c) = self.selected_container() else {
            return self.info("Select container", "Select a container first.");
        };
        let id = vstr(&c, "ID");
        if id.is_empty() {
            return self.info(
                "Missing ID",
                "Cannot perform the action without a container ID.",
            );
        }
        let action = ACTIONS[i].0;
        let short: String = id.chars().take(16).collect();
        let msg = format!(
            "{} on {}?\n\nContainer-ID: {short}\nThis action may affect service availability.",
            action.to_uppercase(),
            vstr(&c, "Names")
        );
        self.confirm("Confirm Docker action", &msg, move |app| {
            app.task(
                false,
                move |_| crate::docker::Docker::default().container_action(&id, action),
                |app, _| app.refresh_docker(),
            );
        });
    }

    fn docker_remove(&mut self) {
        let Some(c) = self.selected_container() else {
            return self.info("Select container", "Select a stopped container first.");
        };
        if !["exited", "created", "dead"].contains(&vstr(&c, "State").to_lowercase().as_str()) {
            return self.info(
                "Container is not stopped",
                "Only stopped containers can be removed. Running containers must be stopped separately.",
            );
        }
        let id = vstr(&c, "ID");
        let context = self
            .docker_data
            .as_ref()
            .map(|d| vstr(d, "context"))
            .unwrap_or_default();
        let short: String = id.chars().take(16).collect();
        let msg = format!(
            "Remove stopped container {}?\nID: {short}\nContext: {context}\n\nThe container will be permanently removed. Volumes are NOT removed, but changes inside its writable layer will be lost.\nCompose may recreate the container later.\n\nType REMOVE CONTAINER:",
            vstr(&c, "Names")
        );
        self.ask_text(
            "Remove container permanently",
            &msg,
            "",
            move |app, answer| {
                if answer != "REMOVE CONTAINER" {
                    return;
                }
                app.task(
                    false,
                    move |_| {
                        crate::docker::Docker::default().container_remove(
                            &id,
                            &context,
                            "REMOVE CONTAINER",
                        )
                    },
                    |app, _| app.refresh_docker(),
                );
            },
        );
    }

    fn image_preview_done(&mut self, plan: Value) {
        let cands = plan["candidates"].as_array().cloned().unwrap_or_default();
        let rows: Vec<Row> = cands
            .iter()
            .enumerate()
            .map(|(i, x)| {
                let tags: Vec<String> = x["tags"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                let tags = if tags.is_empty() {
                    "<dangling>".to_string()
                } else {
                    tags.join(", ")
                };
                let created: String = vstr(x, "created").chars().take(10).collect();
                let bytes = x["bytes"].as_f64().unwrap_or(0.0);
                Row::keyed(
                    i,
                    vec![
                        vstr(x, "id"),
                        tags.clone(),
                        created.clone(),
                        human_size(bytes),
                    ],
                    vec![
                        Key::S(vstr(x, "id")),
                        Key::S(tags),
                        Key::S(created),
                        Key::N(bytes),
                    ],
                )
            })
            .collect();
        self.tables[TableId::ImageCleanup as usize].set_rows(rows);
        let e = &plan["excluded"];
        self.image_info = format!(
            "{} candidates · {} summed image sizes (NOT recoverable space) · {} referenced by containers · {} newer than cutoff · {} tagged excluded · {} unknown · context: {}",
            plan["count"],
            human_size(plan["apparent_bytes"].as_f64().unwrap_or(0.0)),
            e["in_use"],
            e["too_new"],
            e["tagged_opt_out"],
            e["invalid"],
            vstr(&plan, "context")
        );
        self.image_plan = Some(plan);
    }

    fn delete_images(&mut self) {
        let Some(plan) = self.image_plan.clone() else {
            return self.info("Preview required", "Run Preview cleanup first.");
        };
        if plan["age_days"].as_i64() != Some(self.image_days)
            || plan["include_unused"].as_bool() != Some(self.include_unused)
        {
            return self.info("Policy changed", "Preview again after changing policy.");
        }
        let cands = plan["candidates"].as_array().cloned().unwrap_or_default();
        let ids: Vec<String> = self.tables[TableId::ImageCleanup as usize]
            .selected()
            .into_iter()
            .filter_map(|i| cands.get(i).map(|c| vstr(c, "id")))
            .collect();
        if ids.is_empty() {
            return;
        }
        let msg = format!(
            "Delete {} selected images? This may remove rollback images.\nNo containers or volumes are removed.\nType DELETE IMAGES:",
            ids.len()
        );
        self.ask_text("Delete Docker images", &msg, "", move |app, answer| {
            if answer != "DELETE IMAGES" {
                return;
            }
            app.task(
                false,
                move |_| crate::docker::Docker::default().cleanup_apply(&plan, &ids, &answer),
                |app, r| app.images_deleted(r),
            );
        });
    }

    fn images_deleted(&mut self, r: Value) {
        self.image_plan = None;
        self.tables[TableId::ImageCleanup as usize].clear();
        let removed = r["removed"].as_array().map(Vec::len).unwrap_or(0);
        let errors = r["errors"].as_array().cloned().unwrap_or_default();
        self.image_info = format!(
            "Removed {removed} images ({} tags); errors: {}. Refresh and preview again.",
            r["removed_tags"],
            errors.len()
        );
        self.refresh_docker();
        if !errors.is_empty() {
            let mut cats: Vec<(&str, usize)> = Vec::new();
            for e in &errors {
                let msg = vstr(e, "error").to_lowercase();
                let cat = if msg.contains("container") {
                    "Container reference"
                } else if msg.contains("multiple repositories") {
                    "Tag/repository conflict"
                } else {
                    "Changed or unavailable"
                };
                match cats.iter_mut().find(|(c, _)| *c == cat) {
                    Some((_, n)) => *n += 1,
                    None => cats.push((cat, 1)),
                }
            }
            let text = format!(
                "Removed {removed} images and {} tags.\n{} images could not be removed.\n{}",
                r["removed_tags"],
                errors.len(),
                cats.iter()
                    .map(|(c, n)| format!("{c}: {n}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            let detail = errors
                .iter()
                .map(|e| format!("{}: {}", vstr(e, "id"), vstr(e, "error")))
                .collect::<Vec<_>>()
                .join("\n\n");
            self.info_detail("Docker cleanup results", &text, &detail);
        }
    }

    // ---------------------------------------------------------- storage

    fn storage_scan<T: Send + 'static>(
        &mut self,
        task: impl FnOnce(&Ctx) -> R<T> + Send + 'static,
        done: impl FnOnce(&mut App, T) + Send + 'static,
    ) {
        if self.st_cancel.is_some() {
            self.st_info = "A scan is already running. Cancel it before starting another.".into();
            return;
        }
        self.st_info = "Starting scan… no files are changed".into();
        self.task(true, task, done);
    }

    fn storage_old(&mut self) {
        if self.st_cancel.is_some() {
            return;
        }
        let months = self.st_months;
        let go = move |app: &mut App, root: PathBuf| {
            app.storage_scan(
                move |ctx| {
                    let p = progress_fn(ctx);
                    storage::find_old_files(&root, months, 300, &ctx.cancel, Some(&p))
                },
                |app, rows| app.storage_done(rows, "old files"),
            );
        };
        match self.st_scope {
            0 => go(self, config::home()),
            1 => go(self, config::home().join("Nextcloud")),
            _ => self.pick(PickMode::Dir, "Choose directory", &config::home(), "", go),
        }
    }

    fn duplicates_done(&mut self, groups: Vec<storage::DupGroup>) {
        // Every copy is listed on its own; nothing is deduplicated.
        let mut rows = Vec::new();
        for g in &groups {
            for p in &g.paths {
                if let Ok(md) = std::fs::metadata(p) {
                    rows.push(FileRow::of(p.clone(), &md));
                }
            }
        }
        self.storage_done(rows, "duplicate copies (review carefully)");
        self.st_mode = "duplicates".into();
        self.filter_storage();
    }

    fn storage_done(&mut self, rows: Vec<FileRow>, mode: &str) {
        self.st_mode = mode.into();
        self.st_rows = rows
            .into_iter()
            .map(|r| {
                let cat = storage::file_category(&r.path);
                (r, cat)
            })
            .collect();
        self.filter_storage();
    }

    pub(super) fn filter_storage(&mut self) {
        let q = self.fields[FieldId::StorageSearch as usize]
            .text
            .to_lowercase();
        let kind = TYPES[self.st_type];
        let mut idx: Vec<usize> = (0..self.st_rows.len())
            .filter(|&i| {
                let (r, cat) = &self.st_rows[i];
                r.path.to_lowercase().contains(&q) && (kind == "All types" || *cat == kind)
            })
            .collect();
        let rows = &self.st_rows;
        match self.st_sort {
            0 => idx.sort_by(|a, b| rows[*b].0.bytes.cmp(&rows[*a].0.bytes)),
            1 => idx.sort_by(|a, b| rows[*a].0.bytes.cmp(&rows[*b].0.bytes)),
            2 => idx.sort_by(|a, b| rows[*b].0.mtime.total_cmp(&rows[*a].0.mtime)),
            3 => idx.sort_by(|a, b| rows[*a].0.mtime.total_cmp(&rows[*b].0.mtime)),
            _ => idx.sort_by_key(|i| name_of(&rows[*i].0.path).to_lowercase()),
        }
        let st = self.style();
        let total: u64 = idx.iter().map(|i| self.st_rows[*i].0.bytes).sum();
        let table: Vec<Row> = idx
            .iter()
            .map(|&i| {
                let (r, cat) = &self.st_rows[i];
                Row::new(
                    i,
                    vec![
                        name_of(&r.path),
                        human_size(r.bytes as f64),
                        st.timestamp(r.mtime),
                        cat.to_string(),
                        parent_of(&r.path),
                    ],
                )
            })
            .collect();
        self.st_info = format!(
            "Showing {} of {} scanned files · {} displayed total · {} · no changes",
            idx.len(),
            self.st_rows.len(),
            human_size(total as f64),
            self.st_mode
        );
        self.st_display = idx;
        self.tables[TableId::Storage as usize].set_rows(table);
    }

    fn storage_selected(&self) -> Vec<FileRow> {
        self.tables[TableId::Storage as usize]
            .selected()
            .into_iter()
            .filter_map(|i| self.st_rows.get(i).map(|r| r.0.clone()))
            .collect()
    }

    fn storage_move(&mut self) {
        let rows = self.storage_selected();
        if rows.is_empty() {
            return;
        }
        if self.st_mode == "duplicates" {
            return self.info(
                "Duplicates",
                "Review individual files before removing duplicates. Duplicate-group view is read-only.",
            );
        }
        let synced = rows.iter().any(|r| is_synced(Path::new(&r.path)));
        let total: u64 = rows.iter().map(|r| r.bytes).sum();
        let mut msg = format!(
            "Move {} files ({}) to quarantine?\n\nFiles are revalidated before moving.\nQuarantine requires the same filesystem.",
            rows.len(),
            human_size(total as f64)
        );
        if synced {
            msg.push_str("\n\nWARNING: a sync client may propagate the removal to other devices.");
        }
        self.confirm("Confirm quarantine", &msg, move |app| {
            let run = move |app: &mut App, rows: Vec<FileRow>| {
                app.task(
                    false,
                    move |_| Ok(storage::storage_quarantine(&rows, synced)),
                    |app, out| {
                        let moved: std::collections::HashSet<String> =
                            out.done.iter().cloned().collect();
                        app.st_rows.retain(|r| !moved.contains(&r.0.path));
                        app.filter_storage();
                        app.info(
                            "Quarantine",
                            &format!(
                                "{} files moved to quarantine.\n{} skipped.\nRestore via History.",
                                moved.len(),
                                out.skipped.len()
                            ),
                        );
                    },
                );
            };
            if synced {
                app.ask_text(
                    "Confirm synced-folder change",
                    "Type SYNC DELETE to confirm removal from synchronized directories:",
                    "",
                    move |app, answer| {
                        if answer == "SYNC DELETE" {
                            run(app, rows);
                        }
                    },
                );
            } else {
                run(app, rows);
            }
        });
    }

    fn storage_delete(&mut self) {
        let rows = self.storage_selected();
        if rows.is_empty() {
            return;
        }
        if self.st_mode == "duplicates" {
            return self.info(
                "Duplicates",
                "Review duplicate copies individually using another scan before deletion.",
            );
        }
        let synced = rows.iter().any(|r| is_synced(Path::new(&r.path)));
        let total: u64 = rows.iter().map(|r| r.bytes).sum();
        let mut msg = format!(
            "PERMANENTLY delete {} selected files ({})?\n\nThis cannot be undone locally. Files will be checked again before deletion.",
            rows.len(),
            human_size(total as f64)
        );
        if synced {
            msg.push_str(
                "\n\nA sync client may propagate these deletions to the server and other devices.",
            );
        }
        self.confirm("Confirm permanent deletion", &msg, move |app| {
            let phrase = if synced { "DELETE SYNCED" } else { "DELETE" };
            app.ask_text(
                "Final confirmation",
                &format!("Type {phrase} to permanently delete the selected files:"),
                "",
                move |app, answer| {
                    if answer != phrase {
                        return;
                    }
                    app.task(
                        false,
                        move |_| Ok(storage::storage_delete(&rows, synced)),
                        |app, out| {
                            let gone: std::collections::HashSet<String> = out.done.iter().cloned().collect();
                            app.st_rows.retain(|r| !gone.contains(&r.0.path));
                            app.filter_storage();
                            app.info(
                                "Permanent deletion",
                                &format!("{} files deleted.\n{} skipped.\nSee History for the audit log.", gone.len(), out.skipped.len()),
                            );
                        },
                    );
                },
            );
        });
    }

    // ------------------------------------------------------------ media

    fn media_done(&mut self, inv: crate::media::Inventory) {
        let rows: Vec<Row> = crate::media::KINDS
            .iter()
            .enumerate()
            .map(|(i, k)| {
                let t = &inv.types[i];
                let mut ext = t.extensions.clone();
                ext.sort_by_key(|e| std::cmp::Reverse(e.1));
                let ext = ext
                    .iter()
                    .take(7)
                    .map(|(e, n)| format!("{e}: {n}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                Row::keyed(
                    i,
                    vec![
                        k.to_string(),
                        t.count.to_string(),
                        human_size(t.bytes as f64),
                        ext.clone(),
                    ],
                    vec![
                        Key::S(k.to_string()),
                        Key::N(t.count as f64),
                        Key::N(t.bytes as f64),
                        Key::S(ext),
                    ],
                )
            })
            .collect();
        self.tables[TableId::Media as usize].set_rows(rows);
        self.media_summary = format!(
            "{} files · {} · {} inaccessible directories/files · {} s",
            inv.total,
            human_size(inv.bytes as f64),
            inv.errors,
            crate::json::py_float(inv.duration_s)
        );
    }

    // ---------------------------------------------------------- history

    pub(super) fn refresh_history(&mut self) {
        match crate::store::history(300) {
            Ok(rows) => self.history_rows = rows,
            Err(e) => self.status = format!("Error: {e}"),
        }
        self.fill_history();
    }

    fn fill_history(&mut self) {
        let st = self.style();
        let rows: Vec<Row> = self
            .history_rows
            .iter()
            .enumerate()
            .map(|(i, a)| {
                Row::keyed(
                    i,
                    vec![
                        a.id.to_string(),
                        st.iso(&a.ts),
                        a.rule.clone(),
                        a.source.clone(),
                        a.status.clone(),
                        a.bytes.to_string(),
                    ],
                    vec![
                        Key::N(a.id as f64),
                        Key::S(a.ts.clone()),
                        Key::S(a.rule.clone()),
                        Key::S(a.source.clone()),
                        Key::S(a.status.clone()),
                        Key::N(a.bytes as f64),
                    ],
                )
            })
            .collect();
        self.tables[TableId::History as usize].set_rows(rows);
    }

    fn restore_selected(&mut self) {
        let Some(i) = self.tables[TableId::History as usize].first_selected() else {
            return;
        };
        let Some(a) = self.history_rows.get(i).cloned() else {
            return;
        };
        if a.status != "quarantined" {
            return self.info("Cannot restore", "Select a quarantine entry.");
        }
        self.confirm(
            "Restore",
            &format!("Restore {}?", a.source),
            move |app| match cleanup::restore(a.id) {
                Ok(_) => {
                    app.refresh_history();
                    app.status = "File restored".into();
                }
                Err(e) => app.info("Restore failed", &e),
            },
        );
    }

    // --------------------------------------------------------- settings

    pub(super) fn refresh_scheduler(&mut self) {
        self.sched_loaded = true;
        self.task(
            false,
            |_| crate::scheduler::status(),
            |app, v| app.sched_done(v),
        );
    }

    fn sched_done(&mut self, v: Value) {
        let yn = |k: &str| {
            if v[k].as_bool().unwrap_or(false) {
                "Yes"
            } else {
                "No"
            }
        };
        self.sched_info = if !v["supported"].as_bool().unwrap_or(false) {
            "Not available on this system (the CLI can use another scheduler).".into()
        } else {
            format!(
                "Installed: {}  ·  Active: {}  ·  Enabled at login: {}",
                yn("installed"),
                yn("active"),
                yn("enabled")
            )
        };
    }

    pub(super) fn refresh_locations(&mut self) {
        let rows: Vec<Row> = config::locations(&self.cfg, None)
            .iter()
            .enumerate()
            .map(|(i, x)| {
                let on = x.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                Row::new(
                    i,
                    vec![
                        str_of(x, "name").into(),
                        str_of(x, "kind").into(),
                        str_of(x, "path").into(),
                        if on { "Yes" } else { "No" }.into(),
                    ],
                )
            })
            .collect();
        self.tables[TableId::Locations as usize].set_rows(rows);
    }

    fn selected_location(&self) -> Option<usize> {
        self.tables[TableId::Locations as usize].first_selected()
    }

    // ------------------------------------------------------------ pages

    pub(super) fn draw_page(&mut self, x: i32, y: i32, w: i32, bottom: i32) -> i32 {
        match self.page {
            0 => self.page_overview(x, y, w, bottom),
            1 => self.page_automations(x, y, w, bottom),
            2 => self.page_cleanup(x, y, w, bottom),
            3 => self.page_projects(x, y, w, bottom),
            4 => self.page_docker(x, y, w, bottom),
            5 => self.page_storage(x, y, w, bottom),
            6 => self.page_media(x, y, w, bottom),
            7 => self.page_history(x, y, w, bottom),
            _ => self.page_settings(x, y, w),
        }
    }

    /// The height left for a table above `below` (at least five rows).
    fn fill_h(&self, y: i32, bottom: i32, below: i32) -> i32 {
        (bottom - y - below - self.m.gap).max(self.m.row_h * 6)
    }

    fn b(h: Btn) -> Option<Hit> {
        Some(Hit::Btn(h))
    }

    fn page_overview(&mut self, x: i32, mut y: i32, w: i32, bottom: i32) -> i32 {
        y += self.label(
            x,
            y,
            w,
            "Local maintenance and automation · No cloud service required",
            true,
        );
        let (s, d, e) = (
            self.ov_status.clone(),
            self.ov_disk.clone(),
            self.ov_empty.clone(),
        );
        y += self.label(x, y, w, &s, false);
        y += self.label(x, y, w, &d, false);
        y += self.m.gap;
        y += self.label(x, y, w, "Recent activity", false);
        let th = (self.m.row_h * 12).min(self.fill_h(y, bottom, self.m.bh * 3));
        self.table(Rect::new(x, y, w, th), TableId::Overview);
        y += th + self.m.gap;
        if !e.is_empty() {
            y += self.label(x, y, w, &e, true);
        }
        let by = (bottom - self.m.bh - self.m.gap).max(y);
        self.buttons(
            x,
            by,
            w,
            &[("Refresh overview", Self::b(Btn::OverviewRefresh))],
        )
    }

    fn page_automations(&mut self, x: i32, mut y: i32, w: i32, bottom: i32) -> i32 {
        y += self.label(
            x,
            y,
            w,
            "Rules run independently through CLI or the optional background scheduler.",
            true,
        );
        let items: [(&str, Option<Hit>); 4] = [
            ("Enable / Disable", Self::b(Btn::RuleToggle)),
            ("Edit rule", Self::b(Btn::RuleEdit)),
            ("Preview", Self::b(Btn::RulePreview)),
            ("Run now (quarantine)", Self::b(Btn::RuleRun)),
        ];
        let below = self.measure(|a, y| {
            let mut h = a.label(
                x,
                y,
                w,
                "Synced folders are excluded from unattended cleanup.",
                true,
            );
            h += a.buttons(x, y, w, &items) - y;
            h
        });
        let th = self.fill_h(y, bottom, below);
        self.table(Rect::new(x, y, w, th), TableId::Rules);
        y += th + self.m.gap;
        y += self.label(
            x,
            y,
            w,
            "Synced folders are excluded from unattended cleanup.",
            true,
        );
        self.buttons(x, y, w, &items)
    }

    fn page_cleanup(&mut self, x: i32, y: i32, w: i32, bottom: i32) -> i32 {
        let mut f = self.flow(x, y, w);
        self.fcombo(&mut f, ComboId::CleanupRule);
        let on = self.cl_enabled;
        self.fcheck(&mut f, CheckId::CleanupEnabled, "Enabled", on);
        self.fcombo(&mut f, ComboId::CleanupAge);
        self.fbtn(&mut f, "Choose directory", Self::b(Btn::CleanupChooseDir));
        self.fbtn(&mut f, "Save rule", Self::b(Btn::CleanupSave));
        let mut y = f.end();
        let info = self.cl_info.clone();
        y += self.label(x, y, w, &info, true);
        let items: [(&str, Option<Hit>); 3] = [
            ("Analyze", Self::b(Btn::CleanupAnalyze)),
            ("Select all", Self::b(Btn::CleanupSelectAll)),
            (
                "Move selected to quarantine",
                Self::b(Btn::CleanupQuarantine),
            ),
        ];
        let note = "Subdirectories are not traversed. Protected names are excluded. Large files use a bounded content fingerprint; inspect the file size and path before permanent deletion.";
        let below =
            self.measure(|a, y| a.buttons(x, y, w, &items) - y + a.label(x, y, w, note, true));
        let th = self.fill_h(y, bottom, below);
        self.table(Rect::new(x, y, w, th), TableId::Cleanup);
        y += th + self.m.gap;
        y = self.buttons(x, y, w, &items);
        y + self.label(x, y, w, note, true)
    }

    fn page_projects(&mut self, x: i32, mut y: i32, w: i32, bottom: i32) -> i32 {
        y += self.label(
            x,
            y,
            w,
            "Read-only inventory. Patch chains and unknown artifacts are protected.",
            true,
        );
        let mut f = self.flow(x, y, w);
        self.fbtn(
            &mut f,
            "Analyze project directories",
            Self::b(Btn::ProjectsAnalyze),
        );
        let fw = (w - (f.x - x)).max((200.0 * self.m.s) as i32);
        self.ffield(&mut f, FieldId::ProjectFilter, fw);
        y = f.end();
        let status = self.projects_status.clone();
        let note = "“Review” does not mean safe to delete. Project inventory is read-only.";
        let below =
            self.measure(|a, y| a.label(x, y, w, note, true) + a.label(x, y, w, &status, true));
        let th = self.fill_h(y, bottom, below);
        self.table(Rect::new(x, y, w, th), TableId::Projects);
        y += th + self.m.gap;
        y += self.label(x, y, w, note, true);
        y + self.label(x, y, w, &status, true)
    }

    fn page_docker(&mut self, x: i32, mut y: i32, w: i32, bottom: i32) -> i32 {
        y += self.label(
            x,
            y,
            w,
            "Docker Engine · explicit confirmation for actions; no unattended deletion.",
            true,
        );
        let mut f = self.flow(x, y, w);
        self.fbtn(&mut f, "Refresh Docker", Self::b(Btn::DockerRefresh));
        y = f.end();
        let summary = self.docker_summary.clone();
        y += self.label(x, y, w, &summary, false);
        // Tabs.
        let mut tx = x;
        for (i, name) in ["Containers", "Images", "Compose", "Cleanup"]
            .iter()
            .enumerate()
        {
            let tw = self.text.width(name, self.m.fs) + 2 * self.m.pad;
            let r = Rect::new(tx, y, tw, self.m.bh);
            if i == self.docker_tab {
                self.fill(r, self.c.hover);
                self.fill(Rect::new(r.x, r.y + r.h - 2, r.w, 2), self.c.fg);
            } else if self.hover == Some(Hit::Tab(i)) {
                self.fill(r, self.c.panel);
            }
            let c = if i == self.docker_tab {
                self.c.fg
            } else {
                self.c.muted
            };
            self.draw_text(
                tx + self.m.pad,
                y + (self.m.bh - self.m.fs.height) / 2,
                name,
                self.m.fs,
                c,
            );
            self.hit(r, Hit::Tab(i));
            tx += tw + 2;
        }
        y += self.m.bh;
        self.fill(Rect::new(x, y, w, 1), self.c.border);
        y += self.m.gap;
        match self.docker_tab {
            0 => {
                let mut f = self.flow(x, y, w);
                let cw = self.measure(|a, _| {
                    let mut g = a.flow(0, 0, 10000);
                    a.fcombo(&mut g, ComboId::ContainerFilter);
                    g.x
                });
                self.ffield(
                    &mut f,
                    FieldId::ContainerSearch,
                    (w - cw - self.m.gap).max(100),
                );
                self.fcombo(&mut f, ComboId::ContainerFilter);
                y = f.end();
                let sel = match self.selected_container() {
                    Some(c) => {
                        format!("Selected: {}  ·  {}", vstr(&c, "Names"), vstr(&c, "Status"))
                    }
                    None => "Selected: none".into(),
                };
                y += self.label(x, y, w, &sel, true);
                let mut items: Vec<(&str, Option<Hit>)> = ACTIONS
                    .iter()
                    .enumerate()
                    .map(|(i, (_, c))| (*c, Self::b(Btn::DockerAction(i))))
                    .collect();
                items.push(("Logs", Self::b(Btn::DockerLogs)));
                items.push(("Mounts", Self::b(Btn::DockerMounts)));
                items.push(("Remove stopped container", Self::b(Btn::DockerRemove)));
                let below = self.measure(|a, y| a.buttons(x, y, w, &items) - y);
                let th = self.fill_h(y, bottom, below);
                self.table(Rect::new(x, y, w, th), TableId::Containers);
                y += th + self.m.gap;
                self.buttons(x, y, w, &items)
            }
            1 => {
                let mut f = self.flow(x, y, w);
                self.ffield(&mut f, FieldId::ImageSearch, w);
                y = f.end();
                let th = self.fill_h(y, bottom, 0);
                self.table(Rect::new(x, y, w, th), TableId::Images);
                y + th + self.m.gap
            }
            2 => {
                y += self.label(
                    x,
                    y,
                    w,
                    "Compose projects on disk (read-only inventory)",
                    false,
                );
                y = self.buttons(
                    x,
                    y,
                    w,
                    &[("Discover Compose files", Self::b(Btn::ComposeDiscover))],
                );
                let th = self.fill_h(y, bottom, 0);
                self.table(Rect::new(x, y, w, th), TableId::Compose);
                y + th + self.m.gap
            }
            _ => {
                let mut f = self.flow(x, y, w);
                self.flabel(&mut f, "Image cleanup · minimum age (days)", false);
                self.fcombo(&mut f, ComboId::ImageAge);
                let inc = self.include_unused;
                self.fcheck(
                    &mut f,
                    CheckId::IncludeUnused,
                    "Include unused tagged images",
                    inc,
                );
                self.fbtn(&mut f, "Preview cleanup", Self::b(Btn::ImagesPreview));
                self.fbtn(&mut f, "Delete selected images", Self::b(Btn::ImagesDelete));
                y = f.end();
                let info = self.image_info.clone();
                y += self.label(x, y, w, &info, true);
                let disk_lines = self.docker_disk.lines().count().clamp(3, 6) as i32;
                let disk_h = disk_lines * (self.m.small.height + 1) + 10;
                let below = self.m.bh + 2 * self.m.gap + self.m.fs.height + self.m.gap + disk_h;
                let th = self.fill_h(y, bottom, below).max(self.m.row_h * 8);
                self.table(Rect::new(x, y, w, th), TableId::ImageCleanup);
                y += th + self.m.gap;
                y = self.buttons(
                    x,
                    y,
                    w,
                    &[("Select all candidates", Self::b(Btn::ImagesSelectAll))],
                );
                y += self.label(x, y, w, "Docker disk usage", false);
                let r = Rect::new(x, y, w, disk_h);
                self.fill(r, self.c.panel);
                self.frame(r, self.c.border);
                let disk = self.docker_disk.clone();
                for (k, line) in disk.lines().take(disk_lines as usize).enumerate() {
                    let s = self.text.ellipsize(line, self.m.small, w - 12);
                    self.draw_text(
                        x + 6,
                        y + 5 + k as i32 * (self.m.small.height + 1),
                        &s,
                        self.m.small,
                        self.c.fg,
                    );
                }
                y + disk_h + self.m.gap
            }
        }
    }

    fn page_storage(&mut self, x: i32, mut y: i32, w: i32, bottom: i32) -> i32 {
        y += self.label(
            x,
            y,
            w,
            "Storage Explorer · inspect before moving files to quarantine. Synced files need extra confirmation.",
            true,
        );
        let scanning = self.st_cancel.is_some();
        let idle = |h: Btn| if scanning { None } else { Some(Hit::Btn(h)) };
        let mut f = self.flow(x, y, w);
        self.fcombo(&mut f, ComboId::StorageScope);
        self.fcombo(&mut f, ComboId::StorageMonths);
        self.fbtn(&mut f, "Old files", idle(Btn::StorageOld));
        self.fbtn(&mut f, "Largest files", idle(Btn::StorageLargest));
        self.fbtn(&mut f, "Duplicates", idle(Btn::StorageDup));
        let cancel = if scanning {
            Self::b(Btn::StorageCancel)
        } else {
            None
        };
        self.fbtn(&mut f, "Cancel scan", cancel);
        y = f.end();
        let mut f = self.flow(x, y, w);
        let cw = self.measure(|a, _| {
            let mut g = a.flow(0, 0, 10000);
            a.fcombo(&mut g, ComboId::StorageType);
            a.fcombo(&mut g, ComboId::StorageSort);
            g.x
        });
        self.ffield(
            &mut f,
            FieldId::StorageSearch,
            (w - cw - self.m.gap).max(120),
        );
        self.fcombo(&mut f, ComboId::StorageType);
        self.fcombo(&mut f, ComboId::StorageSort);
        y = f.end();
        let info = self.st_info.clone();
        y += self.label(x, y, w, &info, true);
        let items: [(&str, Option<Hit>); 4] = [
            ("Open directory", Self::b(Btn::StorageOpenDir)),
            ("Select all visible", Self::b(Btn::StorageSelectVisible)),
            (
                "Move selected to quarantine",
                Self::b(Btn::StorageQuarantine),
            ),
            ("Delete permanently", Self::b(Btn::StorageDelete)),
        ];
        let below = self.measure(|a, y| a.buttons(x, y, w, &items) - y);
        let th = self.fill_h(y, bottom, below);
        self.table(Rect::new(x, y, w, th), TableId::Storage);
        y += th + self.m.gap;
        self.buttons(x, y, w, &items)
    }

    fn page_media(&mut self, x: i32, mut y: i32, w: i32, bottom: i32) -> i32 {
        y += self.label(
            x,
            y,
            w,
            "Read-only media inventory. Scans run in the background; no files are modified.",
            true,
        );
        let mut f = self.flow(x, y, w);
        let bw = self.btn_w("Add directory") + self.btn_w("Scan media") + 2 * self.m.gap;
        self.ffield(&mut f, FieldId::MediaRoots, (w - bw - self.m.gap).max(160));
        self.fbtn(&mut f, "Add directory", Self::b(Btn::MediaAdd));
        self.fbtn(&mut f, "Scan media", Self::b(Btn::MediaScan));
        y = f.end();
        let s = self.media_summary.clone();
        y += self.label(x, y, w, &s, false);
        let th = (self.m.row_h * 6).max(self.fill_h(y, bottom, 0).min(self.m.row_h * 8));
        self.table(Rect::new(x, y, w, th), TableId::Media);
        y + th + self.m.gap
    }

    fn page_history(&mut self, x: i32, mut y: i32, w: i32, bottom: i32) -> i32 {
        y += self.label(
            x,
            y,
            w,
            "Quarantined files can be restored if the original path is available.",
            true,
        );
        let items: [(&str, Option<Hit>); 2] = [
            ("Refresh", Self::b(Btn::HistoryRefresh)),
            ("Restore selected action", Self::b(Btn::HistoryRestore)),
        ];
        let below = self.measure(|a, y| a.buttons(x, y, w, &items) - y);
        let th = self.fill_h(y, bottom, below);
        self.table(Rect::new(x, y, w, th), TableId::History);
        y += th + self.m.gap;
        self.buttons(x, y, w, &items)
    }

    fn page_settings(&mut self, x: i32, mut y: i32, w: i32) -> i32 {
        y += self.label(
            x,
            y,
            w,
            &format!(
                "NIRUFLOW {} · Ing Leif Nicklas Rudolfsson · MIT",
                crate::VERSION
            ),
            false,
        );
        y += self.label(
            x,
            y,
            w,
            "Interface language: English · Regional formatting",
            false,
        );
        let mut f = self.flow(x, y, w);
        self.flabel(&mut f, "Regional format", false);
        self.fcombo(&mut f, ComboId::Locale);
        self.flabel(&mut f, "Date format", false);
        self.fcombo(&mut f, ComboId::DateFmt);
        self.flabel(&mut f, "Time format", false);
        self.fcombo(&mut f, ComboId::TimeFmt);
        y = f.end();
        let mut f = self.flow(x, y, w);
        let wd = self.style().weekday;
        self.fcheck(
            &mut f,
            CheckId::Weekday,
            "Show weekday (e.g. mån, tis, ons for Sweden)",
            wd,
        );
        y = f.end();
        let example = format!(
            "Example: {}",
            self.style().civil(&crate::fmt::Civil {
                y: 2026,
                mo: 10,
                d: 9,
                h: 22,
                mi: 0,
                s: 0,
                us: 0
            })
        );
        y += self.label(x, y, w, &example, true);
        for line in [
            "Change theme in the upper-right corner.",
            "Configuration: ~/.config/niruflow/config.json",
            "History and quarantine: ~/.local/state/niruflow/",
            "Scheduler: optional systemd user timer.",
            "Unattended Nextcloud cleanup is blocked.",
        ] {
            y += self.label(x, y, w, line, true);
        }
        y = self.buttons(
            x,
            y,
            w,
            &[("Preview automation safety", Self::b(Btn::SafetyPreview))],
        );
        y += self.label(
            x,
            y,
            w,
            "Background scheduling · independent of desktop window",
            false,
        );
        let si = self.sched_info.clone();
        y += self.label(x, y, w, &si, true);
        y = self.buttons(
            x,
            y,
            w,
            &[
                ("Refresh status", Self::b(Btn::SchedRefresh)),
                ("Install scheduler", Self::b(Btn::SchedInstall)),
                ("Enable", Self::b(Btn::SchedEnable)),
                ("Pause", Self::b(Btn::SchedPause)),
            ],
        );
        y += self.label(
            x,
            y,
            w,
            "The scheduler checks enabled rules hourly, even when the desktop is closed. Synced directories are skipped.",
            true,
        );
        y += self.label(x, y, w, "Configuration backup · profiles, rules and preferences; excludes history and quarantine", false);
        y = self.buttons(
            x,
            y,
            w,
            &[
                ("Export configuration", Self::b(Btn::BackupExport)),
                ("Import configuration", Self::b(Btn::BackupImport)),
            ],
        );
        let mut f = self.flow(x, y, w);
        self.flabel(&mut f, "Profile", false);
        self.fcombo(&mut f, ComboId::Profile);
        self.fbtn(&mut f, "New profile", Self::b(Btn::NewProfile));
        y = f.end();
        y += self.label(
            x,
            y,
            w,
            "Monitored locations · profile-specific directories",
            false,
        );
        let th = self.m.row_h * 8;
        self.table(Rect::new(x, y, w, th), TableId::Locations);
        y += th + self.m.gap;
        self.buttons(
            x,
            y,
            w,
            &[
                ("Add location", Self::b(Btn::LocAdd)),
                ("Edit selected", Self::b(Btn::LocEdit)),
                ("Enable / Pause", Self::b(Btn::LocToggle)),
                ("Remove selected", Self::b(Btn::LocRemove)),
            ],
        )
    }
}

/// Progress lines for a storage scan, sent to the window.
fn progress_fn(ctx: &Ctx) -> impl Fn(usize, &Path) + Sync + '_ {
    move |count: usize, dir: &Path| {
        let text = format!(
            "Scanning · {} files examined · {} · no changes",
            thousands(count),
            dir.display()
        );
        ctx.send(Box::new(move |app: &mut App| {
            if app
                .st_cancel
                .as_ref()
                .is_some_and(|c| !c.load(Ordering::Relaxed))
            {
                app.st_info = text;
            }
        }));
    }
}

/// (total, used, free) bytes of the filesystem holding `p`, like
/// `shutil.disk_usage`.
fn disk_usage(p: &Path) -> Option<(u64, u64, u64)> {
    let c = std::ffi::CString::new(p.as_os_str().as_encoded_bytes()).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    let f = s.f_frsize as u64;
    Some((
        s.f_blocks as u64 * f,
        (s.f_blocks as u64 - s.f_bfree as u64) * f,
        s.f_bavail as u64 * f,
    ))
}
