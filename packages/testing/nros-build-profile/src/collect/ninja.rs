//! `.ninja_log` collector — covers west (Zephyr), cmake (C/C++), idf.py (esp32).
//!
//! ninja writes `build*/.ninja_log` with no opt-in. Format (v5/v6): a header
//! line `# ninja log vN` then tab-separated rows
//! `start_ms  end_ms  mtime  output  command_hash`. Duration is `end - start`
//! in milliseconds; `start_ms` is relative to the build's first edge.

use std::{collections::BTreeMap, path::Path};

use crate::{
    collect::Collected,
    model::{Backend, Kind, RawUnit},
};

/// Discover and parse the newest `.ninja_log` under `dir` (searches `dir` and
/// any `build*/` subdirectory, one level deep).
pub fn collect(dir: &Path) -> Collected {
    let Some(log) = find_ninja_log(dir) else {
        return Collected::default();
    };
    match std::fs::read_to_string(&log) {
        Ok(text) => {
            let mut c = parse(&text);
            c.backend = Some(detect_driver(log.parent().unwrap_or(dir)));
            c
        }
        Err(e) => Collected {
            notes: vec![format!("ninja: could not read {}: {e}", log.display())],
            ..Default::default()
        },
    }
}

/// Locate a `.ninja_log`: prefer `dir/build*/.ninja_log`, fall back to
/// `dir/.ninja_log`. When several exist, pick the most recently modified.
fn find_ninja_log(dir: &Path) -> Option<std::path::PathBuf> {
    let mut candidates = Vec::new();
    let direct = dir.join(".ninja_log");
    if direct.is_file() {
        candidates.push(direct);
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n == "build" || n.starts_with("build"))
            {
                let log = p.join(".ninja_log");
                if log.is_file() {
                    candidates.push(log);
                }
            }
        }
    }
    candidates
        .into_iter()
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
}

/// Parse `.ninja_log` text. Last row per output wins (handles rebuild rows).
pub fn parse(text: &str) -> Collected {
    // output -> (start_ms, end_ms); BTreeMap for deterministic ordering.
    let mut rows: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut notes = Vec::new();
    let mut skipped = 0usize;

    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut f = line.split('\t');
        match (f.next(), f.next(), f.next(), f.next()) {
            (Some(start), Some(end), Some(_mtime), Some(output)) => {
                match (start.parse::<u64>(), end.parse::<u64>()) {
                    (Ok(s), Ok(e)) if e >= s => {
                        rows.insert(output.to_string(), (s, e));
                    }
                    _ => skipped += 1,
                }
            }
            _ => skipped += 1,
        }
    }
    if skipped > 0 {
        notes.push(format!("ninja: skipped {skipped} malformed row(s)"));
    }

    let units = rows
        .into_iter()
        .map(|(output, (s, e))| {
            let kind = classify(&output);
            RawUnit {
                name: basename(&output),
                kind,
                dur_s: (e - s) as f64 / 1000.0,
                start_s: s as f64 / 1000.0,
                is_native: matches!(kind, Kind::Compile | Kind::Link),
            }
        })
        .collect::<Vec<_>>();

    Collected {
        deep: !units.is_empty(),
        units,
        backend: None,
        notes,
    }
}

/// Classify a ninja output path into a stage by extension.
fn classify(output: &str) -> Kind {
    let ext = Path::new(output)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "o" | "obj" | "lo" => Kind::Compile,
        "a" | "lib" | "elf" | "so" | "dll" | "dylib" | "bin" | "hex" | "out" | "axf" => Kind::Link,
        _ => Kind::Other,
    }
}

fn basename(output: &str) -> String {
    Path::new(output)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(output)
        .to_string()
}

/// Heuristically identify the ninja driver from marker files in the build dir.
fn detect_driver(build_dir: &Path) -> Backend {
    // Zephyr (west) bakes a `zephyr/` subdir and a Kconfig `.config`.
    if build_dir.join("zephyr").is_dir() || build_dir.join("zephyr").join(".config").is_file() {
        return Backend::NinjaWest;
    }
    // ESP-IDF writes `project_description.json` + `config/sdkconfig.json`.
    if build_dir.join("project_description.json").is_file()
        || build_dir.join("config").join("sdkconfig.json").is_file()
    {
        return Backend::NinjaIdf;
    }
    if build_dir.join("CMakeCache.txt").is_file() {
        // Distinguish a Zephyr cache that lacks the zephyr/ dir.
        if let Ok(cache) = std::fs::read_to_string(build_dir.join("CMakeCache.txt"))
            && cache.contains("ZEPHYR_BASE")
        {
            return Backend::NinjaWest;
        }
        return Backend::NinjaCmake;
    }
    Backend::Ninja
}
