//! The X11 side: one window, a keyboard map, and presenting the canvas with
//! MIT-SHM (one shared-memory copy per frame) or, without it, `PutImage` in
//! slices that fit the request size. Adapted from NIRUFM.
use crate::canvas::Canvas;
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::shm::ConnectionExt as _;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

pub type Res<T> = Result<T, Box<dyn std::error::Error>>;

pub struct Atoms {
    pub wm_protocols: Atom,
    pub wm_delete: Atom,
    pub net_wm_name: Atom,
    pub net_wm_icon: Atom,
    pub net_wm_pid: Atom,
    pub utf8: Atom,
    pub niruwm_theme: Atom,
    pub clipboard: Atom,
    pub targets: Atom,
    pub sel_prop: Atom,
}

struct Shm {
    seg: u32,
    addr: *mut u8,
    size: usize,
}

pub struct X {
    pub c: RustConnection,
    pub root: Window,
    pub win: Window,
    pub depth: u8,
    gc: Gcontext,
    pub atoms: Atoms,
    shm: Option<Shm>,
    shm_ok: bool,
    min_keycode: u8,
    per: u8,
    keysyms: Vec<u32>,
}

fn atom(c: &RustConnection, name: &str) -> Res<Atom> {
    Ok(c.intern_atom(false, name.as_bytes())?.reply()?.atom)
}

