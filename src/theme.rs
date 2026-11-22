//! NIRU suite colours (shared contract, adapted from NIRUFM) and the three
//! fixed NIRUFLOW palettes of 0.x.
//!
//! The same contract as the other NIRU components: the theme named in
//! `~/.config/niruwm/config.toml`, a user theme file in
//! `~/.config/niruwm/themes/NAME.toml` before the built-in palette, and the
//! optional `[colors]` of `~/.config/niruwm/appearance.toml`. NIRUWM publishes
//! the theme in effect as `_NIRUWM_THEME` on the root window; a change of that
//! property reloads the palette live.
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub background: u32,
    pub surface: u32,
    pub foreground: u32,
    pub muted: u32,
    pub border: u32,
    pub accent: u32,
    pub selected: u32,
    pub warning: u32,
    pub error: u32,
}

impl Default for Palette {
    fn default() -> Self {
        builtin("niru-noir").unwrap()
    }
}

pub fn blend(a: u32, b: u32, amount: f32) -> u32 {
    let mix = |x: u32, y: u32| {
        let x = (x & 0xff) as f32;
        let y = (y & 0xff) as f32;
        (x + (y - x) * amount).round().clamp(0.0, 255.0) as u32
    };
    (mix(a >> 16, b >> 16) << 16) | (mix(a >> 8, b >> 8) << 8) | mix(a, b)
}

fn builtin(name: &str) -> Option<Palette> {
    let (background, foreground, muted, border, accent) = match name {
        "niru-noir" => (0x0f0f10, 0xe5e5e5, 0xa8a8a8, 0x2a2a2d, 0x5a5a60),
        "niru-nord" => (0x2e3440, 0xeceff4, 0x9aa5b1, 0x3b4252, 0x88c0d0),
        "niru-satie" => (0x141414, 0xdedad4, 0x8f8b86, 0x2a2724, 0xb08d7d),
        "niru-carl-larsson" => (0xf3ece0, 0x3b3228, 0x7d6f5f, 0xd8cdbb, 0xa63d2f),
        "niru-forest-vigil" => (0x0b100c, 0xd8ded2, 0x96a292, 0x2b362c, 0x8aa87e),
        "niru-blood-of-urdr" => (0x120808, 0xe8d6cf, 0xb09a92, 0x3a1d1b, 0xc96050),
        "niru-root-and-stone" => (0x17100b, 0xeadfd2, 0xb5a08c, 0x3a2a1e, 0xc19768),
        "niru-blue-horizon" => (0x0b1821, 0xdce6eb, 0x93a6b2, 0x243846, 0x86aec8),
        "niru-the-norns" => (0x111110, 0xece9e4, 0xa6a19b, 0x3b3936, 0xc9c2b9),
        "niru-winter-nights" => (0x1f232b, 0xead9c3, 0xb9aa9a, 0x4e5157, 0xd8c4aa),
        _ => return None,
    };
    Some(Palette {
        background,
        surface: blend(background, foreground, 0.05),
        foreground,
        muted,
        border,
        accent,
        selected: blend(background, accent, 0.28),
        warning: 0xd7b36a,
        error: 0xd77a7a,
    })
}

pub fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(".config"))
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn parse_hex(s: &str) -> Option<u32> {
    let s = s.trim().trim_start_matches('#');
    (s.len() == 6 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| u32::from_str_radix(s, 16).ok())
        .flatten()
}

fn read_toml(path: &Path) -> Option<toml::Value> {
    std::fs::read_to_string(path)
        .ok()?
        .parse::<toml::Value>()
        .ok()
}

/// The theme name from the WM (`_NIRUWM_THEME`) or the configuration file.
pub fn configured_theme(base: &Path) -> String {
    read_toml(&base.join("niruwm/config.toml"))
        .and_then(|v| v.get("theme")?.as_str().map(str::to_string))
        .unwrap_or_else(|| "niru-noir".to_string())
}

/// The font size of the suite (`font_size` in the NIRUWM configuration).
pub fn configured_font_size(base: &Path) -> Option<f32> {
    let v = read_toml(&base.join("niruwm/config.toml"))?;
    let size = v.get("font_size")?;
    size.as_float()
        .or_else(|| size.as_integer().map(|i| i as f32 as f64))
        .map(|f| f as f32)
        .filter(|f| (8.0..=40.0).contains(f))
}

pub fn configured_font_path(base: &Path) -> Option<String> {
    let v = read_toml(&base.join("niruwm/config.toml"))?;
    let p = v.get("font_path")?.as_str()?;
    let p = p
        .strip_prefix("~/")
        .map_or_else(|| PathBuf::from(p), |rest| home().join(rest));
    p.is_file().then(|| p.to_string_lossy().into_owned())
}

