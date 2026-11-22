//! The NIRUFLOW window: one canvas, an event loop on the X socket, an
//! eventfd for finished background work and inotify for the NIRUWM theme,
//! so the window sleeps while idle. Every scan runs on a worker thread.
use crate::canvas::{Canvas, Rect};
use crate::config::R;
use crate::text::{Size, Text};
use crate::theme::{self, Colors};
use crate::ui::{keysym_char, Field, PickMode, Picker, Table};
use crate::x::{Res, X};
use serde_json::Value;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{KeyButMask, SelectionRequestEvent};
use x11rb::protocol::Event;

#[path = "pages.rs"]
mod pages;

pub const PAGES: [&str; 9] = [
    "Overview",
    "Automations",
    "File Cleanup",
    "Projects",
    "Docker",
    "Storage",
    "Media Library",
    "History",
    "Settings",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Btn {
    Close,
    OverviewRefresh,
    RuleToggle,
    RuleEdit,
    RulePreview,
    RuleRun,
    CleanupChooseDir,
    CleanupSave,
    CleanupAnalyze,
    CleanupSelectAll,
    CleanupQuarantine,
    ProjectsAnalyze,
    DockerRefresh,
    DockerAction(usize),
    DockerLogs,
    DockerMounts,
    DockerRemove,
    ComposeDiscover,
    ImagesPreview,
    ImagesDelete,
    ImagesSelectAll,
    StorageOld,
    StorageLargest,
    StorageDup,
    StorageCancel,
    StorageOpenDir,
    StorageSelectVisible,
    StorageQuarantine,
    StorageDelete,
    MediaAdd,
    MediaScan,
    HistoryRefresh,
    HistoryRestore,
    SafetyPreview,
    SchedRefresh,
    SchedInstall,
    SchedEnable,
    SchedPause,
    BackupExport,
    BackupImport,
    NewProfile,
    LocAdd,
    LocEdit,
    LocToggle,
    LocRemove,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComboId {
    Theme,
    CleanupRule,
    CleanupAge,
    ContainerFilter,
    ImageAge,
    StorageScope,
    StorageMonths,
    StorageType,
    StorageSort,
    Locale,
    DateFmt,
    TimeFmt,
    Profile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckId {
    CleanupEnabled,
    IncludeUnused,
    Weekday,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldId {
    ProjectFilter = 0,
    ContainerSearch,
    ImageSearch,
    StorageSearch,
    MediaRoots,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableId {
    Overview = 0,
    Rules,
    Cleanup,
    Projects,
    Containers,
    Images,
    Compose,
    ImageCleanup,
    Storage,
    Media,
    History,
    Locations,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DlgHit {
    Ok,
    Cancel,
    Item(usize),
    Field,
    Unit(usize),
    Up,
    Home,
    Hidden,
    Entry(usize),
    List,
    Detail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Nav(usize),
    Btn(Btn),
    Combo(ComboId),
    PopItem(usize),
    Check(CheckId),
    Field(FieldId),
    Row(TableId, usize),
    Header(TableId, usize),
    Body(TableId),
    Tab(usize),
    Dlg(DlgHit),
    Shade,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Field(FieldId),
    Table(TableId),
}

pub enum PopOwner {
    Combo(ComboId),
    Context,
}

pub struct Popup {
    pub owner: PopOwner,
    pub items: Vec<String>,
    pub rect: Rect,
    pub hover: usize,
}

pub enum DlgValue {
    None,
    Text(String),
    Index(usize),
    Hours(i64),
    Path(PathBuf),
}

pub enum DlgKind {
    Info,
    Confirm,
    Input,
    Choice(Vec<String>, usize),
    Age(usize),
    Pick(Picker),
}

type OnOk = Box<dyn FnOnce(&mut App, DlgValue)>;

pub struct Dialog {
    pub title: String,
    pub text: String,
    pub detail: Vec<String>,
    pub detail_top: usize,
    pub kind: DlgKind,
    pub field: Field,
    /// Confirm dialogs: the button with the keyboard (false = No, default).
    pub yes_focus: bool,
    pub on_ok: Option<OnOk>,
}

impl Dialog {
    fn new(title: &str, text: &str, kind: DlgKind) -> Self {
        Dialog {
            title: title.into(),
            text: text.into(),
            detail: Vec::new(),
            detail_top: 0,
            kind,
            field: Field::new(""),
            yes_focus: false,
            on_ok: None,
        }
    }
}

pub type Done = Box<dyn FnOnce(&mut App) + Send>;

/// What a background job gets: its cancel flag and a way to report.
pub struct Ctx {
    pub cancel: Arc<AtomicBool>,
    tx: Sender<Done>,
    wake: i32,
}

impl Ctx {
    pub fn send(&self, d: Done) {
        let _ = self.tx.send(d);
        let one: u64 = 1;
        unsafe { libc::write(self.wake, &one as *const u64 as *const libc::c_void, 8) };
    }
}

/// Sizes in pixels for the current font size.
#[derive(Clone, Copy)]
pub struct Metrics {
    pub s: f32,
    pub fs: Size,
    pub small: Size,
    pub title: Size,
    pub row_h: i32,
    pub bh: i32,
    pub pad: i32,
    pub gap: i32,
    pub side_w: i32,
    pub header_h: i32,
    pub status_h: i32,
}

/// Lays out widgets left to right, wrapping into new lines.
pub struct Flow {
    pub x0: i32,
    pub x: i32,
    pub y: i32,
    pub max: i32,
    pub line: i32,
    pub gap: i32,
}

impl Flow {
    pub fn place(&mut self, w: i32, h: i32) -> (i32, i32) {
        if self.x > self.x0 && self.x + w > self.max {
            self.y += self.line + self.gap;
            self.x = self.x0;
            self.line = 0;
        }
        let at = (self.x, self.y);
        self.x += w + self.gap;
        self.line = self.line.max(h);
        at
    }
    pub fn end(&self) -> i32 {
        self.y + self.line + self.gap
    }
}

pub struct App {
    x: X,
    cv: Canvas,
    text: Text,
    c: Colors,
    desktop_theme: String,
    m: Metrics,
    cfg: Value,
    page: usize,
    hits: Vec<(Rect, Hit)>,
    measuring: bool,
    hover: Option<Hit>,
    mouse: (i32, i32),
    tables: Vec<Table>,
    fields: Vec<Field>,
    focus: Option<Focus>,
    popup: Option<Popup>,
    dialogs: VecDeque<Dialog>,
    status: String,
    jobs: usize,
    tx: Sender<Done>,
    rx: Receiver<Done>,
    wake: i32,
    inotify: i32,
    dirty: bool,
    quit: bool,
    started: Instant,
    shown_once: bool,
    smoke_until: Option<Instant>,
    page_scroll: [i32; 9],
    page_h: i32,
    content: Rect,
    base_clip: Rect,
    last_click: Option<(Hit, Instant)>,
    clipboard: String,
    // Page state
    ov_status: String,
    ov_disk: String,
    ov_empty: String,
    cl_rule: usize,
    cl_enabled: bool,
    cl_hours: i64,
    cl_info: String,
    candidates: Vec<crate::cleanup::Candidate>,
    project_rows: Vec<crate::projects::Entry>,
    projects_status: String,
    docker_data: Option<Value>,
    docker_summary: String,
    docker_tab: usize,
    container_filter: usize,
    image_days: i64,
    include_unused: bool,
    image_plan: Option<Value>,
    image_info: String,
    docker_disk: String,
    docker_loading: bool,
    compose_rows: Vec<crate::compose::Compose>,
    st_scope: usize,
    st_months: i64,
    st_type: usize,
    st_sort: usize,
    st_info: String,
    st_rows: Vec<(crate::storage::FileRow, &'static str)>,
    st_display: Vec<usize>,
    st_mode: String,
    st_cancel: Option<Arc<AtomicBool>>,
    media_summary: String,
    history_rows: Vec<crate::store::Action>,
    sched_info: String,
    sched_loaded: bool,
}

fn event_mods(state: KeyButMask) -> (bool, bool, bool) {
    (
        state.contains(KeyButMask::CONTROL),
        state.contains(KeyButMask::SHIFT),
        state.contains(KeyButMask::MOD1),
    )
}

pub fn launch(cfg: Value, started: Instant) -> Res<()> {
    let mut app = App::new(cfg, started)?;
    app.run()
}

impl App {
    pub fn new(cfg: Value, started: Instant) -> Res<Self> {
        let base = theme::config_home();
        let font_px = theme::configured_font_size(&base).unwrap_or(14.0);
        let font_path = theme::configured_font_path(&base);
        // Finding the font (fc-match) overlaps with connecting to X.
        let font = std::thread::spawn(move || Text::new(font_path.as_deref()));
        let s = (font_px / 14.0).clamp(0.6, 3.0);
        let (w, h) = ((1180.0 * s) as u16, (760.0 * s) as u16);
        let x = X::open(w, h, &format!("NIRUFLOW {}", crate::VERSION))?;
        let desktop_theme = x
            .root_theme()
            .unwrap_or_else(|| theme::configured_theme(&base));
        let theme_name = cfg
            .get("theme")
            .and_then(Value::as_str)
            .unwrap_or("Niru Noir")
            .to_string();
        let c = theme::colors(&theme_name, &base, &desktop_theme);
        let _ = x.set_icon(&crate::logo::window_icons(&c));
        let text = font.join().map_err(|_| "font thread failed")??;
        let m = metrics(&text, font_px, s);
        let inotify = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if inotify >= 0 {
            for dir in [base.join("niruwm"), base.join("niruwm/themes")] {
                if let Ok(cs) = std::ffi::CString::new(dir.as_os_str().as_encoded_bytes()) {
                    unsafe {
                        libc::inotify_add_watch(
                            inotify,
                            cs.as_ptr(),
                            libc::IN_CLOSE_WRITE
                                | libc::IN_MOVED_TO
                                | libc::IN_CREATE
                                | libc::IN_DELETE,
                        )
                    };
                }
            }
        }
        let wake = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
        let (tx, rx) = std::sync::mpsc::channel();
        let tables = vec![
            Table::new(&["Time", "Rule", "Action", "File"], true, true),
            Table::new(
                &["Rule", "Directory", "Age", "Enabled", "Background"],
                true,
                true,
            ),
            Table::new(&["File", "Size", "Modified", "Full path"], true, true),
            Table::new(
                &[
                    "Project",
                    "Artifact",
                    "Type",
                    "Version",
                    "From → To",
                    "Assessment",
                ],
                true,
                true,
            ),
            Table::new(&["Name", "Image", "Status", "Health", "Size"], false, true),
            Table::new(&["Repository", "Tag", "ID", "Size"], true, true),
            Table::new(&["Project", "Compose file", "Search root"], true, true),
            Table::new(&["Image ID", "Tags", "Created", "Size"], true, true),
            Table::new(
                &["File", "Size", "Modified", "Type", "Directory"],
                true,
                false,
            ),
            Table::new(&["Type", "Files", "Size", "Extensions"], true, true),
            Table::new(
                &["ID", "Time", "Rule", "Original", "Status", "Size"],
                true,
                true,
            ),
            Table::new(&["Name", "Type", "Directory", "Enabled"], true, true),
        ];
        let media_roots = match cfg.get("media_roots").and_then(Value::as_array) {
            Some(a) => a
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
            None => crate::media::default_roots(),
        };
        let mut fields = vec![
            Field::new("Filter projects or filenames…"),
            Field::new("Search containers..."),
            Field::new("Search images..."),
            Field::new("Filter by name or path…"),
            Field::new("Directories separated by semicolons"),
        ];
        fields[FieldId::MediaRoots as usize].set(&media_roots.join("; "));
        let smoke_until = (std::env::var("NIRUFLOW_SMOKE").as_deref() == Ok("1"))
            .then(|| Instant::now() + Duration::from_millis(1500));
        let mut app = App {
            cv: Canvas::new(w as i32, h as i32),
            x,
            text,
            c,
            desktop_theme,
            m,
            cfg,
            page: 0,
            hits: Vec::new(),
            measuring: false,
            hover: None,
            mouse: (-1, -1),
            tables,
            fields,
            focus: None,
            popup: None,
            dialogs: VecDeque::new(),
            status: "Ready · destructive actions require confirmation".into(),
            jobs: 0,
            tx,
            rx,
            wake,
            inotify,
            dirty: true,
            quit: false,
            started,
            shown_once: false,
            smoke_until,
            page_scroll: [0; 9],
            page_h: 0,
            content: Rect::default(),
            base_clip: Rect::new(0, 0, w as i32, h as i32),
            last_click: None,
            clipboard: String::new(),
            ov_status: String::new(),
            ov_disk: String::new(),
            ov_empty: String::new(),
            cl_rule: 0,
            cl_enabled: false,
            cl_hours: 24,
            cl_info: String::new(),
            candidates: Vec::new(),
            project_rows: Vec::new(),
            projects_status: "No project scan yet. Analysis is read-only.".into(),
            docker_data: None,
            docker_summary: "Refresh Docker to inspect the current context.".into(),
            docker_tab: 0,
            container_filter: 0,
            image_days: 7,
            include_unused: true,
            image_plan: None,
            image_info: "Preview includes tagged images not referenced by containers. No volumes are deleted.".into(),
            docker_disk: String::new(),
            docker_loading: false,
            compose_rows: Vec::new(),
            st_scope: 0,
            st_months: 12,
            st_type: 0,
            st_sort: 0,
            st_info: "Run a scan to see results. No files are changed.".into(),
            st_rows: Vec::new(),
            st_display: Vec::new(),
            st_mode: "files".into(),
            st_cancel: None,
            media_summary: "No scan yet".into(),
            history_rows: Vec::new(),
            sched_info: String::new(),
            sched_loaded: false,
        };
        app.show_rule();
        app.refresh_locations();
        app.x.map()?;
        Ok(app)
    }

    fn theme_name(&self) -> String {
        self.cfg
            .get("theme")
            .and_then(Value::as_str)
            .unwrap_or("Niru Noir")
            .to_string()
    }

    fn reload_theme(&mut self) {
        let base = theme::config_home();
        self.desktop_theme = self
            .x
            .root_theme()
            .unwrap_or_else(|| theme::configured_theme(&base));
        let c = theme::colors(&self.theme_name(), &base, &self.desktop_theme);
        if c != self.c {
            self.c = c;
            let _ = self.x.set_icon(&crate::logo::window_icons(&c));
            if self.theme_name() == theme::FOLLOW {
                self.status = "NIRUX theme changed; palette updated".into();
            }
            self.dirty = true;
        }
    }

    pub fn save_cfg(&mut self) {
        if let Err(e) = crate::config::save(&self.cfg) {
            self.status = format!("Error: {e}");
            self.info("Saving failed", &e);
        }
    }

    // ------------------------------------------------------------- jobs

    /// Run `task` on a worker thread and `done` with its result here.
    pub fn task<T: Send + 'static>(
        &mut self,
        storage: bool,
        task: impl FnOnce(&Ctx) -> R<T> + Send + 'static,
        done: impl FnOnce(&mut App, T) + Send + 'static,
    ) {
        let cancel = Arc::new(AtomicBool::new(false));
        if storage {
            self.st_cancel = Some(cancel.clone());
        }
        let ctx = Ctx {
            cancel,
            tx: self.tx.clone(),
            wake: self.wake,
        };
        self.jobs += 1;
        self.status = "Analysis running in the background…".into();
        self.dirty = true;
        std::thread::spawn(move || {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| task(&ctx)))
                .unwrap_or_else(|_| Err("internal error in the background task".into()));
            let cancelled = ctx.cancel.load(Ordering::Relaxed);
            ctx.send(Box::new(move |app: &mut App| {
                app.jobs = app.jobs.saturating_sub(1);
                if storage {
                    app.st_cancel = None;
                }
                if cancelled {
                    app.status = "Scan cancelled · no files changed".into();
                    if storage {
                        app.st_info = "Scan cancelled · no files changed".into();
                    }
                    return;
                }
                match r {
                    Ok(v) => {
                        app.status = "Analysis complete".into();
                        done(app, v);
                    }
                    Err(e) => {
                        app.status = format!("Error: {e}");
                        app.info("Analysis failed", &e);
                    }
                }
            }));
        });
    }

    fn worker_messages(&mut self) {
        while let Ok(d) = self.rx.try_recv() {
            d(self);
            self.dirty = true;
        }
    }

    // ---------------------------------------------------------- dialogs

    pub fn info(&mut self, title: &str, text: &str) {
        self.dialogs
            .push_back(Dialog::new(title, text, DlgKind::Info));
        self.dirty = true;
    }

    pub fn info_detail(&mut self, title: &str, text: &str, detail: &str) {
        let mut d = Dialog::new(title, text, DlgKind::Info);
        d.detail = detail.lines().map(str::to_string).collect();
        self.dialogs.push_back(d);
        self.dirty = true;
    }

    pub fn confirm(&mut self, title: &str, text: &str, yes: impl FnOnce(&mut App) + 'static) {
        let mut d = Dialog::new(title, text, DlgKind::Confirm);
        d.on_ok = Some(Box::new(move |app, _| yes(app)));
        self.dialogs.push_front(d);
        self.dirty = true;
    }

    pub fn ask_text(
        &mut self,
        title: &str,
        text: &str,
        initial: &str,
        ok: impl FnOnce(&mut App, String) + 'static,
    ) {
        let mut d = Dialog::new(title, text, DlgKind::Input);
        d.field.set(initial);
        d.on_ok = Some(Box::new(move |app, v| {
            if let DlgValue::Text(s) = v {
                ok(app, s)
            }
        }));
        self.dialogs.push_front(d);
        self.dirty = true;
    }

    pub fn ask_choice(
        &mut self,
        title: &str,
        text: &str,
        items: &[&str],
        ok: impl FnOnce(&mut App, usize) + 'static,
    ) {
        let mut d = Dialog::new(
            title,
            text,
            DlgKind::Choice(items.iter().map(|s| s.to_string()).collect(), 0),
        );
        d.on_ok = Some(Box::new(move |app, v| {
            if let DlgValue::Index(i) = v {
                ok(app, i)
            }
        }));
        self.dialogs.push_front(d);
        self.dirty = true;
    }

    pub fn ask_age(&mut self, current: i64, ok: impl FnOnce(&mut App, i64) + 'static) {
        let (amount, unit) = if current % 168 == 0 {
            (current / 168, 2)
        } else if current % 24 == 0 {
            (current / 24, 1)
        } else {
            (current, 0)
        };
        let mut d = Dialog::new("File age", "Amount", DlgKind::Age(unit));
        d.field.set(&amount.clamp(1, 3650).to_string());
        d.on_ok = Some(Box::new(move |app, v| {
            if let DlgValue::Hours(h) = v {
                ok(app, h)
            }
        }));
        self.dialogs.push_front(d);
        self.dirty = true;
    }

    pub fn pick(
        &mut self,
        mode: PickMode,
        title: &str,
        start: &Path,
        name: &str,
        ok: impl FnOnce(&mut App, PathBuf) + 'static,
    ) {
        let p = Picker::new(mode, start, name);
        let mut d = Dialog::new(title, "", DlgKind::Pick(p.clone()));
        d.field.set(&p.field_path());
        d.kind = DlgKind::Pick(p);
        d.on_ok = Some(Box::new(move |app, v| {
            if let DlgValue::Path(p) = v {
                ok(app, p)
            }
        }));
        self.dialogs.push_front(d);
        self.dirty = true;
    }

    fn dialog_accept(&mut self) {
        let Some(d) = self.dialogs.front_mut() else {
            return;
        };
        let value = match &mut d.kind {
            DlgKind::Info | DlgKind::Confirm => DlgValue::None,
            DlgKind::Input => DlgValue::Text(d.field.text.clone()),
            DlgKind::Choice(_, i) => DlgValue::Index(*i),
            DlgKind::Age(unit) => {
                let n: i64 = d.field.text.trim().parse().unwrap_or(0);
                if !(1..=3650).contains(&n) {
                    d.text = "Amount (1 to 3650)".into();
                    self.dirty = true;
                    return;
                }
                DlgValue::Hours(n * [1, 24, 168][*unit])
            }
            DlgKind::Pick(p) => {
                let path = PathBuf::from(crate::config::expand_user(d.field.text.trim()));
                match p.mode {
                    PickMode::Dir if !path.is_dir() => {
                        if path.as_os_str().is_empty() {
                            return;
                        }
                        d.text = "Not a folder".into();
                        self.dirty = true;
                        return;
                    }
                    PickMode::OpenZip if !path.is_file() => {
                        if path.is_dir() {
                            p.enter(path);
                            d.field.set(&p.field_path());
                        } else {
                            d.text = "Choose a .zip file".into();
                        }
                        self.dirty = true;
                        return;
                    }
                    PickMode::SaveZip if path.is_dir() => {
                        p.enter(path);
                        d.field.set(&p.field_path());
                        self.dirty = true;
                        return;
                    }
                    _ => DlgValue::Path(path),
                }
            }
        };
        let d = self.dialogs.pop_front().unwrap();
        self.dirty = true;
        if let Some(f) = d.on_ok {
            f(self, value);
        }
    }

    fn dialog_cancel(&mut self) {
        self.dialogs.pop_front();
        self.dirty = true;
    }

    // ------------------------------------------------------------- loop

    pub fn run(&mut self) -> Res<()> {
        let xfd = {
            use std::os::fd::AsRawFd;
            self.x.c.stream().as_raw_fd()
        };
        while !self.quit {
            while let Some(ev) = self.x.c.poll_for_event()? {
                self.event(ev)?;
            }
            if self.quit {
                break;
            }
            if self.smoke_until.is_some_and(|t| Instant::now() >= t) {
                break;
            }
            if self.dirty {
                self.draw();
                self.x.present(&self.cv)?;
                self.dirty = false;
                if !self.shown_once {
                    self.shown_once = true;
                    if std::env::var_os("NIRUFLOW_TIMING").is_some() {
                        eprintln!(
                            "niruflow: first frame after {:.1} ms",
                            self.started.elapsed().as_secs_f64() * 1000.0
                        );
                    }
                    // The overview reads the history after the first frame.
                    self.refresh_overview();
                }
                if std::env::var_os("NIRUFLOW_HITS").is_some() {
                    self.dump_hits();
                }
                continue;
            }
            self.x.c.flush()?;
            let timeout = self
                .smoke_until
                .map(|t| t.saturating_duration_since(Instant::now()).as_millis() as i32 + 1)
                .unwrap_or(-1);
            let mut fds = [
                libc::pollfd {
                    fd: xfd,
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: self.wake,
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: self.inotify,
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            let n = if self.inotify >= 0 { 3 } else { 2 };
            unsafe { libc::poll(fds.as_mut_ptr(), n, timeout) };
            if fds[1].revents & libc::POLLIN != 0 {
                let mut v = 0u64;
                unsafe { libc::read(self.wake, &mut v as *mut u64 as *mut libc::c_void, 8) };
                self.worker_messages();
            }
            if n == 3 && fds[2].revents & libc::POLLIN != 0 {
                let mut buf = [0u8; 4096];
                while unsafe {
                    libc::read(
                        self.inotify,
                        buf.as_mut_ptr() as *mut libc::c_void,
                        buf.len(),
                    )
                } > 0
                {}
                self.reload_theme();
            }
        }
        Ok(())
    }

    /// For the end-to-end test: where the clickable things are.
    fn dump_hits(&self) {
        for (r, h) in &self.hits {
            let name = match h {
                Hit::Nav(i) => format!("nav:{}", PAGES[*i].replace(' ', "_")),
                Hit::Btn(b) => format!("btn:{b:?}"),
                Hit::Combo(c) => format!("combo:{c:?}"),
                Hit::PopItem(i) => format!("pop:{i}"),
                Hit::Check(c) => format!("check:{c:?}"),
                Hit::Field(f) => format!("field:{f:?}"),
                Hit::Row(t, i) => format!("row:{t:?}:{i}"),
                Hit::Tab(i) => format!("tab:{i}"),
                Hit::Dlg(d) => format!("dlg:{d:?}"),
                _ => continue,
            };
            eprintln!("hit {name} {} {}", r.x + r.w / 2, r.y + r.h / 2);
        }
        eprintln!("hits-end");
    }

    fn try_close(&mut self) {
        if self.jobs > 0 {
            self.info(
                "Analysis in progress",
                "Wait until the current analysis finishes before closing the window.",
            );
            return;
        }
        self.quit = true;
    }

    fn event(&mut self, ev: Event) -> Res<()> {
        match ev {
            Event::Expose(e) if e.count == 0 => self.dirty = true,
            Event::ConfigureNotify(e) => {
                if e.width as i32 != self.cv.w || e.height as i32 != self.cv.h {
                    self.cv.resize(e.width as i32, e.height as i32);
                    self.dirty = true;
                }
            }
            Event::ClientMessage(e) => {
                if e.type_ == self.x.atoms.wm_protocols
                    && e.data.as_data32()[0] == self.x.atoms.wm_delete
                {
                    self.try_close();
                    self.dirty = true;
                }
            }
            Event::PropertyNotify(e) => {
                if e.window == self.x.root && e.atom == self.x.atoms.niruwm_theme {
                    self.reload_theme();
                }
            }
            Event::SelectionRequest(e) => self.answer_selection(&e),
            Event::SelectionNotify(e) => {
                if e.property != x11rb::NONE {
                    if let Some(data) = self.x.take_selection() {
                        let s = String::from_utf8_lossy(&data).replace(['\n', '\r'], " ");
                        self.paste(&s);
                    }
                }
            }
            Event::MotionNotify(e) => {
                self.mouse = (e.event_x as i32, e.event_y as i32);
                let h = self.hit_at(self.mouse.0, self.mouse.1);
                if let Some(p) = &mut self.popup {
                    if let Some(Hit::PopItem(i)) = h {
                        if p.hover != i {
                            p.hover = i;
                            self.dirty = true;
                        }
                    }
                }
                if h != self.hover {
                    self.hover = h;
                    self.dirty = true;
                }
            }
            Event::LeaveNotify(_) => {
                if self.hover.is_some() {
                    self.hover = None;
                    self.dirty = true;
                }
            }
            Event::ButtonPress(e) => {
                let (ctrl, shift, _) = event_mods(e.state);
                self.press(e.event_x as i32, e.event_y as i32, e.detail, ctrl, shift);
                self.dirty = true;
            }
            Event::KeyPress(e) => {
                let ks = self.x.keysym(e.detail, e.state);
                let (ctrl, shift, alt) = event_mods(e.state);
                self.key(ks, ctrl, shift, alt);
                self.dirty = true;
            }
            _ => {}
        }
        Ok(())
    }

    fn answer_selection(&mut self, e: &SelectionRequestEvent) {
        let text = self.clipboard.clone();
        let _ = self.x.answer_text(e, &text);
    }

    fn copy(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        self.clipboard = text;
        let _ = self.x.own_clipboard();
        self.status = "Copied to the clipboard".into();
    }

    fn paste(&mut self, s: &str) {
        if let Some(d) = self.dialogs.front_mut() {
            if !matches!(
                d.kind,
                DlgKind::Info | DlgKind::Confirm | DlgKind::Choice(..)
            ) {
                d.field.insert(s);
            }
            self.dirty = true;
            return;
        }
        if let Some(Focus::Field(f)) = self.focus {
            self.fields[f as usize].insert(s);
            self.field_changed(f);
            self.dirty = true;
        }
    }

    fn hit_at(&self, x: i32, y: i32) -> Option<Hit> {
        self.hits
            .iter()
            .rev()
            .find(|(r, _)| r.contains(x, y))
            .map(|(_, h)| *h)
    }

    fn double(&mut self, h: Hit) -> bool {
        let now = Instant::now();
        let dbl = self
            .last_click
            .is_some_and(|(p, t)| p == h && now.duration_since(t) < Duration::from_millis(400));
        self.last_click = if dbl { None } else { Some((h, now)) };
        dbl
    }

    fn press(&mut self, x: i32, y: i32, button: u8, ctrl: bool, shift: bool) {
        let h = self.hit_at(x, y);
        if button == 4 || button == 5 {
            let d: i64 = if button == 4 { -3 } else { 3 };
            self.wheel(h, d);
            return;
        }
        if let Some(p) = &self.popup {
            if let Some(Hit::PopItem(i)) = h {
                let owner = self.popup.take().map(|p| p.owner);
                self.pop_pick(owner, i);
            } else if !p.rect.contains(x, y) {
                self.popup = None;
            }
            return;
        }
        let Some(h) = h else {
            if self.dialogs.is_empty() {
                self.focus = None;
            }
            return;
        };
        if !self.dialogs.is_empty() {
            if button == 1 {
                self.dialog_click(h);
            }
            return;
        }
        match (button, h) {
            (1, Hit::Nav(i)) => self.navigate(i),
            (1, Hit::Btn(b)) => self.button(b),
            (1, Hit::Combo(c)) => self.open_combo(c),
            (1, Hit::Check(c)) => self.toggle_check(c),
            (1, Hit::Field(f)) => {
                self.focus = Some(Focus::Field(f));
                let fx = self.field_cursor_at(f, x);
                self.fields[f as usize].cur = fx;
            }
            (1, Hit::Row(t, pos)) => {
                self.focus = Some(Focus::Table(t));
                self.tables[t as usize].click(pos, ctrl, shift);
                self.table_selection_changed(t);
            }
            (3, Hit::Row(t, pos)) if t == TableId::Storage => {
                self.focus = Some(Focus::Table(t));
                let tb = &mut self.tables[t as usize];
                if !tb.rows.get(pos).is_some_and(|r| tb.sel.contains(&r.id)) {
                    tb.click(pos, false, false);
                }
                self.context_menu(x, y);
            }
            (1, Hit::Header(t, col)) => {
                self.tables[t as usize].header_click(col);
            }
            (1, Hit::Body(t)) => self.focus = Some(Focus::Table(t)),
            (1, Hit::Tab(i)) => self.docker_tab = i,
            _ => {}
        }
    }

    fn field_cursor_at(&mut self, f: FieldId, x: i32) -> usize {
        let Some(r) = self
            .hits
            .iter()
            .find(|(_, h)| *h == Hit::Field(f))
            .map(|(r, _)| *r)
        else {
            return 0;
        };
        let text = self.fields[f as usize].text.clone();
        let mut px = r.x + self.m.pad / 2;
        for (i, ch) in text.chars().enumerate() {
            let a = self.text.advance(ch, self.m.fs);
            if (px as f32 + a / 2.0) as i32 > x {
                return i;
            }
            px += a as i32;
        }
        text.chars().count()
    }

    fn wheel(&mut self, h: Option<Hit>, d: i64) {
        if let Some(p) = &mut self.popup {
            let _ = p;
            return;
        }
        if let Some(dl) = self.dialogs.front_mut() {
            match &mut dl.kind {
                DlgKind::Pick(p) => {
                    let max = p.entries.len().saturating_sub(1) as i64;
                    p.top = (p.top as i64 + d).clamp(0, max) as usize;
                }
                _ => {
                    let max = dl.detail.len().saturating_sub(1) as i64;
                    dl.detail_top = (dl.detail_top as i64 + d).clamp(0, max) as usize;
                }
            }
            return;
        }
        match h {
            Some(Hit::Row(t, _)) | Some(Hit::Body(t)) | Some(Hit::Header(t, _)) => {
                self.tables[t as usize].scroll(d)
            }
            _ => {
                let max = (self.page_h - self.content.h).max(0);
                let s = &mut self.page_scroll[self.page];
                *s = (*s + d as i32 * self.m.row_h).clamp(0, max);
            }
        }
    }

    fn context_menu(&mut self, x: i32, y: i32) {
        let items: Vec<String> = [
            "Open directory",
            "Move selected to quarantine",
            "Delete permanently",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let w = items
            .iter()
            .map(|s| self.text.width(s, self.m.fs))
            .max()
            .unwrap_or(100)
            + 2 * self.m.pad;
        let h = items.len() as i32 * self.m.row_h + 4;
        let x = x.min(self.cv.w - w - 2).max(0);
        let y = y.min(self.cv.h - h - 2).max(0);
        self.popup = Some(Popup {
            owner: PopOwner::Context,
            items,
            rect: Rect::new(x, y, w, h),
            hover: 0,
        });
    }

    fn open_combo(&mut self, id: ComboId) {
        let Some(r) = self
            .hits
            .iter()
            .find(|(_, h)| *h == Hit::Combo(id))
            .map(|(r, _)| *r)
        else {
            return;
        };
        let items = self.combo_items(id);
        if items.is_empty() {
            return;
        }
        let w = items
            .iter()
            .map(|s| self.text.width(s, self.m.fs))
            .max()
            .unwrap_or(0)
            .max(r.w - 2 * self.m.pad)
            + 2 * self.m.pad;
        let h = items.len() as i32 * self.m.row_h + 4;
        let mut y = r.y + r.h;
        if y + h > self.cv.h {
            y = (r.y - h).max(0);
        }
        let x = r.x.min(self.cv.w - w).max(0);
        let hover = self.combo_index(id);
        self.popup = Some(Popup {
            owner: PopOwner::Combo(id),
            items,
            rect: Rect::new(x, y, w, h),
            hover,
        });
    }

    fn pop_pick(&mut self, owner: Option<PopOwner>, i: usize) {
        match owner {
            Some(PopOwner::Combo(c)) => self.combo_pick(c, i),
            Some(PopOwner::Context) => match i {
                0 => self.button(Btn::StorageOpenDir),
                1 => self.button(Btn::StorageQuarantine),
                _ => self.button(Btn::StorageDelete),
            },
            None => {}
        }
    }

    fn dialog_click(&mut self, h: Hit) {
        let Hit::Dlg(d) = h else { return };
        let dbl = self.double(h);
        match d {
            DlgHit::Ok => self.dialog_accept(),
            DlgHit::Cancel => self.dialog_cancel(),
            DlgHit::Item(i) => {
                if let Some(DlgKind::Choice(_, sel)) = self.dialogs.front_mut().map(|d| &mut d.kind)
                {
                    *sel = i;
                }
                if dbl {
                    self.dialog_accept();
                }
            }
            DlgHit::Unit(u) => {
                if let Some(DlgKind::Age(unit)) = self.dialogs.front_mut().map(|d| &mut d.kind) {
                    *unit = u;
                }
            }
            DlgHit::Field | DlgHit::List | DlgHit::Detail => {}
            DlgHit::Up | DlgHit::Home | DlgHit::Hidden | DlgHit::Entry(_) => {
                let Some(dl) = self.dialogs.front_mut() else {
                    return;
                };
                let DlgKind::Pick(p) = &mut dl.kind else {
                    return;
                };
                match d {
                    DlgHit::Up => p.up(),
                    DlgHit::Home => p.enter(crate::config::home()),
                    DlgHit::Hidden => {
                        p.hidden = !p.hidden;
                        p.list();
                    }
                    DlgHit::Entry(i) => {
                        let Some((name, is_dir)) = p.entries.get(i).cloned() else {
                            return;
                        };
                        let path = p.dir.join(&name);
                        if is_dir && dbl {
                            p.enter(path);
                            dl.field.set(&p.field_path());
                            return;
                        }
                        p.sel = Some(i);
                        match (p.mode, is_dir) {
                            (PickMode::SaveZip, true) => {
                                dl.field.set(&path.join(&p.name).to_string_lossy())
                            }
                            (PickMode::SaveZip, false) => {
                                p.name = name.clone();
                                dl.field.set(&path.to_string_lossy());
                            }
                            _ => dl.field.set(&path.to_string_lossy()),
                        }
                        if !is_dir && dbl {
                            self.dialog_accept();
                        }
                        return;
                    }
                    _ => {}
                }
                dl.field.set(&p.field_path());
            }
        }
    }

    fn dialog_key(&mut self, ks: u32, ctrl: bool) {
        let Some(d) = self.dialogs.front_mut() else {
            return;
        };
        match ks {
            0xff1b => return self.dialog_cancel(),
            0xff0d | 0xff8d => {
                if matches!(d.kind, DlgKind::Confirm) && !d.yes_focus {
                    return self.dialog_cancel();
                }
                return self.dialog_accept();
            }
            0xff09 | 0xfe20 | 0xff51 | 0xff53 if matches!(d.kind, DlgKind::Confirm) => {
                d.yes_focus = !d.yes_focus;
                return;
            }
            _ => {}
        }
        if ctrl && (ks == 0x76 || ks == 0x56) {
            let _ = self.x.request_clipboard(self.x.atoms.utf8);
            return;
        }
        let d = self.dialogs.front_mut().unwrap();
        match &mut d.kind {
            DlgKind::Info => match ks {
                0xff54 => d.detail_top = (d.detail_top + 1).min(d.detail.len().saturating_sub(1)),
                0xff52 => d.detail_top = d.detail_top.saturating_sub(1),
                0xff56 => d.detail_top = (d.detail_top + 10).min(d.detail.len().saturating_sub(1)),
                0xff55 => d.detail_top = d.detail_top.saturating_sub(10),
                _ => {}
            },
            DlgKind::Confirm => match ks {
                0x79 | 0x59 => self.dialog_accept(),
                0x6e | 0x4e => self.dialog_cancel(),
                _ => {}
            },
            DlgKind::Choice(items, sel) => match ks {
                0xff54 => *sel = (*sel + 1).min(items.len().saturating_sub(1)),
                0xff52 => *sel = sel.saturating_sub(1),
                _ => {}
            },
            DlgKind::Age(unit) if ks == 0xff52 || ks == 0xff54 => {
                let n: i64 = d.field.text.trim().parse().unwrap_or(1);
                let n = if ks == 0xff52 { n + 1 } else { n - 1 }.clamp(1, 3650);
                d.field.set(&n.to_string());
                let _ = unit;
            }
            DlgKind::Age(_) | DlgKind::Input | DlgKind::Pick(_) => {
                if let DlgKind::Pick(p) = &mut d.kind {
                    if ks == 0x68 && ctrl {
                        p.hidden = !p.hidden;
                        p.list();
                        return;
                    }
                }
                if !d.field.key(ks, ctrl) && !ctrl {
                    if let Some(c) = keysym_char(ks) {
                        if matches!(d.kind, DlgKind::Age(_)) && !c.is_ascii_digit() {
                            return;
                        }
                        d.field.insert(&c.to_string());
                    }
                }
            }
        }
    }

    fn key(&mut self, ks: u32, ctrl: bool, shift: bool, alt: bool) {
        if let Some(p) = &mut self.popup {
            match ks {
                0xff1b => self.popup = None,
                0xff54 => p.hover = (p.hover + 1).min(p.items.len().saturating_sub(1)),
                0xff52 => p.hover = p.hover.saturating_sub(1),
                0xff0d | 0xff8d | 0x20 => {
                    let i = p.hover;
                    let owner = self.popup.take().map(|p| p.owner);
                    self.pop_pick(owner, i);
                }
                _ => {}
            }
            return;
        }
        if !self.dialogs.is_empty() {
            return self.dialog_key(ks, ctrl);
        }
        // Window-wide keys.
        if ctrl && matches!(ks, 0x71 | 0x51 | 0x77 | 0x57) {
            return self.try_close();
        }
        if (ctrl || alt) && (0x31..=0x39).contains(&ks) {
            return self.navigate((ks - 0x31) as usize);
        }
        if ctrl && ks == 0xff56 {
            return self.navigate((self.page + 1) % PAGES.len());
        }
        if ctrl && ks == 0xff55 {
            return self.navigate((self.page + PAGES.len() - 1) % PAGES.len());
        }
        if ks == 0xffc2 {
            return self.refresh_page();
        }
        if ks == 0xff09 || ks == 0xfe20 {
            return self.cycle_focus(ks == 0xfe20 || shift);
        }
        match self.focus {
            Some(Focus::Field(f)) => {
                match ks {
                    0xff1b => {
                        self.focus = None;
                        return;
                    }
                    0xff0d | 0xff8d => {
                        if f == FieldId::MediaRoots {
                            self.button(Btn::MediaScan);
                        }
                        return;
                    }
                    _ => {}
                }
                if ctrl && (ks == 0x76 || ks == 0x56) {
                    let _ = self.x.request_clipboard(self.x.atoms.utf8);
                    return;
                }
                if ctrl && (ks == 0x63 || ks == 0x43) {
                    let t = self.fields[f as usize].text.clone();
                    self.copy(t);
                    return;
                }
                let fl = &mut self.fields[f as usize];
                let mut changed = fl.key(ks, ctrl);
                if !changed && !ctrl && !alt {
                    if let Some(c) = keysym_char(ks) {
                        fl.insert(&c.to_string());
                        changed = true;
                    }
                }
                if changed {
                    self.field_changed(f);
                }
            }
            Some(Focus::Table(t)) => {
                let page = self.tables[t as usize].visible.max(1) as i64;
                let tb = &mut self.tables[t as usize];
                match ks {
                    0xff54 => tb.move_cursor(1, shift),
                    0xff52 => tb.move_cursor(-1, shift),
                    0xff56 => tb.move_cursor(page, shift),
                    0xff55 => tb.move_cursor(-page, shift),
                    0xff50 => tb.move_cursor(-(tb.rows.len() as i64), shift),
                    0xff57 => tb.move_cursor(tb.rows.len() as i64, shift),
                    0x61 | 0x41 if ctrl => tb.select_all(),
                    0x63 | 0x43 if ctrl => {
                        let t = tb.selected_text();
                        self.copy(t);
                        return;
                    }
                    0xff67 if t == TableId::Storage => {
                        let (x, y) = (
                            self.mouse.0.max(self.content.x + 40),
                            self.mouse.1.max(self.content.y + 40),
                        );
                        return self.context_menu(x, y);
                    }
                    0xffc7 if shift && t == TableId::Storage => {
                        let (x, y) = (self.content.x + 80, self.content.y + 120);
                        return self.context_menu(x, y);
                    }
                    0xff1b => {
                        self.focus = None;
                        return;
                    }
                    _ => return,
                }
                self.table_selection_changed(t);
            }
            None => match ks {
                0xff54 | 0xff52 => {
                    if let Some(t) = self.page_tables().first().copied() {
                        self.focus = Some(Focus::Table(t));
                    }
                }
                _ => {
                    if ctrl && (ks == 0x66 || ks == 0x46) {
                        if let Some(f) = self.page_fields().first().copied() {
                            self.focus = Some(Focus::Field(f));
                        }
                    }
                }
            },
        }
    }

    fn cycle_focus(&mut self, back: bool) {
        let mut order: Vec<Focus> = self.page_fields().into_iter().map(Focus::Field).collect();
        order.extend(self.page_tables().into_iter().map(Focus::Table));
        if order.is_empty() {
            return;
        }
        let i = self.focus.and_then(|f| order.iter().position(|x| *x == f));
        let n = order.len();
        let next = match (i, back) {
            (None, false) => 0,
            (None, true) => n - 1,
            (Some(i), false) => (i + 1) % n,
            (Some(i), true) => (i + n - 1) % n,
        };
        self.focus = Some(order[next]);
    }

    // -------------------------------------------------------- drawing

    fn fill(&mut self, r: Rect, c: u32) {
        if !self.measuring {
            self.cv.fill(r, c);
        }
    }

    fn frame(&mut self, r: Rect, c: u32) {
        if !self.measuring {
            self.cv.frame(r, c);
        }
    }

    fn hit(&mut self, r: Rect, h: Hit) {
        if !self.measuring {
            self.hits.push((r, h));
        }
    }

    fn draw_text(&mut self, x: i32, y: i32, s: &str, size: Size, color: u32) -> i32 {
        if self.measuring {
            return self.text.width(s, size);
        }
        self.text.draw(&mut self.cv, x, y, s, size, color)
    }

    /// Run a layout function without drawing to learn its height.
    fn measure(&mut self, f: impl FnOnce(&mut Self, i32) -> i32) -> i32 {
        let was = self.measuring;
        self.measuring = true;
        let h = f(self, 0);
        self.measuring = was;
        h
    }

    /// Word-wrapped text; returns the height used.
    fn para(&mut self, x: i32, y: i32, w: i32, s: &str, color: u32) -> i32 {
        let size = self.m.fs;
        let mut yy = y;
        for raw in s.split('\n') {
            let mut line = String::new();
            for word in raw.split(' ') {
                let cand = if line.is_empty() {
                    word.to_string()
                } else {
                    format!("{line} {word}")
                };
                if !line.is_empty() && self.text.width(&cand, size) > w {
                    self.draw_text(x, yy, &line, size, color);
                    yy += size.height + 2;
                    line = word.to_string();
                } else {
                    line = cand;
                }
            }
            let line = self.text.ellipsize(&line, size, w);
            self.draw_text(x, yy, &line, size, color);
            yy += size.height + 2;
        }
        yy - y
    }

    pub fn label(&mut self, x: i32, y: i32, w: i32, s: &str, muted: bool) -> i32 {
        let c = if muted { self.c.muted } else { self.c.fg };
        self.para(x, y, w, s, c) + self.m.gap / 2
    }

    fn btn_w(&mut self, label: &str) -> i32 {
        self.text.width(label, self.m.fs) + 2 * self.m.pad
    }

    fn draw_btn(&mut self, x: i32, y: i32, label: &str, hit: Option<Hit>) -> i32 {
        let w = self.btn_w(label);
        let r = Rect::new(x, y, w, self.m.bh);
        let hovered = hit.is_some() && hit == self.hover;
        let bg = if hovered { self.c.hover } else { self.c.panel };
        self.fill(r, bg);
        self.frame(r, self.c.border);
        let color = if hit.is_some() {
            self.c.fg
        } else {
            theme::blend(self.c.panel, self.c.muted, 0.45)
        };
        let ty = y + (self.m.bh - self.m.fs.height) / 2;
        self.draw_text(x + self.m.pad, ty, label, self.m.fs, color);
        if let Some(h) = hit {
            self.hit(r, h);
        }
        w
    }

    pub fn fbtn(&mut self, f: &mut Flow, label: &str, hit: Option<Hit>) {
        let w = self.btn_w(label);
        let (x, y) = f.place(w, self.m.bh);
        self.draw_btn(x, y, label, hit);
    }

    /// A row of buttons; returns the y after it.
    pub fn buttons(&mut self, x: i32, y: i32, w: i32, items: &[(&str, Option<Hit>)]) -> i32 {
        let mut f = self.flow(x, y, w);
        for (label, hit) in items {
            self.fbtn(&mut f, label, *hit);
        }
        f.end()
    }

    pub fn flow(&self, x: i32, y: i32, w: i32) -> Flow {
        Flow {
            x0: x,
            x,
            y,
            max: x + w,
            line: 0,
            gap: self.m.gap,
        }
    }

    pub fn flabel(&mut self, f: &mut Flow, s: &str, muted: bool) {
        let w = self.text.width(s, self.m.fs);
        let (x, y) = f.place(w, self.m.bh);
        let c = if muted { self.c.muted } else { self.c.fg };
        self.draw_text(x, y + (self.m.bh - self.m.fs.height) / 2, s, self.m.fs, c);
    }

    pub fn fcombo(&mut self, f: &mut Flow, id: ComboId) {
        let items = self.combo_items(id);
        let cur = self.combo_index(id);
        let text = items.get(cur).cloned().unwrap_or_default();
        let inner = items
            .iter()
            .map(|s| self.text.width(s, self.m.fs))
            .max()
            .unwrap_or(40)
            .min((360.0 * self.m.s) as i32);
        let arrow = self.text.width(" ▾", self.m.fs);
        let w = inner + arrow + 2 * self.m.pad;
        let (x, y) = f.place(w, self.m.bh);
        let r = Rect::new(x, y, w, self.m.bh);
        let h = Hit::Combo(id);
        let bg = if self.hover == Some(h) {
            self.c.hover
        } else {
            self.c.panel
        };
        self.fill(r, bg);
        self.frame(
            r,
            if self.hover == Some(h) {
                self.c.muted
            } else {
                self.c.border
            },
        );
        let t = self.text.ellipsize(&text, self.m.fs, inner);
        let ty = y + (self.m.bh - self.m.fs.height) / 2;
        self.draw_text(x + self.m.pad, ty, &t, self.m.fs, self.c.fg);
        self.draw_text(
            x + w - self.m.pad - arrow,
            ty,
            " ▾",
            self.m.fs,
            self.c.muted,
        );
        self.hit(r, h);
    }

    pub fn fcheck(&mut self, f: &mut Flow, id: CheckId, label: &str, on: bool) {
        let bx = self.m.fs.height;
        let w = bx + self.m.gap + self.text.width(label, self.m.fs);
        let (x, y) = f.place(w, self.m.bh);
        let by = y + (self.m.bh - bx) / 2;
        let r = Rect::new(x, by, bx, bx);
        self.fill(r, self.c.panel);
        let h = Hit::Check(id);
        self.frame(
            r,
            if self.hover == Some(h) {
                self.c.fg
            } else {
                self.c.muted
            },
        );
        if on {
            self.fill(r.inset((bx / 4).max(2)), self.c.fg);
        }
        let ty = y + (self.m.bh - self.m.fs.height) / 2;
        self.draw_text(x + bx + self.m.gap, ty, label, self.m.fs, self.c.fg);
        self.hit(Rect::new(x, y, w, self.m.bh), h);
    }

    /// A text field; `field` is the state, `focused` draws the caret.
    fn draw_field_state(&mut self, r: Rect, field: &Field, focused: bool) {
        self.fill(r, self.c.panel);
        self.frame(
            r,
            if focused {
                self.c.accent
            } else {
                self.c.border
            },
        );
        let ty = r.y + (r.h - self.m.fs.height) / 2;
        let inner = r.w - self.m.pad;
        let x0 = r.x + self.m.pad / 2;
        if field.text.is_empty() {
            let ph = self.text.ellipsize(field.placeholder, self.m.fs, inner);
            self.draw_text(x0, ty, &ph, self.m.fs, self.c.muted);
            if focused {
                self.fill(Rect::new(x0, ty, 1, self.m.fs.height), self.c.fg);
            }
            return;
        }
        // Scroll so the caret stays visible.
        let chars: Vec<char> = field.text.chars().collect();
        let before: String = chars[..field.cur.min(chars.len())].iter().collect();
        let bw = self.text.width(&before, self.m.fs);
        let shift = (bw - inner + 4).max(0);
        if !self.measuring {
            self.cv.set_clip(self.base_clip.intersect(&r.inset(2)));
            self.text.draw(
                &mut self.cv,
                x0 - shift,
                ty,
                &field.text,
                self.m.fs,
                self.c.fg,
            );
            if focused {
                self.cv.fill(
                    Rect::new(x0 - shift + bw, ty, 1, self.m.fs.height),
                    self.c.fg,
                );
            }
            self.cv.set_clip(self.base_clip);
        }
    }

    pub fn ffield(&mut self, f: &mut Flow, id: FieldId, w: i32) {
        let (x, y) = f.place(w, self.m.bh);
        let r = Rect::new(x, y, w, self.m.bh);
        let field = self.fields[id as usize].clone();
        let focused = self.focus == Some(Focus::Field(id));
        self.draw_field_state(r, &field, focused);
        self.hit(r, Hit::Field(id));
    }

    pub fn table(&mut self, r: Rect, id: TableId) {
        let mut t = std::mem::take(&mut self.tables[id as usize]);
        self.draw_table(r, id, &mut t);
        self.tables[id as usize] = t;
    }

    fn column_widths(&mut self, t: &mut Table, avail: i32) -> Vec<i32> {
        let cp = self.m.pad;
        if t.widths_dirty || t.widths.len() != t.cols.len() {
            let mut w: Vec<i32> = t
                .cols
                .iter()
                .map(|c| self.text.width(c, self.m.fs) + self.text.width(" ↓", self.m.fs))
                .collect();
            for row in t.rows.iter().take(3000) {
                for (i, cell) in row.cells.iter().enumerate() {
                    if i < w.len() {
                        // Character count is a cheap upper bound for long cells.
                        let cw = if cell.len() > 80 {
                            cell.chars().count() as i32 * self.text.advance('m', self.m.fs) as i32
                        } else {
                            self.text.width(cell, self.m.fs)
                        };
                        w[i] = w[i].max(cw);
                    }
                }
            }
            t.widths = w;
            t.widths_dirty = false;
        }
        let mut w: Vec<i32> = t.widths.iter().map(|x| x + 2 * cp).collect();
        let total: i32 = w.iter().sum();
        if total <= avail {
            if let Some(last) = w.last_mut() {
                *last += avail - total;
            }
            return w;
        }
        // Water-fill: cap the widest columns so the sum fits.
        let (mut lo, mut hi) = (cp * 3, *w.iter().max().unwrap_or(&avail));
        while lo < hi {
            let mid = (lo + hi + 1) / 2;
            let s: i32 = w.iter().map(|x| (*x).min(mid)).sum();
            if s <= avail {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        for x in w.iter_mut() {
            *x = (*x).min(lo);
        }
        let s: i32 = w.iter().sum();
        if let Some(last) = w.last_mut() {
            *last += (avail - s).max(0);
        }
        w
    }

    fn draw_table(&mut self, r: Rect, id: TableId, t: &mut Table) {
        let rh = self.m.row_h;
        let focused = self.focus == Some(Focus::Table(id));
        self.fill(r, self.c.panel);
        self.frame(r, if focused { self.c.muted } else { self.c.border });
        self.hit(r, Hit::Body(id));
        let sb = (8.0 * self.m.s) as i32;
        let inner = Rect::new(r.x + 1, r.y + 1, r.w - 2 - sb, r.h - 2);
        let widths = self.column_widths(t, inner.w);
        let cp = self.m.pad;
        let ty = |y: i32, m: &Metrics| y + (rh - m.fs.height) / 2;
        // Header.
        let mut x = inner.x;
        for (i, col) in t.cols.clone().iter().enumerate() {
            let w = widths[i];
            let mut label = col.to_string();
            if let Some((c, desc)) = t.sort {
                if c == i {
                    label.push_str(if desc { " ↓" } else { " ↑" });
                }
            }
            let lab = self.text.ellipsize(&label, self.m.fs, w - 2 * cp);
            if !self.measuring {
                self.cv
                    .set_clip(self.base_clip.intersect(&Rect::new(x, inner.y, w, rh)));
            }
            self.draw_text(x + cp, ty(inner.y, &self.m), &lab, self.m.fs, self.c.muted);
            if !self.measuring {
                self.cv.set_clip(self.base_clip);
            }
            if t.sortable {
                self.hit(Rect::new(x, inner.y, w, rh), Hit::Header(id, i));
            }
            x += w;
        }
        self.fill(
            Rect::new(inner.x, inner.y + rh - 1, inner.w + sb, 1),
            self.c.border,
        );
        let body = Rect::new(inner.x, inner.y + rh, inner.w, inner.h - rh);
        let vis = (body.h / rh).max(0) as usize;
        t.visible = vis;
        let max_top = t.rows.len().saturating_sub(vis);
        t.top = t.top.min(max_top);
        let cursor = t.cursor;
        for (k, row) in t.rows.iter().enumerate().skip(t.top).take(vis) {
            let y = body.y + (k - t.top) as i32 * rh;
            let rr = Rect::new(inner.x, y, inner.w, rh);
            let selected = t.sel.contains(&row.id);
            if selected {
                self.fill(rr, self.c.selected);
            } else if k % 2 == 1 {
                self.fill(rr, theme::blend(self.c.panel, self.c.hover, 0.6));
            }
            if focused && cursor == Some(row.id) {
                self.frame(rr, self.c.accent);
            }
            let mut x = inner.x;
            for (i, cell) in row.cells.iter().enumerate() {
                let w = widths.get(i).copied().unwrap_or(0);
                if w <= 0 {
                    continue;
                }
                let s = self.text.ellipsize(cell, self.m.fs, w - 2 * cp);
                self.draw_text(x + cp, ty(y, &self.m), &s, self.m.fs, self.c.fg);
                x += w;
            }
            self.hit(rr, Hit::Row(id, k));
        }
        if t.rows.len() > vis && vis > 0 {
            let track = Rect::new(r.x + r.w - 1 - sb, body.y, sb, body.h);
            let n = t.rows.len() as i64;
            let th = ((body.h as i64 * vis as i64 / n) as i32).max(sb * 2);
            let ty =
                body.y + ((body.h - th) as i64 * t.top as i64 / (n - vis as i64).max(1)) as i32;
            self.fill(track, self.c.panel);
            self.fill(Rect::new(track.x + 2, ty, sb - 4, th), self.c.border);
        }
    }

    pub fn draw(&mut self) {
        self.hits.clear();
        let (w, h) = (self.cv.w, self.cv.h);
        let m = self.m;
        self.cv.fill(Rect::new(0, 0, w, h), self.c.bg);
        // Header: the pixel logo, the theme and Close.
        let px = (2.0 * m.s).round().max(2.0) as i32;
        let hy = m.pad;
        crate::logo::word(
            &mut self.cv,
            m.pad,
            hy + (m.bh - 7 * px) / 2,
            "NIRUFLOW",
            px,
            self.c.accent,
            Some(theme::blend(self.c.bg, self.c.accent, 0.3)),
        );
        let lw = crate::logo::word_width("NIRUFLOW", px);
        self.draw_text(
            m.pad + lw + 2 * m.pad,
            hy + (m.bh - m.fs.height) / 2,
            "LOCAL AUTOMATION",
            m.fs,
            self.c.muted,
        );
        {
            let close_w = self.btn_w("Close");
            let items = self.combo_items(ComboId::Theme);
            let cw = items
                .iter()
                .map(|s| self.text.width(s, m.fs))
                .max()
                .unwrap_or(0)
                + self.text.width(" ▾", m.fs)
                + 2 * m.pad;
            let tw = self.text.width("Theme", m.fs);
            let x0 = w - m.pad - close_w - m.gap - cw - m.gap - tw - m.gap;
            let mut f = self.flow(x0, hy, w);
            self.flabel(&mut f, "Theme", true);
            self.fcombo(&mut f, ComboId::Theme);
            self.fbtn(&mut f, "Close", Some(Hit::Btn(Btn::Close)));
        }
        // Side panel.
        let top = m.header_h;
        let bottom = h - m.status_h;
        let side = Rect::new(m.pad, top, m.side_w, bottom - top - m.gap);
        self.cv.fill(side, self.c.panel);
        self.cv.frame(side, self.c.border);
        let mut y = side.y + m.gap;
        for (i, name) in PAGES.iter().enumerate() {
            let r = Rect::new(side.x + 4, y, side.w - 8, m.row_h + 6);
            let hov = self.hover == Some(Hit::Nav(i));
            if i == self.page || hov {
                self.cv.fill(r, self.c.hover);
            }
            if i == self.page {
                self.cv.fill(Rect::new(r.x, r.y, 2, r.h), self.c.fg);
            }
            let ty = r.y + (r.h - m.fs.height) / 2;
            let c = if i == self.page {
                self.c.fg
            } else {
                theme::blend(self.c.muted, self.c.fg, 0.4)
            };
            self.text.draw(&mut self.cv, r.x + m.pad, ty, name, m.fs, c);
            self.hits.push((r, Hit::Nav(i)));
            y += r.h + 2;
        }
        let about = format!("{}  ·  Ing Leif Nicklas Rudolfsson", crate::VERSION);
        let about = self.text.ellipsize(&about, m.small, side.w - 2 * m.pad);
        self.text.draw(
            &mut self.cv,
            side.x + m.pad,
            side.y + side.h - m.small.height - m.gap,
            &about,
            m.small,
            self.c.muted,
        );
        // Page.
        let cx = side.x + side.w + m.pad;
        self.content = Rect::new(cx, top, w - cx - m.pad, bottom - top - m.gap);
        let content = self.content;
        let scroll = self.page_scroll[self.page];
        self.base_clip = content;
        self.cv.set_clip(content);
        let mut y = content.y - scroll;
        self.text.draw(
            &mut self.cv,
            content.x,
            y,
            PAGES[self.page],
            m.title,
            self.c.fg,
        );
        y += m.title.height + m.gap;
        let bottom_y = content.y + content.h - scroll;
        let end = self.draw_page(content.x, y, content.w, bottom_y);
        self.page_h = end + scroll - content.y;
        self.cv.reset_clip();
        self.base_clip = Rect::new(0, 0, self.cv.w, self.cv.h);
        // Keep hits inside the visible page.
        self.hits.retain(|(r, h)| {
            matches!(
                h,
                Hit::Nav(_) | Hit::Combo(ComboId::Theme) | Hit::Btn(Btn::Close)
            ) || r.intersect(&content).w > 0 && r.intersect(&content).h > 0
        });
        if self.page_h > content.h {
            let sb = (6.0 * m.s) as i32;
            let th = (content.h as i64 * content.h as i64 / self.page_h as i64) as i32;
            let ty = content.y + (content.h - th) * scroll / (self.page_h - content.h).max(1);
            self.cv.fill(
                Rect::new(content.x + content.w + m.pad / 2 - sb / 2, ty, sb / 2, th),
                self.c.border,
            );
        }
        // Status bar.
        self.cv.fill(Rect::new(0, bottom, w, 1), self.c.border);
        let st = self.text.ellipsize(&self.status, m.fs, w - 2 * m.pad);
        self.text.draw(
            &mut self.cv,
            m.pad,
            bottom + (m.status_h - m.fs.height) / 2,
            &st,
            m.fs,
            self.c.muted,
        );
        if let Some(p) = self.popup.take() {
            self.draw_popup(&p);
            self.popup = Some(p);
        }
        if let Some(d) = self.dialogs.pop_front() {
            self.hits.clear();
            let d = self.draw_dialog(d);
            self.dialogs.push_front(d);
        }
    }

    fn draw_popup(&mut self, p: &Popup) {
        self.hits.clear();
        self.cv.fill(p.rect, self.c.panel);
        self.cv.frame(p.rect, self.c.muted);
        for (i, item) in p.items.iter().enumerate() {
            let r = Rect::new(
                p.rect.x + 2,
                p.rect.y + 2 + i as i32 * self.m.row_h,
                p.rect.w - 4,
                self.m.row_h,
            );
            if i == p.hover {
                self.cv.fill(r, self.c.selected);
            }
            let ty = r.y + (r.h - self.m.fs.height) / 2;
            self.text.draw(
                &mut self.cv,
                r.x + self.m.pad - 2,
                ty,
                item,
                self.m.fs,
                self.c.fg,
            );
            self.hits.push((r, Hit::PopItem(i)));
        }
    }

    fn draw_dialog(&mut self, mut d: Dialog) -> Dialog {
        let m = self.m;
        let (w, h) = (self.cv.w, self.cv.h);
        self.cv.tint(Rect::new(0, 0, w, h), 0x000000, 110);
        self.hits.push((Rect::new(0, 0, w, h), Hit::Shade));
        let wide = matches!(d.kind, DlgKind::Pick(_)) || !d.detail.is_empty();
        let dw = if wide {
            (760.0 * m.s) as i32
        } else {
            (560.0 * m.s) as i32
        }
        .min(w - 2 * m.pad);
        let inner_w = dw - 2 * m.pad;
        // Height: measure the body first.
        let body_h = {
            let text = d.text.clone();
            self.measure(|a, _| a.para(0, 0, inner_w, &text, 0))
        };
        let extra = match &d.kind {
            DlgKind::Info if !d.detail.is_empty() => (300.0 * m.s) as i32 + m.gap,
            DlgKind::Input | DlgKind::Age(_) => m.bh * 2 + 2 * m.gap,
            DlgKind::Choice(items, _) => items.len() as i32 * (m.row_h + 4) + m.gap,
            DlgKind::Pick(_) => (360.0 * m.s) as i32 + 2 * m.bh + 3 * m.gap,
            _ => 0,
        };
        let dh = (m.title.height + m.gap + body_h + extra + m.bh + 3 * m.pad).min(h - 2 * m.pad);
        let r = Rect::new((w - dw) / 2, (h - dh) / 2, dw, dh);
        self.cv.fill(r, self.c.bg);
        self.cv.frame(r, self.c.muted);
        self.base_clip = r;
        self.cv.set_clip(r);
        let x = r.x + m.pad;
        let mut y = r.y + m.pad;
        let title = self.text.ellipsize(&d.title, m.title, inner_w);
        self.text
            .draw(&mut self.cv, x, y, &title, m.title, self.c.fg);
        y += m.title.height + m.gap;
        let text = d.text.clone();
        y += self.para(x, y, inner_w, &text, self.c.fg);
        let hov = self.hover;
        match &mut d.kind {
            DlgKind::Info if !d.detail.is_empty() => {
                y += m.gap / 2;
                let box_r = Rect::new(x, y, inner_w, (300.0 * m.s) as i32);
                self.cv.fill(box_r, self.c.panel);
                self.cv.frame(box_r, self.c.border);
                self.hits.push((box_r, Hit::Dlg(DlgHit::Detail)));
                let lines = (box_r.h - 8) / (m.small.height + 1);
                self.cv.set_clip(box_r.inset(2));
                for (k, line) in d
                    .detail
                    .iter()
                    .skip(d.detail_top)
                    .take(lines.max(0) as usize)
                    .enumerate()
                {
                    let ly = box_r.y + 4 + k as i32 * (m.small.height + 1);
                    let s = self.text.ellipsize(line, m.small, box_r.w - 12);
                    self.text
                        .draw(&mut self.cv, box_r.x + 6, ly, &s, m.small, self.c.fg);
                }
                self.cv.set_clip(r);
            }
            DlgKind::Input | DlgKind::Age(_) => {
                y += m.gap / 2;
                let fw = if matches!(d.kind, DlgKind::Age(_)) {
                    (120.0 * m.s) as i32
                } else {
                    inner_w
                };
                let fr = Rect::new(x, y, fw, m.bh);
                let field = d.field.clone();
                self.draw_field_state(fr, &field, true);
                self.hits.push((fr, Hit::Dlg(DlgHit::Field)));
                if let DlgKind::Age(unit) = d.kind {
                    let mut f = self.flow(x, y + m.bh + m.gap, inner_w);
                    self.flabel(&mut f, "Unit", true);
                    for (i, name) in ["Hours", "Days", "Weeks"].iter().enumerate() {
                        let label = if i == unit {
                            format!("● {name}")
                        } else {
                            format!("○ {name}")
                        };
                        self.fbtn(&mut f, &label, Some(Hit::Dlg(DlgHit::Unit(i))));
                    }
                }
            }
            DlgKind::Choice(items, sel) => {
                y += m.gap / 2;
                for (i, item) in items.iter().enumerate() {
                    let rr = Rect::new(x, y, inner_w, m.row_h + 2);
                    if i == *sel {
                        self.cv.fill(rr, self.c.selected);
                    } else if hov == Some(Hit::Dlg(DlgHit::Item(i))) {
                        self.cv.fill(rr, self.c.hover);
                    }
                    let mark = if i == *sel { "● " } else { "○ " };
                    let ty = rr.y + (rr.h - m.fs.height) / 2;
                    self.text.draw(
                        &mut self.cv,
                        x + m.pad / 2,
                        ty,
                        &format!("{mark}{item}"),
                        m.fs,
                        self.c.fg,
                    );
                    self.hits.push((rr, Hit::Dlg(DlgHit::Item(i))));
                    y += m.row_h + 4;
                }
            }
            DlgKind::Pick(p) => {
                let mut f = self.flow(x, y, inner_w);
                let hidden = if p.hidden {
                    "Hide hidden"
                } else {
                    "Show hidden"
                };
                self.fbtn(&mut f, "Up", Some(Hit::Dlg(DlgHit::Up)));
                self.fbtn(&mut f, "Home", Some(Hit::Dlg(DlgHit::Home)));
                self.fbtn(&mut f, hidden, Some(Hit::Dlg(DlgHit::Hidden)));
                y = f.end();
                let fr = Rect::new(x, y, inner_w, m.bh);
                let field = d.field.clone();
                self.draw_field_state(fr, &field, true);
                self.hits.push((fr, Hit::Dlg(DlgHit::Field)));
                y += m.bh + m.gap;
                let lr = Rect::new(x, y, inner_w, (360.0 * m.s) as i32 - m.bh);
                self.cv.fill(lr, self.c.panel);
                self.cv.frame(lr, self.c.border);
                self.hits.push((lr, Hit::Dlg(DlgHit::List)));
                let vis = ((lr.h - 4) / m.row_h).max(1) as usize;
                p.top = p.top.min(p.entries.len().saturating_sub(vis));
                if p.entries.is_empty() {
                    self.text.draw(
                        &mut self.cv,
                        lr.x + m.pad,
                        lr.y + 4,
                        "(empty)",
                        m.fs,
                        self.c.muted,
                    );
                }
                for (k, (name, is_dir)) in p.entries.iter().enumerate().skip(p.top).take(vis) {
                    let rr = Rect::new(
                        lr.x + 2,
                        lr.y + 2 + (k - p.top) as i32 * m.row_h,
                        lr.w - 4,
                        m.row_h,
                    );
                    if p.sel == Some(k) {
                        self.cv.fill(rr, self.c.selected);
                    } else if hov == Some(Hit::Dlg(DlgHit::Entry(k))) {
                        self.cv.fill(rr, self.c.hover);
                    }
                    let label = if *is_dir {
                        format!("▸ {name}/")
                    } else {
                        format!("  {name}")
                    };
                    let s = self.text.ellipsize(&label, m.fs, rr.w - m.pad);
                    let ty = rr.y + (rr.h - m.fs.height) / 2;
                    self.text.draw(
                        &mut self.cv,
                        rr.x + m.pad / 2,
                        ty,
                        &s,
                        m.fs,
                        if *is_dir { self.c.fg } else { self.c.muted },
                    );
                    self.hits.push((rr, Hit::Dlg(DlgHit::Entry(k))));
                }
            }
            _ => {}
        }
        // Buttons, bottom right.
        let by = r.y + r.h - m.pad - m.bh;
        let (ok, cancel) = match &d.kind {
            DlgKind::Info => ("OK", None),
            DlgKind::Confirm => ("Yes", Some("No")),
            DlgKind::Pick(p) => (
                match p.mode {
                    PickMode::Dir => "Choose folder",
                    PickMode::OpenZip => "Open",
                    PickMode::SaveZip => "Save",
                },
                Some("Cancel"),
            ),
            _ => ("OK", Some("Cancel")),
        };
        let okw = self.btn_w(ok);
        let cw = cancel.map(|c| self.btn_w(c)).unwrap_or(0);
        let mut bx = r.x + r.w - m.pad - okw;
        if let Some(c) = cancel {
            bx -= cw + m.gap;
            self.draw_btn(bx + okw + m.gap, by, c, Some(Hit::Dlg(DlgHit::Cancel)));
            if matches!(d.kind, DlgKind::Confirm) && !d.yes_focus {
                self.cv.frame(
                    Rect::new(bx + okw + m.gap - 2, by - 2, cw + 4, m.bh + 4),
                    self.c.accent,
                );
            }
        }
        self.draw_btn(bx, by, ok, Some(Hit::Dlg(DlgHit::Ok)));
        if !matches!(d.kind, DlgKind::Confirm) || d.yes_focus {
            self.cv
                .frame(Rect::new(bx - 2, by - 2, okw + 4, m.bh + 4), self.c.accent);
        }
        self.cv.reset_clip();
        d
    }
}

fn metrics(text: &Text, font_px: f32, s: f32) -> Metrics {
    let fs = text.size(font_px);
    let small = text.size((font_px * 0.88).round());
    let title = text.size((font_px * 1.45).round());
    let pad = (12.0 * s) as i32;
    let bh = fs.height + (14.0 * s) as i32;
    Metrics {
        s,
        fs,
        small,
        title,
        row_h: fs.height + (8.0 * s) as i32,
        bh,
        pad,
        gap: (8.0 * s) as i32,
        side_w: (200.0 * s) as i32,
        header_h: bh + 2 * pad,
        status_h: fs.height + (12.0 * s) as i32,
    }
}

/// Open a folder with the desktop's handler, without waiting for it.
pub fn open_folder(dir: &Path) {
    let dir = dir.to_path_buf();
    std::thread::spawn(move || {
        let _ = std::process::Command::new("xdg-open")
            .arg(&dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    });
}
