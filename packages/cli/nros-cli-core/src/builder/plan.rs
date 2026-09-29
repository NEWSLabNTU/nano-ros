//! Stage 2 — resolve WHICH image to build (phase-383 W2.c, RFC-0065 D1/D3).
//!
//! Stage 1 said what is in the workspace. This says what to do with it, and
//! the answer is one or more `(bringup, image, board)` triples.
//!
//! ## Plural bringups are normal
//!
//! phase-383 F7: `nano-ros-rt-eval` declares `demo_bringup` AND `load_bringup`,
//! each with its own `system.toml` and its own `[image.*]` set. So an image id
//! is only unique WITHIN a bringup, and `nros build <id>` has to say which one
//! it meant when two bringups both declare `native`. Guessing would build the
//! wrong system and report success.
//!
//! ## The driver is chosen by the board, not by the language mix
//!
//! RFC-0065 D3. A workspace with C++ packages and Rust packages is not a
//! "mixed" case needing its own driver: cargo can be consumed as a cmake target
//! via Corrosion and cmake cannot be consumed as a cargo target, so when the
//! graph crosses languages **cmake wins** (RFC-0024 §6.3). What actually
//! decides is the board — a Zephyr board means `west`, which needs no generated
//! root at all because a Zephyr application already is a complete cmake project.
//!
//! ## There are THREE roads, and choosing one can FAIL
//!
//! There was a fourth, `idf.py`, deleted by the RFC-0065 D3 amendment of
//! 2026-09-28. It was selected for `platform = "esp32"` with a graph that
//! crosses languages, and it could not work: phase-468 W2 retired the ESP-IDF
//! component, nothing emits an ESP-IDF project, no carrier reaches an `idf.py`
//! build, and `nros setup` cannot provision ESP-IDF. That combination now
//! REFUSES, naming issue 1525, rather than exec'ing a tool with no project to
//! build or falling through to a cmake road that is equally absent.
//!
//! A refusal is a fourth OUTCOME beside the three drivers, on purpose.
//! Returning `Driver::CMake` there would be the failure mode this repository
//! keeps paying for: an answer that reads as support.

use std::path::{Path, PathBuf};

use crate::orchestration::image::ImageBlock;

/// Which native tool builds this image, and whether stage 4 must emit a root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Driver {
    /// `cargo` over the generated entry, which is its own cargo root, with its
    /// settings from `build/<coord>/<entry>/nros-cargo.toml` (RFC-0098 D1/D9).
    Cargo,
    /// `cmake` over a generated `CMakeLists.txt`. Also the answer for a
    /// workspace mixing languages.
    CMake,
    /// `west build -b <board>`. Emits NO root: a Zephyr app is already a
    /// complete cmake project and its Kconfig overlays are user intent.
    West,
}

impl Driver {
    /// Whether stage 4 emits a root build file for this driver.
    ///
    /// The rule RFC-0065 D3 states: *stage 4 emits a root only where a root
    /// would otherwise be hand-written.* A west application ships its own.
    #[must_use]
    pub fn needs_generated_root(self) -> bool {
        matches!(self, Driver::Cargo | Driver::CMake)
    }

    /// The program stage 5 execs.
    #[must_use]
    pub fn program(self) -> &'static str {
        match self {
            Driver::Cargo => "cargo",
            Driver::CMake => "cmake",
            Driver::West => "west",
        }
    }
}

/// One resolved build: a bringup, an image within it, and how to build it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildPlan {
    /// Bringup package directory holding the `system.toml` that declared it.
    pub bringup_dir: PathBuf,
    /// Bringup package name — needed to disambiguate in messages.
    pub bringup: String,
    /// The `[image.<id>]` key.
    pub image_id: String,
    /// The image with `[image_defaults]` already folded in.
    pub image: ImageBlock,
    pub driver: Driver,
}

