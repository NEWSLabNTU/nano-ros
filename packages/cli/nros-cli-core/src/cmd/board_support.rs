//! The board support registry as the CLI reads it — RFC-0114 D10/D12,
//! phase-522 W5.
//!
//! `scripts/gen-board-support.py` writes every row's derived facts into
//! `packages/boards/board-support-status.json` (status word, breadth, where it
//! runs, cadence, last run). The CLI reads that file, from the SDK root the
//! read-only `nros board` verbs already resolve ([`super::board::catalog_root`],
//! RFC-0099 D3), so a released toolchain answers with the copy it ships and an
//! installed user sees the words the book prints.
//!
//! The file is written on no clock (a committed file must not change on a day
//! nobody edited anything), so the one time-dependent rule of D10 is applied
//! here, with the real date: a manual row past its `stale_from` reads
//! "Builds (last booted <date>)".

use eyre::{Result, WrapErr};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Where the generated status file sits under an SDK root.
pub const STATUS_FILE: &str = "packages/boards/board-support-status.json";

/// One registry row, as the generator derived it.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Row {
    /// `crate@platform`, the registry's key.
    pub key: String,
    #[serde(rename = "crate")]
    pub krate: String,
    pub matrix_platform: Option<String>,
    /// The status word on no clock.
    pub status: String,
    pub built: bool,
    pub infra: bool,
    pub runs: Option<String>,
    pub emulator: Option<String>,
    pub hardware: Option<String>,
    pub cadence: Option<String>,
    pub last_run: Option<String>,
    /// First ISO day a manual row reads "Builds".
    pub stale_from: Option<String>,
    pub breadth: String,
    pub external: Option<String>,
    /// Descriptor board keys this row's evidence is for, where the package
    /// directory alone cannot say (see the registry header).
    #[serde(default)]
    pub board_keys: Vec<String>,
    pub book_run: Option<String>,
    pub book_flash: Option<String>,
    /// The evidence page, relative to the SDK root.
    pub page: Option<String>,
}

impl Row {
    /// The status word on `today` (an ISO date). ISO dates order as strings.
    pub fn status_on(&self, today: &str) -> String {
        match (&self.stale_from, &self.last_run) {
            (Some(from), Some(last)) if today >= from.as_str() => {
                format!("Builds (last booted {last})")
            }
            _ => self.status.clone(),
        }
    }

    /// Where the image runs, in one phrase.
    pub fn runs_on(&self) -> String {
        let mut out = Vec::new();
        if self.runs.as_deref() == Some("host") {
            out.push("host executable".to_string());
        }
        if let Some(e) = &self.emulator {
            out.push(if self.runs.as_deref() == Some("emulator") {
                e.clone()
            } else {
                format!("{e} (twin)")
            });
        }
        if let Some(h) = &self.hardware {
            out.push(if self.runs.as_deref() == Some("hardware") {
                h.clone()
            } else {
                format!("{h} (target)")
            });
        }
        if out.is_empty() {
            "-".into()
        } else {
            out.join(", ")
        }
    }

    /// The cadence column: the cadence, with the date of a manual run.
    pub fn cadence_text(&self) -> String {
        match (&self.cadence, &self.last_run) {
            (Some(c), Some(d)) => format!("{c} ({d})"),
            (Some(c), None) => c.clone(),
            _ => "-".into(),
        }
    }

    /// Does `name` pick this row? Its key, its crate, the crate without the
    /// `nros-board-` prefix, or its evidence page's stem.
    pub fn answers_to(&self, name: &str) -> bool {
        self.key == name
            || self.krate == name
            || self.krate.strip_prefix("nros-board-") == Some(name)
            || self
                .page
                .as_deref()
                .and_then(|p| Path::new(p).file_stem())
                .and_then(|s| s.to_str())
                == Some(name)
    }
}

/// The whole status file.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Status {
    pub stale_days: u32,
    pub rows: Vec<Row>,
    /// Where it was read from (not in the file).
    #[serde(skip)]
    pub path: PathBuf,
}

