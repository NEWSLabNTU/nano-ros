//! `cargo --timings` collector — covers cargo (native, esp32 bare-metal, cross).
//!
//! `cargo build --timings` writes `target*/cargo-timings/cargo-timing-*.html`
//! (plus a `cargo-timing.html` pointer to the latest). The HTML embeds the data
//! as a `const UNIT_DATA = [ … ];` JSON array of per-unit records
//! (`{name, mode, start, duration, …}`, times in seconds). We scrape that array.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{
    collect::Collected,
    model::{Kind, RawUnit},
};

/// One record from the `UNIT_DATA` array (tolerant — extra fields ignored,
/// missing ones defaulted).
#[derive(Debug, Deserialize)]
struct UnitData {
    #[serde(default)]
    name: String,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    start: f64,
    #[serde(default)]
    duration: f64,
}

/// Discover and parse the newest cargo-timings HTML under `dir`.
pub fn collect(dir: &Path) -> Collected {
    let Some(html) = find_timings_html(dir) else {
        return Collected::default();
    };
    match std::fs::read_to_string(&html) {
        Ok(text) => parse(&text),
        Err(e) => Collected {
            notes: vec![format!("cargo: could not read {}: {e}", html.display())],
            ..Default::default()
        },
    }
}

/// Locate a cargo-timings HTML report under any `target*/cargo-timings/` dir.
/// Prefers the `cargo-timing.html` latest-pointer; else newest `cargo-timing-*.html`.
fn find_timings_html(dir: &Path) -> Option<PathBuf> {
    let mut newest: Option<PathBuf> = None;
    let mut pointer: Option<PathBuf> = None;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            let is_target = p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n == "target" || n.starts_with("target"));
            if !is_target {
                continue;
            }
            let tdir = p.join("cargo-timings");
            if !tdir.is_dir() {
                continue;
            }
            let ptr = tdir.join("cargo-timing.html");
            if ptr.is_file() {
                pointer = Some(ptr);
            }
            if let Ok(files) = std::fs::read_dir(&tdir) {
                for f in files.flatten() {
                    let fp = f.path();
                    if fp
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("cargo-timing-") && n.ends_with(".html"))
                    {
                        let take = match &newest {
                            None => true,
                            Some(cur) => mtime(&fp) > mtime(cur),
                        };
                        if take {
                            newest = Some(fp);
                        }
                    }
                }
            }
        }
    }
    pointer.or(newest)
}

fn mtime(p: &Path) -> std::time::SystemTime {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .unwrap_or(std::time::UNIX_EPOCH)
}

/// Parse a cargo-timings HTML string by extracting the `UNIT_DATA` JSON array.
pub fn parse(html: &str) -> Collected {
    let Some(json) = extract_unit_data(html) else {
        return Collected {
            notes: vec!["cargo: UNIT_DATA array not found in timings HTML".to_string()],
            ..Default::default()
        };
    };
    let data: Vec<UnitData> = match serde_json::from_str(&json) {
        Ok(d) => d,
        Err(e) => {
            return Collected {
                notes: vec![format!("cargo: UNIT_DATA parse error: {e}")],
                ..Default::default()
            };
        }
    };

    let units = data
        .into_iter()
        .map(|u| {
            // `run-custom-build` is a build script execution → codegen stage.
            let kind = if u.mode == "run-custom-build" {
                Kind::Codegen
            } else {
                Kind::Compile
            };
            RawUnit {
                is_native: u.name.ends_with("-sys"),
                name: u.name,
                kind,
                dur_s: u.duration,
                start_s: u.start,
            }
        })
        .collect::<Vec<_>>();

    Collected {
        deep: !units.is_empty(),
        units,
        backend: Some(crate::model::Backend::Cargo),
        notes: Vec::new(),
    }
}

/// Pull the `[ … ]` literal following `UNIT_DATA =` via bracket-depth scan
/// (string-aware so brackets inside JSON strings don't confuse the depth).
fn extract_unit_data(html: &str) -> Option<String> {
    let anchor = html.find("UNIT_DATA")?;
    let rest = &html[anchor..];
    let eq = rest.find('=')?;
    let after = &rest[eq + 1..];
    let start = after.find('[')?;
    let bytes = after.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escaped = false;
    for i in start..bytes.len() {
        let b = bytes[i];
        if in_str {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(after[start..=i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}