pub fn load(base: &Path, name: &str) -> Palette {
    let mut p = builtin(name).unwrap_or_default();
    let mut apply = |table: &toml::Value| {
        let get = |key: &str, old: u32| {
            table
                .get(key)
                .and_then(|v| v.as_str())
                .and_then(parse_hex)
                .unwrap_or(old)
        };
        p.background = get("background", p.background);
        p.surface = get("surface", p.surface);
        p.foreground = get("foreground", p.foreground);
        p.muted = get("muted", p.muted);
        p.border = get("border", p.border);
        p.accent = get("accent", get("focus", p.accent));
        p.selected = get("selected", p.selected);
        p.warning = get("warning", p.warning);
        p.error = get("error", p.error);
    };
    let theme_file = base.join("niruwm/themes").join(format!("{name}.toml"));
    if let Some(theme) = read_toml(&theme_file) {
        apply(theme.get("colors").unwrap_or(&theme));
    } else if let Some(appearance) = read_toml(&base.join("niruwm/appearance.toml")) {
        if appearance.get("schema").and_then(|v| v.as_integer()) == Some(1) {
            if let Some(colors) = appearance.get("colors") {
                apply(colors);
            }
        }
    }
    p
}

/// The name in `config.json` that follows the desktop theme live.
pub const FOLLOW: &str = "NIRUX desktop";
pub const NAMES: [&str; 4] = [FOLLOW, "Niru Noir", "Satie", "C. Larsson"];

/// What the window paints with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Colors {
    pub bg: u32,
    pub panel: u32,
    pub fg: u32,
    pub muted: u32,
    pub border: u32,
    pub hover: u32,
    pub accent: u32,
    pub selected: u32,
    pub warning: u32,
    pub error: u32,
}

/// The fixed palettes of 0.x: background, panel, text, muted, border, hover.
fn fixed(name: &str) -> Option<(u32, u32, u32, u32, u32, u32)> {
    Some(match name {
        "Niru Noir" => (0x131415, 0x202123, 0xe5e5e5, 0x9a9da0, 0x35373a, 0x2a2c2f),
        "Satie" => (0x191919, 0x252422, 0xeee9df, 0xb5afa6, 0x44423f, 0x33312e),
        "C. Larsson" => (0x20211e, 0x2b2d29, 0xece9dd, 0xb9bbaf, 0x494c45, 0x373a34),
        _ => return None,
    })
}

/// The colours for a `theme` setting; `desktop` is the NIRUWM theme name
/// in effect (root property or configuration).
pub fn colors(theme: &str, base: &Path, desktop: &str) -> Colors {
    if let Some((bg, panel, fg, muted, border, hover)) = fixed(theme) {
        return Colors {
            bg,
            panel,
            fg,
            muted,
            border,
            hover,
            accent: blend(border, fg, 0.35),
            selected: border,
            warning: 0xd7b36a,
            error: 0xd77a7a,
        };
    }
    let p = load(base, desktop);
    Colors {
        bg: p.background,
        panel: blend(p.background, p.foreground, 0.07),
        fg: p.foreground,
        muted: p.muted,
        border: p.border,
        hover: blend(p.background, p.border, 0.65),
        accent: p.accent,
        selected: blend(p.background, p.accent, 0.45),
        warning: p.warning,
        error: p.error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_theme_loads_and_carl_larsson_is_light() {
        for name in [
            "niru-noir",
            "niru-nord",
            "niru-forest-vigil",
            "niru-winter-nights",
        ] {
            assert!(builtin(name).is_some(), "{name}");
        }
        assert!(builtin("niru-carl-larsson").is_some());
    }

    #[test]
    fn a_user_theme_file_wins_over_the_builtin_palette() {
        let dir = std::env::temp_dir().join(format!("niruflow-theme-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("niruwm/themes")).unwrap();
        std::fs::write(
            dir.join("niruwm/themes/mine.toml"),
            "[colors]\nbackground = \"101010\"\naccent = \"#ff8800\"\n",
        )
        .unwrap();
        let p = load(&dir, "mine");
        assert_eq!(p.background, 0x101010);
        assert_eq!(p.accent, 0xff8800);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn follows_the_desktop_theme_and_top_level_keys() {
        let dir = std::env::temp_dir().join(format!("niruflow-theme2-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("niruwm/themes")).unwrap();
        std::fs::write(dir.join("niruwm/config.toml"), "theme = \"niru-nord\"\n").unwrap();
        assert_eq!(configured_theme(&dir), "niru-nord");
        let c = colors(FOLLOW, &dir, "niru-nord");
        assert_eq!((c.bg, c.fg), (0x2e3440, 0xeceff4));
        std::fs::write(
            dir.join("niruwm/themes/custom.toml"),
            "background = \"abcdef\"\nforeground = \"123456\"\n",
        )
        .unwrap();
        let c = colors(FOLLOW, &dir, "custom");
        assert_eq!((c.bg, c.fg), (0xabcdef, 0x123456));
        assert_eq!(colors("Satie", &dir, "custom").bg, 0x191919);
        assert!(NAMES.contains(&FOLLOW));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
