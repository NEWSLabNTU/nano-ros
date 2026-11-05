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
//! build, and `nros setup` cannot provision ESP-IDF. That combination
//! REFUSED, naming issue 1525, rather than exec'ing a tool with no project to
//! build or falling through to a cmake road that is equally absent. Since
//! phase-521 W6 the ESP32 board itself is retired (RFC-0113 D5), so every
//! `esp32` image refuses, whatever its languages.
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
/// platform that produces it is `esp32`, retired with its board (RFC-0113 D5).
pub fn driver_for(platform: &str, has_non_rust: bool) -> Result<Driver, NoRoad> {
    match platform {
        "zephyr" => Ok(Driver::West),
        "esp32" => Err(NoRoad(ESP32_RETIRED.to_string())),
        // Not a "mixed" special case — cmake simply wins whenever the graph
        // crosses languages, because corrosion makes cargo consumable from
        // cmake and nothing makes cmake consumable from cargo.
        _ if has_non_rust => Ok(Driver::CMake),
        _ => Ok(Driver::Cargo),
    }
}

/// Why an esp32 image has no road.
///
/// phase-521 W6 — the ESP32-C3 board (`nros-board-esp32-qemu`, the only esp32
/// board) is retired. Until then a pure-Rust image was a cargo image and one
/// whose graph crossed languages refused (issue 1525: no ESP-IDF road).
const ESP32_RETIRED: &str = "\
this image's board is an ESP32 board, and the ESP32 board is retired: it had no \
receive interrupt and no one-shot timer behind a wake object, so an executor on \
it could not park, and nano-ros does not support a board where it cannot keep its \
real-time wait promise (RFC-0113 D5). C or C++ on an ESP32 never had a road \
(issue 1525).";

/// [`driver_for`], refined by the BOARD's entry shape.
///
/// Since the esp32 board retired (phase-521 W6) no board's entry shape moves
/// the answer; the parameter stays so a future board can.
pub fn driver_for_board(
    platform: &str,
    entry_kind: crate::orchestration::board_descriptor::EntryKind,
    has_non_rust: bool,
) -> Result<Driver, NoRoad> {
    let _ = entry_kind;
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
/// (`demo_bringup:linux`). Empty means "use the declared defaults".
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
        // RFC-0065 D6.
        return Err("this workspace declares no `[image.*]`. An image is the \
                    buildable unit: add `[image.<id>] board = \"<board>\"` to system.toml."
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

/// The rung of RFC-0121 D5's target ladder that named the image a target verb
/// (`run`, `flash`, `monitor`) acts on. `--why` prints [`TargetRung::describe`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetRung {
    /// Rung 1: `<bringup>:<image>`, the full spelling.
    Qualified,
    /// Rung 2: `<image>`, declared by exactly one bringup.
    Image { bringup: String },
    /// Rung 3: `<node>`, the one bringup whose `[[component]]` rows name only
    /// that node's package, with that bringup's only image.
    Node { node: String, bringup: String },
    /// Rung 4: nothing named, the workspace's only image (or its declared
    /// `default_images`).
    Sole,
}

impl TargetRung {
    /// The rung, in words, for `--why`.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Qualified => "rung 1, `<bringup>:<image>` as written".to_string(),
            Self::Image { bringup } => {
                format!("rung 2, `<image>`: declared by one bringup, `{bringup}`")
            }
            Self::Node { node, bringup } => format!(
                "rung 3, `<node>`: `{node}` is the only component of `{bringup}`, \
                 whose only image this is"
            ),
            Self::Sole => "rung 4, no target named: the workspace's only image".to_string(),
        }
    }
}

