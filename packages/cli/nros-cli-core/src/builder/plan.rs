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
//! decides is the board — a Zephyr board means `west`, an ESP32 board means
//! `idf.py`, and neither needs a generated root at all.

use std::path::{Path, PathBuf};

use crate::orchestration::image::ImageBlock;

/// Which native tool builds this image, and whether stage 4 must emit a root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Driver {
    /// `cargo` over a synthesized `[workspace] members` root.
    Cargo,
    /// `cmake` over a generated `CMakeLists.txt`. Also the answer for a
    /// workspace mixing languages.
    CMake,
    /// `west build -b <board>`. Emits NO root: a Zephyr app is already a
    /// complete cmake project and its Kconfig overlays are user intent.
    West,
    /// `idf.py build`. Emits no root, same reasoning.
    IdfPy,
}

impl Driver {
    /// Whether stage 4 emits a root build file for this driver.
    ///
    /// The rule RFC-0065 D3 states: *stage 4 emits a root only where a root
    /// would otherwise be hand-written.* west and ESP-IDF apps ship their own.
    #[must_use]
    pub fn needs_generated_root(self) -> bool {
        matches!(self, Driver::Cargo | Driver::CMake)
    }

    /// Whether this driver's entry package must be EXCLUDED from the cargo
    /// root rather than listed as a member.
    ///
    /// Not the same question as [`Self::needs_generated_root`], though the two
    /// were conflated until phase-383 W9.b. That one asks whether stage 4 emits
    /// a root; this one asks whether the package can be a cargo member at all.
    ///
    /// Only west: a Zephyr entry is a `staticlib` built by `west` through
    /// `rust_cargo_application()`, carries its own `CMakeLists.txt`, and
    /// deliberately declares no `[workspace]` of its own. The eight
    /// hand-written roots excluded exactly their west entries and nothing else.
    ///
    /// An ESP-IDF entry is an ordinary cargo package — `esp32_entry` is a
    /// `Cargo.toml`, a `package.xml` and `src/`, with no CMakeLists — and
    /// `idf.py` wraps a cargo build of it. Excluding it broke the fixture row
    /// that builds the same package directly (`cargo build -p esp32_entry
    /// --target riscv32imc-unknown-none-elf` → "package ID specification
    /// `esp32_entry` did not match any packages"), because an excluded package
    /// is not a member.
    ///
    /// Cross-target membership is not itself a reason to exclude: `freertos_entry`
    /// and `nuttx_entry` are members and always were. What protects a bare
    /// `cargo build` at the root is that nothing here ever runs one — every
    /// build names its package with `-p`.
    #[must_use]
    pub fn excluded_from_cargo_root(self) -> bool {
        matches!(self, Driver::West)
    }

    /// The program stage 5 execs.
    #[must_use]
    pub fn program(self) -> &'static str {
        match self {
            Driver::Cargo => "cargo",
            Driver::CMake => "cmake",
            Driver::West => "west",
            Driver::IdfPy => "idf.py",
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

/// Choose a driver from the board's platform and the workspace's languages.
///
/// `platform` is the board descriptor's platform token (`zephyr`, `esp32`,
/// `freertos`, `posix`, …); `has_non_rust` says whether any discovered package
/// builds C or C++.
#[must_use]
pub fn driver_for(platform: &str, has_non_rust: bool) -> Driver {
    match platform {
        "zephyr" => Driver::West,
        "esp32" => Driver::IdfPy,
        // Not a "mixed" special case — cmake simply wins whenever the graph
        // crosses languages, because corrosion makes cargo consumable from
        // cmake and nothing makes cmake consumable from cargo.
        _ if has_non_rust => Driver::CMake,
        _ => Driver::Cargo,
    }
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
            // builder must not pick one.
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
    fn zephyr_and_esp32_need_no_generated_root() {
        assert_eq!(driver_for("zephyr", false), Driver::West);
        assert_eq!(driver_for("esp32", false), Driver::IdfPy);
        assert!(!driver_for("zephyr", false).needs_generated_root());
        assert!(!driver_for("esp32", true).needs_generated_root());
    }

    #[test]
    fn cmake_wins_whenever_the_graph_crosses_languages() {
        // RFC-0024 §6.3 — corrosion makes cargo consumable from cmake; nothing
        // makes cmake consumable from cargo. "Mixed" is not a fourth driver.
        assert_eq!(driver_for("posix", true), Driver::CMake);
        assert_eq!(driver_for("freertos", true), Driver::CMake);
        assert_eq!(driver_for("posix", false), Driver::Cargo);
    }

    #[test]
    fn an_empty_workspace_says_what_is_missing() {
        let e = resolve(&[], &[]).expect_err("nothing to build");
        assert!(e.contains("[image.*]"), "{e}");
    }
}
