//! Configuration-only backup as a small ZIP (manifest.json + config.json),
//! the same format NIRUFLOW 0.x reads and writes. No history, quarantine or
//! secrets. The ZIP reader/writer is minimal and strict on purpose.
use crate::config::{config_file, copy2, save, R};
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const MEMBER: &str = "config.json";
const MAX: usize = 2_000_000;

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, t) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *t = c;
    }
    let mut c = !0u32;
    for b in data {
        c = table[((c ^ *b as u32) & 0xff) as usize] ^ (c >> 8);
    }
    !c
}

fn dos_time() -> (u16, u16) {
    let c = crate::fmt::local_civil(crate::fmt::now_f64());
    let time = ((c.h as u16) << 11) | ((c.mi as u16) << 5) | ((c.s as u16) / 2);
    let date = (((c.y - 1980).max(0) as u16) << 9) | ((c.mo as u16) << 5) | c.d as u16;
    (time, date)
}

pub fn zip_write(members: &[(&str, &[u8])]) -> Vec<u8> {
    let (time, date) = dos_time();
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in members {
        let comp = miniz_oxide::deflate::compress_to_vec(data, 6);
        let crc = crc32(data);
        let offset = out.len() as u32;
        let le16 = |v: &mut Vec<u8>, x: u16| v.extend_from_slice(&x.to_le_bytes());
        let le32 = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&x.to_le_bytes());
        le32(&mut out, 0x0403_4b50);
        le16(&mut out, 20);
        le16(&mut out, 0);
        le16(&mut out, 8);
        le16(&mut out, time);
        le16(&mut out, date);
        le32(&mut out, crc);
        le32(&mut out, comp.len() as u32);
        le32(&mut out, data.len() as u32);
        le16(&mut out, name.len() as u16);
        le16(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&comp);
        le32(&mut central, 0x0201_4b50);
        le16(&mut central, (3 << 8) | 20);
        le16(&mut central, 20);
        le16(&mut central, 0);
        le16(&mut central, 8);
        le16(&mut central, time);
        le16(&mut central, date);
        le32(&mut central, crc);
        le32(&mut central, comp.len() as u32);
        le32(&mut central, data.len() as u32);
        le16(&mut central, name.len() as u16);
        le16(&mut central, 0);
        le16(&mut central, 0);
        le16(&mut central, 0);
        le16(&mut central, 0);
        le32(&mut central, 0o100600 << 16);
        le32(&mut central, offset);
        central.extend_from_slice(name.as_bytes());
    }
    let cd_offset = out.len() as u32;
    let cd_len = central.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&(members.len() as u16).to_le_bytes());
    out.extend_from_slice(&(members.len() as u16).to_le_bytes());
    out.extend_from_slice(&cd_len.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    out
}

struct Member {
    name: String,
    method: u16,
    crc: u32,
    comp: usize,
    size: usize,
    offset: usize,
}

fn u16_at(b: &[u8], i: usize) -> R<u16> {
    b.get(i..i + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| "File is not a zip file".to_string())
}

fn u32_at(b: &[u8], i: usize) -> R<u32> {
    b.get(i..i + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| "File is not a zip file".to_string())
}

fn members(b: &[u8]) -> R<Vec<Member>> {
    let bad = || "File is not a zip file".to_string();
    if b.len() < 22 {
        return Err(bad());
    }
    let lo = b.len().saturating_sub(22 + 65535);
    let eocd = (lo..=b.len() - 22)
        .rev()
        .find(|&i| b[i..i + 4] == [0x50, 0x4b, 0x05, 0x06])
        .ok_or_else(bad)?;
    let count = u16_at(b, eocd + 10)? as usize;
    let mut p = u32_at(b, eocd + 16)? as usize;
    let mut out = Vec::new();
    for _ in 0..count {
        if u32_at(b, p)? != 0x0201_4b50 {
            return Err("Bad magic number for central directory".into());
        }
        let n = u16_at(b, p + 28)? as usize;
        let extra = u16_at(b, p + 30)? as usize;
        let comment = u16_at(b, p + 32)? as usize;
        let name = b.get(p + 46..p + 46 + n).ok_or_else(bad)?;
        out.push(Member {
            name: String::from_utf8_lossy(name).into_owned(),
            method: u16_at(b, p + 10)?,
            crc: u32_at(b, p + 16)?,
            comp: u32_at(b, p + 20)? as usize,
            size: u32_at(b, p + 24)? as usize,
            offset: u32_at(b, p + 42)? as usize,
        });
        p += 46 + n + extra + comment;
    }
    Ok(out)
}

fn read_member(b: &[u8], m: &Member) -> R<Vec<u8>> {
    let o = m.offset;
    if u32_at(b, o)? != 0x0403_4b50 {
        return Err("Bad magic number for file header".into());
    }
    let start = o + 30 + u16_at(b, o + 26)? as usize + u16_at(b, o + 28)? as usize;
    let raw = b.get(start..start + m.comp).ok_or("Truncated zip member")?;
    let data = match m.method {
        0 => raw.to_vec(),
        8 => miniz_oxide::inflate::decompress_to_vec_with_limit(raw, MAX + 1)
            .map_err(|_| "Error -3 while decompressing data".to_string())?,
        _ => return Err("compression type not supported".into()),
    };
    if data.len() != m.size || crc32(&data) != m.crc {
        return Err(format!("Bad CRC-32 for file '{}'", m.name));
    }
    Ok(data)
}