/// RFC-0121 D5 — the image a target verb's word names, and the rung that
/// named it. `components` maps each bringup to the packages its
/// `[[component]]` rows name.
///
/// Returns the `<bringup>:<image>` spelling (or `None` for rung 4, which
/// [`resolve`] answers with no argument). Every case no rung answers alone is
/// refused, listing the `<bringup>:<image>` spellings that would resolve, so
/// a shortcut never picks one candidate out of several.
pub fn name_target(
    bringups: &[(String, PathBuf, ImageSet)],
    components: &std::collections::BTreeMap<String, Vec<String>>,
    word: Option<&str>,
) -> Result<(Option<String>, TargetRung), String> {
    let Some(word) = word else {
        return Ok((None, TargetRung::Sole));
    };
    if word.contains(':') {
        return Ok((Some(word.to_string()), TargetRung::Qualified));
    }
    let all = all_images(bringups);
    let image_hits: Vec<&(String, PathBuf, String, ImageBlock)> =
        all.iter().filter(|(_, _, id, _)| id == word).collect();
    if let [(b, _, id, _)] = image_hits.as_slice() {
        return Ok((
            Some(qualified(b, id)),
            TargetRung::Image { bringup: b.clone() },
        ));
    }
    let spellings_of = |bringup: &str| -> Vec<String> {
        all.iter()
            .filter(|(b, _, _, _)| b == bringup)
            .map(|(b, _, i, _)| qualified(b, i))
            .collect()
    };
    // Bringups whose rows name only `word`, and those that name it among
    // others (offered as spellings, never picked).
    let only: Vec<&String> = components
        .iter()
        .filter(|(_, pkgs)| !pkgs.is_empty() && pkgs.iter().all(|p| p == word))
        .map(|(b, _)| b)
        .collect();
    let among: Vec<&String> = components
        .iter()
        .filter(|(_, pkgs)| pkgs.iter().any(|p| p == word))
        .map(|(b, _)| b)
        .collect();
    if let [b] = only.as_slice()
        && image_hits.is_empty()
    {
        let spellings = spellings_of(b);
        return match spellings.as_slice() {
            [one] => Ok((
                Some(one.clone()),
                TargetRung::Node {
                    node: word.to_string(),
                    bringup: (*b).clone(),
                },
            )),
            [] => Err(format!(
                "`{word}` is the only component of `{b}`, which declares no `[image.*]`. \
                 Add `[image.<id>] board = \"<board>\"` to its system.toml."
            )),
            many => Err(format!(
                "`{word}` is the only component of `{b}`, which declares {} images; name \
                 one:\n  {}",
                many.len(),
                many.join("\n  ")
            )),
        };
    }
    // Refused: list every spelling that would resolve.
    let mut spellings: Vec<String> = image_hits
        .iter()
        .map(|(b, _, i, _)| qualified(b, i))
        .collect();
    for b in &among {
        spellings.extend(spellings_of(b));
    }
    spellings.sort();
    spellings.dedup();
    let why = match (image_hits.len(), only.len(), among.len()) {
        (0, 0, 0) => {
            let mut known: Vec<String> = all.iter().map(|(b, _, i, _)| qualified(b, i)).collect();
            known.sort();
            return Err(format!(
                "no image or node `{word}`: no bringup declares an image `{word}` and no \
                 bringup's `[[component]]` rows name a package `{word}`. Declared: {}",
                if known.is_empty() {
                    "(none)".to_string()
                } else {
                    known.join(", ")
                }
            ));
        }
        (0, 0, _) => format!(
            "`{word}` is a component of {}, but never the only one, so it names no one image",
            among
                .iter()
                .map(|b| format!("`{b}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        (0, _, _) => format!(
            "`{word}` is the only component of {} bringups ({})",
            only.len(),
            only.iter()
                .map(|b| format!("`{b}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        (n, 0, 0) => format!("`{word}` is an image declared by {n} bringups"),
        (n, _, _) => {
            format!("`{word}` is an image declared by {n} bringups, and also a node package")
        }
    };
    Err(format!("{why}; name one:\n  {}", spellings.join("\n  ")))
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

    fn comps(rows: &[(&str, &[&str])]) -> std::collections::BTreeMap<String, Vec<String>> {
        rows.iter()
            .map(|(b, p)| {
                (
                    (*b).to_string(),
                    p.iter().map(|s| (*s).to_string()).collect(),
                )
            })
            .collect()
    }

    /// RFC-0121 D5 — each rung answers its own case and says which it was.
    #[test]
    fn each_target_rung_names_its_image() {
        let b = ws(&[
            (
                "sensor_node_bringup",
                set(&[("baremetal", "mps2-an385-baremetal")], &[]),
            ),
            (
                "demo_bringup",
                set(&[("linux", "linux"), ("freertos", "f")], &[]),
            ),
        ]);
        let c = comps(&[
            ("sensor_node_bringup", &["sensor_node"]),
            ("demo_bringup", &["talker_pkg", "listener_pkg"]),
        ]);
        assert_eq!(name_target(&b, &c, None).unwrap(), (None, TargetRung::Sole));
        assert_eq!(
            name_target(&b, &c, Some("demo_bringup:linux")).unwrap(),
            (Some("demo_bringup:linux".into()), TargetRung::Qualified)
        );
        assert_eq!(
            name_target(&b, &c, Some("freertos")).unwrap(),
            (
                Some("demo_bringup:freertos".into()),
                TargetRung::Image {
                    bringup: "demo_bringup".into()
                }
            )
        );
        let (q, rung) = name_target(&b, &c, Some("sensor_node")).unwrap();
        assert_eq!(q.as_deref(), Some("sensor_node_bringup:baremetal"));
        assert_eq!(
            rung,
            TargetRung::Node {
                node: "sensor_node".into(),
                bringup: "sensor_node_bringup".into()
            }
        );
        assert!(rung.describe().contains("rung 3"), "{}", rung.describe());
    }

    /// Two bringups naming one node refuse, listing both; so does a node
    /// whose one bringup has several images, a node that is never the only
    /// component, and a word nothing declares.
    #[test]
    fn every_ambiguous_target_is_refused_with_its_spellings() {
        let b = ws(&[
            ("a_bringup", set(&[("linux", "linux")], &[])),
            ("b_bringup", set(&[("baremetal", "m")], &[])),
            ("c_bringup", set(&[("x", "linux"), ("y", "linux")], &[])),
            ("d_bringup", set(&[("z", "linux")], &[])),
        ]);
        let c = comps(&[
            ("a_bringup", &["sensor_node"]),
            ("b_bringup", &["sensor_node"]),
            ("c_bringup", &["cam_node"]),
            ("d_bringup", &["cam_node", "other_node"]),
        ]);
        let e = name_target(&b, &c, Some("sensor_node")).unwrap_err();
        assert!(
            e.contains("a_bringup:linux") && e.contains("b_bringup:baremetal"),
            "{e}"
        );
        assert!(e.contains("2 bringups"), "{e}");

        let e = name_target(&b, &c, Some("cam_node")).unwrap_err();
        assert!(
            e.contains("c_bringup:x") && e.contains("c_bringup:y"),
            "{e}"
        );

        let e = name_target(&b, &c, Some("other_node")).unwrap_err();
        assert!(
            e.contains("never the only one") && e.contains("d_bringup:z"),
            "{e}"
        );

        let e = name_target(&b, &c, Some("nope")).unwrap_err();
        assert!(e.contains("no image or node `nope`"), "{e}");

        // An image two bringups declare is refused, listing both spellings.
        let b2 = ws(&[
            ("a_bringup", set(&[("linux", "linux")], &[])),
            ("b_bringup", set(&[("linux", "linux")], &[])),
        ]);
        let e = name_target(&b2, &comps(&[]), Some("linux")).unwrap_err();
        assert!(
            e.contains("a_bringup:linux") && e.contains("b_bringup:linux"),
            "{e}"
        );
    }

    #[test]
    fn a_lone_image_needs_no_argument() {
        let b = ws(&[("demo_bringup", set(&[("linux", "linux-x86_64")], &[]))]);
        let got = resolve(&b, &[]).expect("resolves");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].2, "linux");
    }

    #[test]
    fn several_images_without_a_default_list_and_fail() {
        let b = ws(&[(
            "demo_bringup",
            set(
                &[
                    ("linux", "linux-x86_64"),
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
                    ("linux", "linux-x86_64"),
                    ("freertos", "mps2-an385-freertos"),
                ],
                &["linux"],
            ),
        )]);
        let got = resolve(&b, &[]).expect("resolves");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].2, "linux");
    }

    #[test]
    fn an_id_declared_by_two_bringups_demands_qualification() {
        // phase-383 F7 — nano-ros-rt-eval has demo_bringup AND load_bringup.
        let b = ws(&[
            ("demo_bringup", set(&[("linux", "linux-x86_64")], &[])),
            ("load_bringup", set(&[("linux", "linux-x86_64")], &[])),
        ]);
        let e = resolve(&b, &["linux".to_string()]).expect_err("ambiguous");
        assert!(e.contains("demo_bringup:linux"), "{e}");
        assert!(e.contains("load_bringup:linux"), "{e}");
        assert!(e.contains("Qualify"), "{e}");
    }

    #[test]
    fn a_qualified_id_resolves_across_bringups() {
        let b = ws(&[
            ("demo_bringup", set(&[("linux", "linux-x86_64")], &[])),
            ("load_bringup", set(&[("linux", "linux-x86_64")], &[])),
        ]);
        let got = resolve(&b, &["load_bringup:linux".to_string()]).expect("resolves");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, "load_bringup");
    }

    #[test]
    fn several_images_can_be_requested_at_once() {
        // phase-383 F10 — `cargo build -p linux_entry -p peer_entry` is
        // nano-ros-rt-eval's actual `just build`.
        let b = ws(&[(
            "demo_bringup",
            set(&[("linux", "linux-x86_64"), ("peer", "linux-x86_64")], &[]),
        )]);
        let got = resolve(&b, &["linux".to_string(), "peer".to_string()]).expect("resolves");
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn an_unknown_image_lists_what_exists() {
        let b = ws(&[("demo_bringup", set(&[("linux", "linux-x86_64")], &[]))]);
        let e = resolve(&b, &["nativ".to_string()]).expect_err("must reject");
        assert!(e.contains("demo_bringup:linux"), "{e}");
    }

    #[test]
    fn defaults_fold_into_each_image() {
        let mut s = set(&[("linux", "linux-x86_64")], &[]);
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
    fn every_esp32_image_is_refused_naming_rfc_0113() {
        // phase-521 W6 — the esp32 board is retired (RFC-0113 D5), so the
        // pure-Rust image that used to be a cargo image refuses too.
        use crate::orchestration::board_descriptor::EntryKind;
        for entry_kind in [
            EntryKind::BoardRun,
            EntryKind::HostedMain,
            EntryKind::ZephyrStaticlib,
        ] {
            for has_non_rust in [false, true] {
                let e = driver_for_board("esp32", entry_kind, has_non_rust)
                    .expect_err("an esp32 image has no road");
                assert!(e.0.contains("RFC-0113 D5"), "{e}");
                assert!(e.0.contains("1525"), "{e}");
            }
        }
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