/// The refusal `driver_for` returns when no road can build this image.
///
/// Carries the prose the user reads. It is a struct rather than a bare `String`
/// so a call site that only wants "is this the cmake road?" reads as a question
/// about the road and not as error handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoRoad(pub String);

impl std::fmt::Display for NoRoad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Choose a driver from the board's platform and the workspace's languages.
///
/// `platform` is the board descriptor's platform token (`zephyr`, `esp32`,
/// `freertos`, `posix`, …); `has_non_rust` says whether any discovered package
/// builds C or C++.
///
/// `Err` is a fourth answer and not an accident — see the module doc. The one
/// combination that produces it is an esp32 board with a graph that crosses
/// languages, which had a road (`idf.py`) that could not work.
pub fn driver_for(platform: &str, has_non_rust: bool) -> Result<Driver, NoRoad> {
    match platform {
        "zephyr" => Ok(Driver::West),
        "esp32" if has_non_rust => Err(NoRoad(ESP32_CROSS_LANGUAGE.to_string())),
        // Not a "mixed" special case — cmake simply wins whenever the graph
        // crosses languages, because corrosion makes cargo consumable from
        // cmake and nothing makes cmake consumable from cargo.
        _ if has_non_rust => Ok(Driver::CMake),
        _ => Ok(Driver::Cargo),
    }
}

/// Why an esp32 image whose graph crosses languages has no road.
///
/// Written out rather than summarised because every one of the four clauses was
/// measured, and a reader who only hears "unsupported" will reasonably assume a
/// missing flag. The issue carries the evidence and the acceptance for undoing
/// this.
const ESP32_CROSS_LANGUAGE: &str = "\
this image's board is an ESP32 board and its package graph crosses languages, \
and nano-ros has no build road for that combination.\n\n\
Until 2026-09-28 it chose `idf.py`, which could not have worked: the ESP-IDF \
component was retired in phase-468 W2, so there is nothing for an ESP-IDF \
project to register; nothing emits an ESP-IDF project; no carrier delivers a \
resolved knob to an `idf.py` build; and `nros setup` cannot provision ESP-IDF. \
Choosing cmake instead would be the same claim in a different tool.\n\n\
What works today:\n\n  \
* ESP32 in pure Rust — the esp-hal bare-metal road, `nros build` over the \
cargo driver. `book/src/getting-started/esp32.md` is the walkthrough.\n  \
* C or C++ on another target — every other platform token has a road.\n\n\
Tracked as issue 1525, which states what re-adding an ESP-IDF road would need.";

/// [`driver_for`], refined by the BOARD's entry shape.
///
/// The esp32 board this tree ships is esp-hal on bare metal:
/// `nros-board-esp32-qemu`'s entry is a Rust `board-run` binary
/// (`#[esp_hal::main]`) and nothing about it involves ESP-IDF. That image is a
/// CARGO image, decided here rather than by the platform token alone — deciding
/// it by platform made `nros build esp32` exec a tool with no project to build
/// (`could not exec idf.py`), which is why the fixture lane built that image by
/// hand with `cargo build -p` and could not stop.
///
/// Every other esp32 shape reaches [`driver_for`]'s refusal.
pub fn driver_for_board(
    platform: &str,
    entry_kind: crate::orchestration::board_descriptor::EntryKind,
    has_non_rust: bool,
) -> Result<Driver, NoRoad> {
    use crate::orchestration::board_descriptor::EntryKind;
    if platform == "esp32" && entry_kind == EntryKind::BoardRun && !has_non_rust {
        return Ok(Driver::Cargo);
    }
    driver_for(platform, has_non_rust)
}

/// An image id qualified by its bringup, for messages and for `--image`.
#[must_use]
pub fn qualified(bringup: &str, image_id: &str) -> String {
    format!("{bringup}:{image_id}")
}