impl Status {
    /// Read the status file under `root`.
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(STATUS_FILE);
        let raw = std::fs::read_to_string(&path).wrap_err_with(|| {
            format!(
                "read the board support status {} (generated from \
                 packages/boards/board-support.toml by scripts/gen-board-support.py)",
                path.display()
            )
        })?;
        let mut s: Status =
            serde_json::from_str(&raw).wrap_err_with(|| format!("parse {}", path.display()))?;
        s.path = path;
        Ok(s)
    }

    /// The rows a package directory carries (one crate may serve several
    /// witnesses, `nros-board-zephyr` is native_sim and the FVP).
    pub fn rows_for_crate<'a>(&'a self, krate: &str) -> Vec<&'a Row> {
        self.rows.iter().filter(|r| r.krate == krate).collect()
    }

    /// The rows an image's board key reports: the rows that claim it in
    /// `board_keys`, else the rows of `krate`, the package its descriptor
    /// resolved to.
    pub fn rows_for_board<'a>(&'a self, key: &str, krate: Option<&str>) -> Vec<&'a Row> {
        let claimed: Vec<&Row> = self
            .rows
            .iter()
            .filter(|r| r.board_keys.iter().any(|k| k == key))
            .collect();
        if !claimed.is_empty() {
            return claimed;
        }
        match krate {
            Some(k) => self.rows_for_crate(k),
            None => self.rows_named(key),
        }
    }

    /// The rows `name` picks (see [`Row::answers_to`]).
    pub fn rows_named<'a>(&'a self, name: &str) -> Vec<&'a Row> {
        self.rows.iter().filter(|r| r.answers_to(name)).collect()
    }
}

/// RFC-0114 / phase-522 W5 decision step — when the checkout's registry and
/// the running toolchain's own copy disagree, say so and name the one read;
/// never pick silently. `read` is the root the verb answered from; `other`
/// the SDK root this binary would answer from on its own. Returns the lines
/// to print (empty when they agree or only one exists), and the other copy
/// when it differs.
pub fn other_copy(read: &Path, other: Option<&Path>) -> (Vec<String>, Option<Status>) {
    let Some(other) = other else {
        return (Vec::new(), None);
    };
    let (a, b) = (read.join(STATUS_FILE), other.join(STATUS_FILE));
    if same_file(&a, &b) {
        return (Vec::new(), None);
    }
    let (Ok(x), Ok(y)) = (std::fs::read(&a), std::fs::read(&b)) else {
        return (Vec::new(), None);
    };
    if x == y {
        return (Vec::new(), None);
    }
    let lines = vec![
        format!(
            "note: two copies of the board support registry disagree; read {}",
            a.display()
        ),
        format!(
            "      this nros's own SDK root carries {}; rows that differ show both",
            b.display()
        ),
    ];
    (lines, Status::load(other).ok())
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Today's date as `YYYY-MM-DD` (UTC), from the system clock.
pub fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    iso_from_days(secs.div_euclid(86_400))
}