pub fn export_config(destination: &str) -> R<PathBuf> {
    let dest = PathBuf::from(crate::config::expand_user(destination));
    if crate::safety::suffix(&dest.file_name().unwrap_or_default().to_string_lossy()).to_lowercase()
        != ".zip"
    {
        return Err("Backup must be a .zip file".into());
    }
    if let Some(parent) = dest.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if dest.exists() {
        return Err(dest.display().to_string());
    }
    let cfg = crate::config::load()?;
    let payload = crate::json::pretty(&cfg, false);
    if payload.len() > MAX {
        return Err("Configuration is unexpectedly large".into());
    }
    let manifest = crate::json::compact(&json!({"application": "NIRUFLOW", "format": 1,
        "exported_at": crate::fmt::iso_utc(crate::fmt::now_f64())}));
    let zip = zip_write(&[
        ("manifest.json", manifest.as_bytes()),
        (MEMBER, payload.as_bytes()),
    ]);
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&dest)
        .map_err(|_| dest.display().to_string())?;
    f.write_all(&zip).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())?;
    Ok(dest)
}

pub fn read_backup(source: &Path) -> R<Value> {
    let b = std::fs::read(source).map_err(|e| e.to_string())?;
    let ms = members(&b)?;
    let names: std::collections::BTreeSet<&str> = ms.iter().map(|m| m.name.as_str()).collect();
    if names != ["manifest.json", MEMBER].into_iter().collect() {
        return Err("Invalid backup contents".into());
    }
    if ms.iter().any(|m| m.size > MAX) {
        return Err("Backup member too large".into());
    }
    let get = |name: &str| -> R<Value> {
        let m = ms
            .iter()
            .rev()
            .find(|m| m.name == name)
            .ok_or("Invalid backup contents")?;
        serde_json::from_slice(&read_member(&b, m)?).map_err(|e| e.to_string())
    };
    let manifest = get("manifest.json")?;
    if manifest.get("application").and_then(Value::as_str) != Some("NIRUFLOW")
        || manifest.get("format").and_then(Value::as_i64) != Some(1)
    {
        return Err("Unsupported backup format".into());
    }
    let cfg = get(MEMBER)?;
    if !cfg.is_object()
        || !cfg.get("rules").is_some_and(Value::is_array)
        || !cfg.get("profiles").is_some_and(Value::is_object)
    {
        return Err("Invalid configuration structure".into());
    }
    Ok(cfg)
}

pub fn import_config(source: &str) -> R<Value> {
    let cfg = read_backup(Path::new(&crate::config::expand_user(source)))?;
    let file = config_file();
    if file.exists() {
        let c = crate::fmt::local_civil(crate::fmt::now_f64());
        let stamp = format!(
            "{:04}{:02}{:02}-{:02}{:02}{:02}-{:06}",
            c.y, c.mo, c.d, c.h, c.mi, c.s, c.us
        );
        let snap = file.with_file_name(format!("config.before-import-{stamp}.json"));
        copy2(&file, &snap).map_err(|e| e.to_string())?;
        let _ = std::fs::set_permissions(&snap, std::fs::Permissions::from_mode(0o600));
    }
    save(&cfg)?;
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_round_trip_and_bad_contents() {
        let d = std::env::temp_dir().join(format!("niruflow-zip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let cfg = json!({"rules": [], "profiles": {"default": {"locations": []}}, "active_profile": "default"});
        let manifest = crate::json::compact(&json!({"application": "NIRUFLOW", "format": 1}));
        let payload = crate::json::pretty(&cfg, false);
        let p = d.join("b.zip");
        std::fs::write(
            &p,
            zip_write(&[
                ("manifest.json", manifest.as_bytes()),
                ("config.json", payload.as_bytes()),
            ]),
        )
        .unwrap();
        assert_eq!(read_backup(&p).unwrap(), cfg);
        let q = d.join("evil.zip");
        std::fs::write(&q, zip_write(&[("evil.txt", b"bad")])).unwrap();
        assert!(read_backup(&q).is_err());
        std::fs::write(d.join("x.zip"), b"not a zip").unwrap();
        assert!(read_backup(&d.join("x.zip")).is_err());
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn crc_known_value() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }
}

#[cfg(test)]
mod python_compat {
    #[test]
    fn reads_a_backup_written_by_niruflow_0_6() {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/python-0.6-backup.zip");
        let cfg = super::read_backup(&p).unwrap();
        assert_eq!(cfg["theme"], "Satie");
        assert_eq!(cfg["note"], "åäö");
        assert_eq!(
            crate::json::pretty(&cfg["rules"][0]["hours"], false),
            "24.0"
        );
    }
}