/// Every image declared across every bringup, with defaults folded in.
///
/// The `Vec` is ordered by (bringup, image id) so output is reproducible.
#[must_use]
pub fn all_images(
    bringups: &[(String, PathBuf, ImageSet)],
) -> Vec<(String, PathBuf, String, ImageBlock)> {
    let mut out = Vec::new();
    for (name, dir, set) in bringups {
        for (id, img) in &set.images {
            let folded = match &set.defaults {
                Some(base) => img.with_base(base),
                None => img.clone(),
            };
            out.push((name.clone(), dir.clone(), id.clone(), folded));
        }
    }
    out.sort_by(|a, b| (&a.0, &a.2).cmp(&(&b.0, &b.2)));
    out
}

/// The `[image.*]` half of one bringup's `system.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImageSet {
    pub images: std::collections::BTreeMap<String, ImageBlock>,
    pub defaults: Option<ImageBlock>,
    pub default_images: Vec<String>,
}

/// Resolve the requested image(s).
///
/// `requested` is what the user typed — bare (`native`) or qualified
/// (`demo_bringup:native`). Empty means "use the declared defaults".
///
/// Errors are the product here: every failure names what was asked for, what
/// exists, and how to disambiguate. A builder that guesses builds the wrong
/// system and reports success.
pub fn resolve(
    bringups: &[(String, PathBuf, ImageSet)],
    requested: &[String],
) -> Result<Vec<(String, PathBuf, String, ImageBlock)>, String> {
    let all = all_images(bringups);
    if all.is_empty() {
        return Err("this workspace declares no `[image.*]`. An image is the \
                    buildable unit — see RFC-0065 D6."
            .to_string());
    }

    if !requested.is_empty() {
        let mut out = Vec::new();
        for want in requested {
            out.push(pick_one(&all, want)?);
        }
        return Ok(out);
    }

    // No argument: honour every bringup's `default_images`.
    let defaults: Vec<(String, PathBuf, String, ImageBlock)> = bringups
        .iter()
        .flat_map(|(name, _, set)| {
            set.default_images
                .iter()
                .map(move |id| qualified(name, id))
                .collect::<Vec<_>>()
        })
        .map(|q| pick_one(&all, &q))
        .collect::<Result<_, _>>()?;
    if !defaults.is_empty() {
        return Ok(defaults);
    }

    if all.len() == 1 {
        return Ok(all);
    }

    Err(ambiguity_message(&all))
}

fn pick_one(
    all: &[(String, PathBuf, String, ImageBlock)],
    want: &str,
) -> Result<(String, PathBuf, String, ImageBlock), String> {
    let (want_bringup, want_id) = match want.split_once(':') {
        Some((b, i)) => (Some(b), i),
        None => (None, want),
    };
    let hits: Vec<&(String, PathBuf, String, ImageBlock)> = all
        .iter()
        .filter(|(b, _, id, _)| id == want_id && want_bringup.is_none_or(|wb| wb == b))
        .collect();
    match hits.len() {
        1 => Ok(hits[0].clone()),
        0 => {
            let mut known: Vec<String> = all.iter().map(|(b, _, i, _)| qualified(b, i)).collect();
            known.sort();
            Err(format!("no image `{want}`. Declared: {}", known.join(", ")))
        }
        _ => {
            // F7 — two bringups both declaring `native` is normal, and the
            // builder must not pick one. (Normal to DECLARE; two of them cannot
            // both be GENERATED, because a generated entry, facade and cmake
            // target are keyed on the id — `cmd::build::generated_outputs`
            // refuses that pair, issue 1582.)
            let mut which: Vec<String> = hits.iter().map(|(b, _, i, _)| qualified(b, i)).collect();
            which.sort();
            Err(format!(
                "`{want}` is declared by {} bringups: {}. Qualify it as \
                 `<bringup>:{want_id}`.",
                hits.len(),
                which.join(", ")
            ))
        }
    }
}

