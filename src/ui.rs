//! Widget state for the canvas UI: sortable multi-select tables, single
//! line text fields and the folder/file picker. Drawing lives in app.rs.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub enum Key {
    N(f64),
    S(String),
}

#[derive(Clone, Debug)]
pub struct Row {
    /// Stable identity (an index into the page's data).
    pub id: usize,
    pub cells: Vec<String>,
    /// Sort keys per column; empty means sort by the cell text.
    pub keys: Vec<Key>,
}

impl Row {
    pub fn new(id: usize, cells: Vec<String>) -> Self {
        Row {
            id,
            cells,
            keys: Vec::new(),
        }
    }
    pub fn keyed(id: usize, cells: Vec<String>, keys: Vec<Key>) -> Self {
        Row { id, cells, keys }
    }
    fn key(&self, col: usize) -> Key {
        self.keys
            .get(col)
            .cloned()
            .unwrap_or_else(|| Key::S(self.cells.get(col).cloned().unwrap_or_default()))
    }
}

#[derive(Default, Debug)]
pub struct Table {
    pub cols: Vec<&'static str>,
    pub rows: Vec<Row>,
    pub sel: BTreeSet<usize>,
    pub cursor: Option<usize>,
    anchor: Option<usize>,
    pub top: usize,
    pub sortable: bool,
    pub sort: Option<(usize, bool)>,
    pub multi: bool,
    pub widths: Vec<i32>,
    pub widths_dirty: bool,
    pub visible: usize,
}