impl X {
    pub fn open(w: u16, h: u16, title: &str) -> Res<Self> {
        let (c, screen) = x11rb::connect(None)?;
        let s = &c.setup().roots[screen];
        let root = s.root;
        let depth = s.root_depth;
        let visual = s.root_visual;
        let atoms = Atoms {
            wm_protocols: atom(&c, "WM_PROTOCOLS")?,
            wm_delete: atom(&c, "WM_DELETE_WINDOW")?,
            net_wm_name: atom(&c, "_NET_WM_NAME")?,
            net_wm_icon: atom(&c, "_NET_WM_ICON")?,
            net_wm_pid: atom(&c, "_NET_WM_PID")?,
            utf8: atom(&c, "UTF8_STRING")?,
            niruwm_theme: atom(&c, "_NIRUWM_THEME")?,
            clipboard: atom(&c, "CLIPBOARD")?,
            targets: atom(&c, "TARGETS")?,
            sel_prop: atom(&c, "_NIRUFLOW_SELECTION")?,
        };
        let win = c.generate_id()?;
        c.create_window(
            depth,
            win,
            root,
            0,
            0,
            w,
            h,
            0,
            WindowClass::INPUT_OUTPUT,
            visual,
            &CreateWindowAux::new()
                .background_pixel(s.black_pixel)
                .bit_gravity(Gravity::NORTH_WEST)
                .event_mask(
                    EventMask::EXPOSURE
                        | EventMask::KEY_PRESS
                        | EventMask::KEY_RELEASE
                        | EventMask::BUTTON_PRESS
                        | EventMask::BUTTON_RELEASE
                        | EventMask::POINTER_MOTION
                        | EventMask::LEAVE_WINDOW
                        | EventMask::STRUCTURE_NOTIFY
                        | EventMask::FOCUS_CHANGE,
                ),
        )?;
        // The theme property on the root window changes when the user picks
        // another NIRU theme.
        c.change_window_attributes(
            root,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?;
        c.change_property8(
            PropMode::REPLACE,
            win,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"niruflow\0NIRUFLOW\0",
        )?;
        c.change_property32(
            PropMode::REPLACE,
            win,
            atoms.wm_protocols,
            AtomEnum::ATOM,
            &[atoms.wm_delete],
        )?;
        c.change_property32(
            PropMode::REPLACE,
            win,
            atoms.net_wm_pid,
            AtomEnum::CARDINAL,
            &[std::process::id()],
        )?;
        let gc = c.generate_id()?;
        c.create_gc(gc, win, &CreateGCAux::new().graphics_exposures(0))?;
        let shm_ok = c
            .extension_information(x11rb::protocol::shm::X11_EXTENSION_NAME)?
            .is_some()
            && c.shm_query_version()
                .ok()
                .and_then(|r| r.reply().ok())
                .is_some()
            && std::env::var_os("NIRUFLOW_NO_SHM").is_none();
        let setup = c.setup();
        let (min_keycode, max_keycode) = (setup.min_keycode, setup.max_keycode);
        let map = c
            .get_keyboard_mapping(min_keycode, max_keycode - min_keycode + 1)?
            .reply()?;
        let x = X {
            c,
            root,
            win,
            depth,
            gc,
            atoms,
            shm: None,
            shm_ok,
            min_keycode,
            per: map.keysyms_per_keycode,
            keysyms: map.keysyms,
        };
        x.set_title(title)?;
        Ok(x)
    }

    pub fn set_title(&self, title: &str) -> Res<()> {
        self.c.change_property8(
            PropMode::REPLACE,
            self.win,
            self.atoms.net_wm_name,
            self.atoms.utf8,
            title.as_bytes(),
        )?;
        self.c.change_property8(
            PropMode::REPLACE,
            self.win,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            title.as_bytes(),
        )?;
        Ok(())
    }

    /// `_NET_WM_ICON`: width, height, then ARGB pixels, for every size.
    pub fn set_icon(&self, sizes: &[(u32, u32, Vec<u32>)]) -> Res<()> {
        let mut data = Vec::new();
        for (w, h, px) in sizes {
            data.push(*w);
            data.push(*h);
            data.extend_from_slice(px);
        }
        self.c.change_property32(
            PropMode::REPLACE,
            self.win,
            self.atoms.net_wm_icon,
            AtomEnum::CARDINAL,
            &data,
        )?;
        Ok(())
    }

    pub fn map(&self) -> Res<()> {
        self.c.map_window(self.win)?;
        self.c.flush()?;
        Ok(())
    }

    pub fn root_theme(&self) -> Option<String> {
        let r = self
            .c
            .get_property(
                false,
                self.root,
                self.atoms.niruwm_theme,
                AtomEnum::ANY,
                0,
                64,
            )
            .ok()?
            .reply()
            .ok()?;
        let s = String::from_utf8(r.value).ok()?;
        (!s.is_empty()).then_some(s)
    }

    /// The keysym of a key, with Shift (column 1) or AltGr (columns 4/5).
    pub fn keysym(&self, keycode: u8, state: KeyButMask) -> u32 {
        let idx = (keycode.saturating_sub(self.min_keycode)) as usize * self.per as usize;
        let shift = state.contains(KeyButMask::SHIFT) ^ state.contains(KeyButMask::LOCK);
        let altgr = state.contains(KeyButMask::MOD5);
        let col = match (altgr, shift) {
            (true, false) => 4,
            (true, true) => 5,
            (false, true) => 1,
            (false, false) => 0,
        };
        let get = |c: usize| self.keysyms.get(idx + c).copied().unwrap_or(0);
        let k = get(col);
        if k != 0 {
            return k;
        }
        let base = get(if altgr { 4 } else { 0 });
        if base != 0 {
            // A letter without its own shifted symbol: upper case.
            if shift && (0x61..=0x7a).contains(&base) {
                return base - 0x20;
            }
            return base;
        }
        get(0)
    }

    fn ensure_shm(&mut self, bytes: usize) -> bool {
        if !self.shm_ok {
            return false;
        }
        if self.shm.as_ref().is_some_and(|s| s.size >= bytes) {
            return true;
        }
        self.drop_shm();
        let size = bytes.next_multiple_of(1 << 20);
        unsafe {
            let id = libc::shmget(libc::IPC_PRIVATE, size, libc::IPC_CREAT | 0o600);
            if id < 0 {
                self.shm_ok = false;
                return false;
            }
            let addr = libc::shmat(id, std::ptr::null(), 0);
            if addr as isize == -1 {
                libc::shmctl(id, libc::IPC_RMID, std::ptr::null_mut());
                self.shm_ok = false;
                return false;
            }
            let Ok(seg) = self.c.generate_id() else {
                return false;
            };
            let attached = self
                .c
                .shm_attach(seg, id as u32, false)
                .ok()
                .and_then(|ck| ck.check().ok())
                .is_some();
            // Removed now: the segment lives until both sides detach, so a
            // crash cannot leak it.
            libc::shmctl(id, libc::IPC_RMID, std::ptr::null_mut());
            if !attached {
                libc::shmdt(addr);
                self.shm_ok = false;
                return false;
            }
            self.shm = Some(Shm {
                seg,
                addr: addr as *mut u8,
                size,
            });
        }
        true
    }

    fn drop_shm(&mut self) {
        if let Some(s) = self.shm.take() {
            let _ = self.c.shm_detach(s.seg);
            unsafe { libc::shmdt(s.addr as *const libc::c_void) };
        }
    }

    /// Copy the canvas to the window and wait until the server has read it.
    pub fn present(&mut self, cv: &Canvas) -> Res<()> {
        let bytes = cv.as_bytes();
        let (w, h) = (cv.w as u16, cv.h as u16);
        if self.ensure_shm(bytes.len()) {
            let s = self.shm.as_ref().unwrap();
            unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), s.addr, bytes.len()) };
            self.c.shm_put_image(
                self.win,
                self.gc,
                w,
                h,
                0,
                0,
                w,
                h,
                0,
                0,
                self.depth,
                ImageFormat::Z_PIXMAP.into(),
                false,
                s.seg,
                0,
            )?;
        } else {
            let max = self.c.maximum_request_bytes().saturating_sub(64).max(4096);
            let row_bytes = cv.w as usize * 4;
            let rows = (max / row_bytes).max(1);
            let mut y = 0usize;
            while y < cv.h as usize {
                let n = rows.min(cv.h as usize - y);
                self.c.put_image(
                    ImageFormat::Z_PIXMAP,
                    self.win,
                    self.gc,
                    w,
                    n as u16,
                    0,
                    y as i16,
                    0,
                    self.depth,
                    &bytes[y * row_bytes..(y + n) * row_bytes],
                )?;
                y += n;
            }
        }
        // A round trip: the next frame may overwrite the shared buffer.
        self.c.get_input_focus()?.reply()?;
        Ok(())
    }
}