fn ambiguity_message(all: &[(String, PathBuf, String, ImageBlock)]) -> String {
    let mut names: Vec<String> = all.iter().map(|(b, _, i, _)| qualified(b, i)).collect();
    names.sort();
    format!(
        "this workspace declares {} images and no default.\n\n  {}\n\n  \
         build one:   nros build {}\n  build all:   nros build --all\n  \
         or declare:  [system] default_images = [\"{}\"]",
        names.len(),
        names.join("\n  "),
        all[0].2,
        all[0].2
    )
}

/// Resolve the bringup directory an image belongs to, for stage 4.
#[must_use]
pub fn bringup_of(plan: &BuildPlan) -> &Path {
    &plan.bringup_dir
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn img(board: &str) -> ImageBlock {
        ImageBlock {
            board: Some(board.to_string()),
            ..Default::default()
        }
    }

    fn set(pairs: &[(&str, &str)], defaults: &[&str]) -> ImageSet {
        ImageSet {
            images: pairs
                .iter()
                .map(|(id, b)| ((*id).to_string(), img(b)))
                .collect::<BTreeMap<_, _>>(),
            defaults: None,
            default_images: defaults.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    fn ws(entries: &[(&str, ImageSet)]) -> Vec<(String, PathBuf, ImageSet)> {
        entries
            .iter()
            .map(|(n, s)| ((*n).to_string(), PathBuf::from("/ws").join(n), s.clone()))
            .collect()
    }

    #[test]
    fn a_lone_image_needs_no_argument() {
        let b = ws(&[("demo_bringup", set(&[("native", "linux-x86_64")], &[]))]);
        let got = resolve(&b, &[]).expect("resolves");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].2, "native");
    }

    #[test]
    fn several_images_without_a_default_list_and_fail() {
        let b = ws(&[(
            "demo_bringup",
            set(
                &[
                    ("native", "linux-x86_64"),
                    ("freertos", "mps2-an385-freertos"),
                ],
                &[],
            ),
        )]);
        let e = resolve(&b, &[]).expect_err("must not guess");
        assert!(e.contains("no default"), "{e}");
        assert!(e.contains("nros build --all"), "offers the escape: {e}");
        assert!(e.contains("default_images"), "offers the fix: {e}");
    }

    #[test]
    fn default_images_is_honoured() {
        let b = ws(&[(
            "demo_bringup",
            set(
                &[
                    ("native", "linux-x86_64"),
                    ("freertos", "mps2-an385-freertos"),
                ],
                &["native"],
            ),
        )]);
        let got = resolve(&b, &[]).expect("resolves");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].2, "native");
    }

    #[test]
    fn an_id_declared_by_two_bringups_demands_qualification() {
        // phase-383 F7 — nano-ros-rt-eval has demo_bringup AND load_bringup.
        let b = ws(&[
            ("demo_bringup", set(&[("native", "linux-x86_64")], &[])),
            ("load_bringup", set(&[("native", "linux-x86_64")], &[])),
        ]);
        let e = resolve(&b, &["native".to_string()]).expect_err("ambiguous");
        assert!(e.contains("demo_bringup:native"), "{e}");
        assert!(e.contains("load_bringup:native"), "{e}");
        assert!(e.contains("Qualify"), "{e}");
    }

    #[test]
    fn a_qualified_id_resolves_across_bringups() {
        let b = ws(&[
            ("demo_bringup", set(&[("native", "linux-x86_64")], &[])),
            ("load_bringup", set(&[("native", "linux-x86_64")], &[])),
        ]);
        let got = resolve(&b, &["load_bringup:native".to_string()]).expect("resolves");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, "load_bringup");
    }

    #[test]
    fn several_images_can_be_requested_at_once() {
        // phase-383 F10 — `cargo build -p native_entry -p peer_entry` is
        // nano-ros-rt-eval's actual `just build`.
        let b = ws(&[(
            "demo_bringup",
            set(&[("native", "linux-x86_64"), ("peer", "linux-x86_64")], &[]),
        )]);
        let got = resolve(&b, &["native".to_string(), "peer".to_string()]).expect("resolves");
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn an_unknown_image_lists_what_exists() {
        let b = ws(&[("demo_bringup", set(&[("native", "linux-x86_64")], &[]))]);
        let e = resolve(&b, &["nativ".to_string()]).expect_err("must reject");
        assert!(e.contains("demo_bringup:native"), "{e}");
    }

    #[test]
    fn defaults_fold_into_each_image() {
        let mut s = set(&[("native", "linux-x86_64")], &[]);
        s.defaults = Some(ImageBlock {
            rmw: Some("zenoh".to_string()),
            ..Default::default()
        });
        let got = resolve(&ws(&[("demo_bringup", s)]), &[]).expect("resolves");
        assert_eq!(got[0].3.rmw.as_deref(), Some("zenoh"));
        assert_eq!(got[0].3.board.as_deref(), Some("linux-x86_64"));
    }

    #[test]
    fn a_west_application_needs_no_generated_root() {
        assert_eq!(driver_for("zephyr", false), Ok(Driver::West));
        use crate::orchestration::board_descriptor::EntryKind;
        assert_eq!(
            driver_for_board("zephyr", EntryKind::ZephyrStaticlib, false),
            Ok(Driver::West)
        );
        assert!(!driver_for("zephyr", false).unwrap().needs_generated_root());
    }

    #[test]
    fn a_pure_rust_esp32_board_run_image_is_a_cargo_image() {
        // The in-tree esp32 board is esp-hal bare metal, whose Rust entry is a
        // cargo bin and not an ESP-IDF app. The BOARD decides this, not the
        // platform token — deciding it by token exec'd `idf.py` on a project
        // that did not exist (phase-445 W4 / PR #880).
        use crate::orchestration::board_descriptor::EntryKind;
        assert_eq!(
            driver_for_board("esp32", EntryKind::BoardRun, false),
            Ok(Driver::Cargo)
        );
    }

    #[test]
    fn an_esp32_graph_that_crosses_languages_has_no_road() {
        // RFC-0065 D3, amended 2026-09-28. This combination used to answer
        // `idf.py`, a road with no ESP-IDF component to build, no carrier and
        // no way to provision the tool. The refusal is the answer; falling
        // through to cmake would be the same unsupported claim in another tool.
        use crate::orchestration::board_descriptor::EntryKind;
        for entry_kind in [
            EntryKind::BoardRun,
            EntryKind::HostedMain,
            EntryKind::ZephyrStaticlib,
        ] {
            let e = driver_for_board("esp32", entry_kind, true)
                .expect_err("an esp32 cross-language graph has no road");
            assert!(e.0.contains("1525"), "the refusal names its issue: {e}");
            assert!(
                e.0.contains("esp-hal"),
                "the refusal names the road that does work: {e}"
            );
        }
        // The refusal is about the LANGUAGE MIX, not about the entry shape: a
        // pure-Rust esp32 image is a cargo image whatever its board declares,
        // because cargo is a road that exists for it.
        assert_eq!(
            driver_for_board("esp32", EntryKind::HostedMain, false),
            Ok(Driver::Cargo)
        );
    }

    #[test]
    fn cmake_wins_whenever_the_graph_crosses_languages() {
        // RFC-0024 §6.3 — corrosion makes cargo consumable from cmake; nothing
        // makes cmake consumable from cargo. "Mixed" is not a fourth driver.
        assert_eq!(driver_for("posix", true), Ok(Driver::CMake));
        assert_eq!(driver_for("freertos", true), Ok(Driver::CMake));
        assert_eq!(driver_for("posix", false), Ok(Driver::Cargo));
    }

    #[test]
    fn an_empty_workspace_says_what_is_missing() {
        let e = resolve(&[], &[]).expect_err("nothing to build");
        assert!(e.contains("[image.*]"), "{e}");
    }
}