impl Table {
    pub fn new(cols: &[&'static str], multi: bool, sortable: bool) -> Self {
        Table {
            cols: cols.to_vec(),
            multi,
            sortable,
            widths_dirty: true,
            ..Default::default()
        }
    }

    /// Replace the rows (selection cleared, sort kept, scrolled to the top).
    pub fn set_rows(&mut self, rows: Vec<Row>) {
        self.rows = rows;
        self.sel.clear();
        self.cursor = None;
        self.anchor = None;
        self.top = 0;
        self.widths_dirty = true;
        self.apply_sort();
    }

    pub fn clear(&mut self) {
        self.set_rows(Vec::new());
    }

    pub fn apply_sort(&mut self) {
        let Some((col, desc)) = self.sort else { return };
        self.rows.sort_by(|a, b| {
            let o = match (a.key(col), b.key(col)) {
                (Key::N(x), Key::N(y)) => x.total_cmp(&y),
                (Key::S(x), Key::S(y)) => x.cmp(&y),
                (Key::N(_), Key::S(_)) => std::cmp::Ordering::Less,
                (Key::S(_), Key::N(_)) => std::cmp::Ordering::Greater,
            };
            if desc {
                o.reverse()
            } else {
                o
            }
        });
    }

    pub fn header_click(&mut self, col: usize) {
        if !self.sortable {
            return;
        }
        self.sort = match self.sort {
            Some((c, false)) if c == col => Some((col, true)),
            _ => Some((col, false)),
        };
        self.apply_sort();
    }

    pub fn pos_of(&self, id: usize) -> Option<usize> {
        self.rows.iter().position(|r| r.id == id)
    }

    /// Selected row ids in display order.
    pub fn selected(&self) -> Vec<usize> {
        self.rows
            .iter()
            .filter(|r| self.sel.contains(&r.id))
            .map(|r| r.id)
            .collect()
    }

    pub fn first_selected(&self) -> Option<usize> {
        self.selected().first().copied()
    }

    pub fn click(&mut self, pos: usize, ctrl: bool, shift: bool) {
        let Some(id) = self.rows.get(pos).map(|r| r.id) else {
            return;
        };
        if self.multi && shift {
            let a = self
                .anchor
                .unwrap_or(pos)
                .min(self.rows.len().saturating_sub(1));
            let (lo, hi) = (a.min(pos), a.max(pos));
            if !ctrl {
                self.sel.clear();
            }
            for r in &self.rows[lo..=hi] {
                self.sel.insert(r.id);
            }
        } else if self.multi && ctrl {
            if !self.sel.remove(&id) {
                self.sel.insert(id);
            }
            self.anchor = Some(pos);
        } else {
            self.sel.clear();
            self.sel.insert(id);
            self.anchor = Some(pos);
        }
        self.cursor = Some(id);
    }

    pub fn select_all(&mut self) {
        if !self.multi {
            return;
        }
        self.sel = self.rows.iter().map(|r| r.id).collect();
    }

    pub fn move_cursor(&mut self, delta: i64, shift: bool) {
        if self.rows.is_empty() {
            return;
        }
        let cur = self
            .cursor
            .and_then(|c| self.pos_of(c))
            .map(|p| p as i64)
            .unwrap_or(-1);
        let n = self.rows.len() as i64;
        let next = if cur < 0 && delta > 0 {
            0
        } else {
            (cur + delta).clamp(0, n - 1)
        } as usize;
        self.click(next, false, shift);
        self.ensure_visible(next);
    }

    pub fn ensure_visible(&mut self, pos: usize) {
        let vis = self.visible.max(1);
        if pos < self.top {
            self.top = pos;
        } else if pos >= self.top + vis {
            self.top = pos + 1 - vis;
        }
    }

    pub fn scroll(&mut self, delta: i64) {
        let max = self.rows.len().saturating_sub(self.visible.max(1)) as i64;
        self.top = (self.top as i64 + delta).clamp(0, max.max(0)) as usize;
    }

    /// Tab-separated text of the selected rows (for the clipboard).
    pub fn selected_text(&self) -> String {
        self.rows
            .iter()
            .filter(|r| self.sel.contains(&r.id))
            .map(|r| r.cells.join("\t"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Clone, Debug, Default)]
pub struct Field {
    pub text: String,
    /// Cursor as a char index.
    pub cur: usize,
    pub placeholder: &'static str,
}

impl Field {
    pub fn new(placeholder: &'static str) -> Self {
        Field {
            placeholder,
            ..Default::default()
        }
    }

    pub fn set(&mut self, s: &str) {
        self.text = s.to_string();
        self.cur = self.text.chars().count();
    }

    fn byte(&self, ci: usize) -> usize {
        self.text
            .char_indices()
            .nth(ci)
            .map(|(b, _)| b)
            .unwrap_or(self.text.len())
    }

    pub fn insert(&mut self, s: &str) {
        let s: String = s.chars().filter(|c| !c.is_control()).collect();
        let b = self.byte(self.cur);
        self.text.insert_str(b, &s);
        self.cur += s.chars().count();
    }

    /// Editing keys; true when the text changed.
    pub fn key(&mut self, ks: u32, ctrl: bool) -> bool {
        let n = self.text.chars().count();
        match ks {
            0xff08 if ctrl => {
                // Delete the word before the cursor.
                let chars: Vec<char> = self.text.chars().collect();
                let mut i = self.cur;
                while i > 0 && chars[i - 1] == ' ' {
                    i -= 1;
                }
                while i > 0 && !matches!(chars[i - 1], ' ' | '/' | '-' | '_' | '.') {
                    i -= 1;
                }
                if i == self.cur && i > 0 {
                    i -= 1;
                }
                let (a, b) = (self.byte(i), self.byte(self.cur));
                self.text.replace_range(a..b, "");
                self.cur = i;
                true
            }
            0xff08 => {
                if self.cur == 0 {
                    return false;
                }
                let (a, b) = (self.byte(self.cur - 1), self.byte(self.cur));
                self.text.replace_range(a..b, "");
                self.cur -= 1;
                true
            }
            0xffff => {
                if self.cur >= n {
                    return false;
                }
                let (a, b) = (self.byte(self.cur), self.byte(self.cur + 1));
                self.text.replace_range(a..b, "");
                true
            }
            0xff51 => {
                self.cur = self.cur.saturating_sub(1);
                false
            }
            0xff53 => {
                self.cur = (self.cur + 1).min(n);
                false
            }
            0xff50 => {
                self.cur = 0;
                false
            }
            0xff57 => {
                self.cur = n;
                false
            }
            0x75 | 0x55 if ctrl => {
                let changed = !self.text.is_empty();
                self.text.clear();
                self.cur = 0;
                changed
            }
            _ => false,
        }
    }
}

/// A printable character for a keysym (Latin-1 and Unicode keysyms).
pub fn keysym_char(ks: u32) -> Option<char> {
    match ks {
        0x20..=0x7e | 0xa0..=0xff => char::from_u32(ks),
        0x0100_0000..=0x0110_ffff => char::from_u32(ks - 0x0100_0000),
        0xffaa..=0xffb9 => match ks {
            0xffaa => Some('*'),
            0xffab => Some('+'),
            0xffad => Some('-'),
            0xffae => Some('.'),
            0xffaf => Some('/'),
            0xffb0..=0xffb9 => char::from_u32(ks - 0xffb0 + '0' as u32),
            _ => None,
        },
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickMode {
    Dir,
    OpenZip,
    SaveZip,
}

#[derive(Clone, Debug)]
pub struct Picker {
    pub mode: PickMode,
    pub dir: PathBuf,
    /// (name, is a directory), folders first.
    pub entries: Vec<(String, bool)>,
    pub sel: Option<usize>,
    pub top: usize,
    pub hidden: bool,
    /// The file name kept while browsing in save mode.
    pub name: String,
}

impl Picker {
    pub fn new(mode: PickMode, start: &Path, name: &str) -> Self {
        let dir = if start.is_dir() {
            start.to_path_buf()
        } else {
            crate::config::home()
        };
        let mut p = Picker {
            mode,
            dir,
            entries: Vec::new(),
            sel: None,
            top: 0,
            hidden: false,
            name: name.to_string(),
        };
        p.list();
        p
    }

    pub fn list(&mut self) {
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if !self.hidden && name.starts_with('.') {
                    continue;
                }
                let p = e.path();
                if p.is_dir() {
                    dirs.push(name);
                } else if self.mode != PickMode::Dir && name.to_lowercase().ends_with(".zip") {
                    files.push(name);
                }
            }
        }
        let key = |s: &String| s.to_lowercase();
        dirs.sort_by_key(key);
        files.sort_by_key(key);
        self.entries = dirs
            .into_iter()
            .map(|d| (d, true))
            .chain(files.into_iter().map(|f| (f, false)))
            .collect();
        self.sel = None;
        self.top = 0;
    }

    /// The path the field should show for the current folder.
    pub fn field_path(&self) -> String {
        match self.mode {
            PickMode::SaveZip => self.dir.join(&self.name).to_string_lossy().into_owned(),
            _ => self.dir.to_string_lossy().into_owned(),
        }
    }

    pub fn enter(&mut self, dir: PathBuf) {
        if dir.is_dir() {
            self.dir = std::fs::canonicalize(&dir).unwrap_or(dir);
            self.list();
        }
    }

    pub fn up(&mut self) {
        if let Some(p) = self.dir.parent().map(Path::to_path_buf) {
            self.enter(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_selection_and_sort() {
        let mut t = Table::new(&["a", "b"], true, true);
        t.set_rows(vec![
            Row::keyed(
                0,
                vec!["x".into(), "10".into()],
                vec![Key::S("x".into()), Key::N(10.0)],
            ),
            Row::keyed(
                1,
                vec!["y".into(), "2".into()],
                vec![Key::S("y".into()), Key::N(2.0)],
            ),
            Row::keyed(
                2,
                vec!["z".into(), "30".into()],
                vec![Key::S("z".into()), Key::N(30.0)],
            ),
        ]);
        t.header_click(1);
        assert_eq!(t.rows.iter().map(|r| r.id).collect::<Vec<_>>(), [1, 0, 2]);
        t.header_click(1);
        assert_eq!(t.rows[0].id, 2);
        t.click(0, false, false);
        t.click(2, false, true);
        assert_eq!(t.selected(), [2, 0, 1]);
        t.click(1, true, false);
        assert_eq!(t.selected(), [2, 1]);
        t.select_all();
        assert_eq!(t.sel.len(), 3);
    }

    #[test]
    fn field_editing() {
        let mut f = Field::new("");
        f.insert("hello wörld");
        assert!(f.key(0xff08, false));
        assert_eq!(f.text, "hello wörl");
        assert!(f.key(0xff08, true));
        assert_eq!(f.text, "hello ");
        f.key(0xff50, false);
        f.insert(">");
        assert_eq!(f.text, ">hello ");
        assert_eq!(keysym_char(0xe5), Some('å'));
    }
}