impl X {
    /// Take the CLIPBOARD so other programs can paste our files.
    pub fn own_clipboard(&self) -> Res<()> {
        self.c
            .set_selection_owner(self.win, self.atoms.clipboard, x11rb::CURRENT_TIME)?;
        Ok(())
    }

    /// Ask the clipboard owner for files; the answer is a SelectionNotify.
    pub fn request_clipboard(&self, target: Atom) -> Res<()> {
        self.c.convert_selection(
            self.win,
            self.atoms.clipboard,
            target,
            self.atoms.sel_prop,
            x11rb::CURRENT_TIME,
        )?;
        self.c.flush()?;
        Ok(())
    }

    pub fn take_selection(&self) -> Option<Vec<u8>> {
        let r = self
            .c
            .get_property(
                true,
                self.win,
                self.atoms.sel_prop,
                AtomEnum::ANY,
                0,
                1 << 24,
            )
            .ok()?
            .reply()
            .ok()?;
        (!r.value.is_empty()).then_some(r.value)
    }

    /// Answer another program's paste request with `text`.
    pub fn answer_text(&self, e: &SelectionRequestEvent, text: &str) -> Res<()> {
        let a = &self.atoms;
        let mut prop = if e.property == x11rb::NONE {
            e.target
        } else {
            e.property
        };
        if e.target == a.targets {
            let t = [a.targets, a.utf8, u32::from(AtomEnum::STRING)];
            self.c
                .change_property32(PropMode::REPLACE, e.requestor, prop, AtomEnum::ATOM, &t)?;
        } else if e.target == a.utf8 || e.target == u32::from(AtomEnum::STRING) {
            self.c.change_property8(
                PropMode::REPLACE,
                e.requestor,
                prop,
                e.target,
                text.as_bytes(),
            )?;
        } else {
            prop = x11rb::NONE;
        }
        let ev = SelectionNotifyEvent {
            response_type: SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: e.time,
            requestor: e.requestor,
            selection: e.selection,
            target: e.target,
            property: prop,
        };
        self.c
            .send_event(false, e.requestor, EventMask::NO_EVENT, ev)?;
        self.c.flush()?;
        Ok(())
    }
}

impl Drop for X {
    fn drop(&mut self) {
        self.drop_shm();
        let _ = self.c.flush();
    }
}