/// Days since 1970-01-01 to an ISO date (Howard Hinnant's days-to-civil
/// algorithm, exact over the proleptic Gregorian calendar).
fn iso_from_days(z: i64) -> String {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
    }

    #[test]
    fn iso_dates_from_days() {
        assert_eq!(iso_from_days(0), "1970-01-01");
        assert_eq!(iso_from_days(20_393), "2025-11-01");
        assert_eq!(iso_from_days(11_016), "2000-02-29");
    }

    fn fvp() -> Row {
        Row {
            key: "nros-board-zephyr@Fvp".into(),
            krate: "nros-board-zephyr".into(),
            matrix_platform: Some("Fvp".into()),
            status: "Boots, manual (2026-09-06)".into(),
            built: true,
            infra: false,
            runs: Some("emulator".into()),
            emulator: Some("fvp:FVP_BaseR_AEMv8R".into()),
            hardware: None,
            cadence: Some("manual".into()),
            last_run: Some("2026-09-06".into()),
            stale_from: Some("2027-01-05".into()),
            breadth: String::new(),
            external: None,
            board_keys: vec!["fvp-aemv8r-smp".into()],
            book_run: None,
            book_flash: None,
            page: Some("book/src/reference/boards/fvp.md".into()),
        }
    }

    /// RFC-0114 evaluation step 5, both directions, on the live reader.
    #[test]
    fn a_manual_row_degrades_on_its_stale_day() {
        let r = fvp();
        assert_eq!(r.status_on("2027-01-04"), "Boots, manual (2026-09-06)");
        assert_eq!(r.status_on("2027-01-05"), "Builds (last booted 2026-09-06)");
        assert_eq!(r.cadence_text(), "manual (2026-09-06)");
        assert_eq!(r.runs_on(), "fvp:FVP_BaseR_AEMv8R");
    }

    #[test]
    fn a_board_key_reports_the_row_that_claims_it() {
        let root = repo();
        let status = Status::load(&root).expect("status file");
        let keys = |key: &str, krate: &str| -> Vec<String> {
            status
                .rows_for_board(key, Some(krate))
                .iter()
                .map(|r| r.key.clone())
                .collect()
        };
        assert_eq!(keys("linux", "linux"), ["nros-board-linux@Linux"]);
        assert_eq!(
            keys("zephyr", "zephyr"),
            ["nros-board-zephyr@ZephyrNativeSim"]
        );
        assert_eq!(
            keys("rv-virt-nuttx", "nros-board-nuttx-qemu"),
            ["nros-board-nuttx-qemu@NuttxRiscv"]
        );
        assert_eq!(
            keys("mps2-an385-freertos", "nros-board-mps2-an385-freertos"),
            ["nros-board-mps2-an385-freertos@FreertosMps2"]
        );
    }

    #[test]
    fn a_row_answers_to_its_key_crate_short_name_and_page() {
        let r = fvp();
        for name in [
            "nros-board-zephyr@Fvp",
            "nros-board-zephyr",
            "zephyr",
            "fvp",
        ] {
            assert!(r.answers_to(name), "{name}");
        }
        assert!(!r.answers_to("linux"));
    }

    /// The checkout's status file is the generator's, covers every registry
    /// row, and states a word from RFC-0114 D10's table for each — so a row
    /// the CLI prints is one the book prints.
    #[test]
    fn the_status_file_covers_every_registry_row() {
        let root = repo();
        let status = Status::load(&root).expect("status file");
        let reg: toml::Table =
            std::fs::read_to_string(root.join("packages/boards/board-support.toml"))
                .expect("registry")
                .parse()
                .expect("registry parses");
        let keys: Vec<String> = reg["board"]
            .as_array()
            .expect("[[board]]")
            .iter()
            .map(|b| {
                format!(
                    "{}@{}",
                    b["crate"].as_str().unwrap(),
                    b.get("matrix_platform")
                        .and_then(|p| p.as_str())
                        .unwrap_or("-")
                )
            })
            .collect();
        let have: Vec<&str> = status.rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(
            have, keys,
            "regenerate: python3 scripts/gen-board-support.py"
        );
        for r in &status.rows {
            let ok = [
                "Verified ",
                "Boots",
                "Builds",
                "Not supported",
                "Infrastructure",
            ]
            .iter()
            .any(|w| r.status.starts_with(w));
            assert!(ok, "{}: `{}` is not a D10 status word", r.key, r.status);
            assert!(!r.status.to_lowercase().contains("tier"), "{}", r.key);
        }
    }

    #[test]
    fn disagreeing_copies_are_named_never_picked_silently() {
        let d = tempfile::tempdir().unwrap();
        let (a, b) = (d.path().join("a"), d.path().join("b"));
        for (root, word) in [(&a, "Builds"), (&b, "Not supported")] {
            let f = root.join(STATUS_FILE);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(
                &f,
                format!(
                    r#"{{"stale_days":120,"rows":[{{"key":"x@-","crate":"x","matrix_platform":null,"status":"{word}","built":true,"infra":false,"runs":"none","emulator":null,"hardware":null,"cadence":null,"last_run":null,"stale_from":null,"breadth":"","external":null,"book_run":null,"book_flash":null,"page":null}}]}}"#
                ),
            )
            .unwrap();
        }
        let (lines, other) = other_copy(&a, Some(&b));
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].contains(&a.join(STATUS_FILE).display().to_string()));
        assert_eq!(other.unwrap().rows[0].status, "Not supported");
        let (lines, other) = other_copy(&a, Some(&a));
        assert!(
            lines.is_empty() && other.is_none(),
            "one copy is no disagreement"
        );
    }
}
