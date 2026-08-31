//! `[image.<id>]` — the buildable unit (RFC-0065 D6, phase-383 W1).
//!
//! ## Why this is a NEW table and not a rename of `[deploy.*]`
//!
//! `[deploy.*]` is **not ours**. It is
//! [`ros_launch_manifest_model::system_config::DeployBlock`], an upstream
//! `#[serde(deny_unknown_fields)]` type, so a field added there has to land in
//! another repository first — the wall RFC-0078 hit and recorded in
//! [`super::cargo_metadata_schema`].
//!
//! More importantly the two answer different questions, and upstream has
//! already paid for the merge. Its placement resolver filters our half out:
//!
//! ```ignore
//! let partitioning = self.deploy.iter()
//!     .filter(|(_, b)| b.applies_to_launch(launch_file))
//!     .filter(|(_, b)| b.kind.as_deref() != Some("embedded"));   // excluded
//! ```
//!
//! …commenting that this is "a conflated axis", because "with SEVERAL
//! [embedded blocks], the fallback asks which of N whole-system board builds
//! runs a given node, and the answer is 'all of them' — a node→target map
//! cannot say that".
//!
//! The relation is N:M, and all three cells occur in ONE `system.toml`
//! (`examples/workspaces/rust/src/demo_bringup/`):
//!
//! | case | example |
//! | --- | --- |
//! | placement, no image | `[deploy.robot1]` / `[deploy.robot2]` — no entry names either |
//! | image, no placement | every `kind = "embedded"` block |
//! | both | `[deploy.native]` — a machine that is also a host build |
//!
//! So: **`[deploy.*]` answers "which nodes run where"; `[image.*]` answers
//! "what do I compile, for which board".** An image needs no deploy block and a
//! deploy block needs no image.
//!
//! ## What an image is
//!
//! One image is one `(launch, args, board)` bake. Everything else the builder
//! needs is derived from those three — RFC-0065 D4.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::board_descriptor::{BoardCatalog, BoardDescriptor, DeployResolution};

/// `[image]` / `[image.<id>]` — a buildable image.
///
/// `deny_unknown_fields` for upstream's stated reason: a mistyped key must be
/// an error rather than a silent drop. A silently ignored `board` key is a
/// build for the wrong target that reports success.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageBlock {
    /// nano-ros board id — resolved through `packages/boards/board-support.toml`
    /// (RFC-0065 D9). NEVER a framework's own board string: the registry
    /// carries `framework_board` for platforms that have one, so
    /// `native_sim/native/64` is a resolution RESULT, not something authored
    /// here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board: Option<String>,

    /// Launch file this image bakes, relative to the bringup pkg. Absent ⇒ the
    /// system's `default_launch`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch: Option<String>,

    /// Launch arguments bound at resolve time — `{ host = "robot1" }`.
    ///
    /// This is how an image selects a MACHINE: `native_entry_robot1` differs
    /// from `native_entry_robot2` only here. Placement itself stays in
    /// `[deploy.*]`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, String>,

    /// Panic policy, forwarded verbatim to `nros::main!` — the EXISTING
    /// RFC-0077 enum (`platform` | `halt` | `own`), never a crate name.
    /// `own` means something else in the image carries the `#[panic_handler]`;
    /// a support package (RFC-0065 D12) is that slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panic: Option<String>,

    /// RMW backend for this image. Absent ⇒ the system header's `rmw`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rmw: Option<String>,

    /// ROS edition. Absent ⇒ the system header's `ros_edition`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ros_edition: Option<String>,

    /// Cargo/CMake build profile name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,

    /// Framework build-variant name — mapped onto Zephyr's own
    /// `prj_<buildtype>.conf` → `CONF_FILE_BUILD_TYPE` mechanism rather than a
    /// parallel axis (phase-383 W5.c). `autoware-safety-island`'s
    /// `prj_actuation.conf` is already this shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,

    /// Extra framework config fragments for THIS image, in order.
    ///
    /// Per-image, not per-board: `nano-ros-rt-eval` builds one app on one board
    /// twice — with `prj-edf.conf` and without — and `autoware-safety-island`
    /// carries `tracing.conf` / `tracing_stats.conf`, named by concern
    /// (phase-383 F3).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conf: Vec<String>,

    /// The workspace package that IS this image's framework application —
    /// the directory a framework driver is pointed at (for Zephyr, the one
    /// holding the `CMakeLists.txt` west builds).
    ///
    /// Normally UNSET and derived: an entry package declares the deploy token
    /// it serves (`[package.metadata.nros.entry] deploy` in Cargo.toml,
    /// `nano_ros_add_executable(... DEPLOY <token>)` in CMakeLists.txt), and
    /// exactly one package in the workspace claims a given board.
    ///
    /// Set it when that is AMBIGUOUS. `examples/workspaces/realtime-cpp` has
    /// two — `zephyr_entry` and `fvp_entry`, both `DEPLOY zephyr`, both on
    /// `native_sim/native/64` — for two images that differ in what they
    /// deploy, not in which board they target. Deriving there is a coin flip
    /// between two right-looking answers, so the resolver refuses and names
    /// the candidates instead of picking one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,

    /// Capability axes for this image, over the system's own list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
}

impl ImageBlock {
    /// Overlay `self` (an `[image.<id>]`) onto `base` (the `[image]` table),
    /// per-field, with the specific block winning.
    ///
    /// RFC-0065 D5.1: without a base table an eight-image workspace repeats its
    /// RMW and edition eight times, and eight copies of one fact is how they
    /// start disagreeing.
    #[must_use]
    pub fn with_base(&self, base: &ImageBlock) -> ImageBlock {
        fn pick(specific: &Option<String>, base: &Option<String>) -> Option<String> {
            specific.clone().or_else(|| base.clone())
        }
        ImageBlock {
            board: pick(&self.board, &base.board),
            launch: pick(&self.launch, &base.launch),
            // Maps MERGE (base first, so the specific block overwrites a key it
            // also sets); scalars replace. A base `args` is a default binding
            // set, not an all-or-nothing choice.
            args: {
                let mut merged = base.args.clone();
                merged.extend(self.args.clone());
                merged
            },
            panic: pick(&self.panic, &base.panic),
            rmw: pick(&self.rmw, &base.rmw),
            ros_edition: pick(&self.ros_edition, &base.ros_edition),
            profile: pick(&self.profile, &base.profile),
            variant: pick(&self.variant, &base.variant),
            // Scalar, so it replaces: a base naming an entry is a default for
            // the images that do not name one, and an image that does is
            // making a choice the base cannot have made for it.
            entry: pick(&self.entry, &base.entry),
            // Lists CONCATENATE base-then-specific: conf fragments are ordered
            // and later ones override earlier, which is exactly Zephyr's
            // CONF_FILE semantics. A specific block extends the base set rather
            // than replacing it.
            conf: base.conf.iter().chain(self.conf.iter()).cloned().collect(),
            features: base
                .features
                .iter()
                .chain(self.features.iter())
                .cloned()
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> ImageBlock {
        toml::from_str(src).expect("parses")
    }

    /// The `examples/workspaces/safety` shape: one bringup, three languages.
    /// The workspace-wide "does it cross languages" answer was true for every
    /// image there, which routed the Rust ones through cmake.
    #[test]
    fn launch_node_pkgs_reads_the_pkgs_this_image_actually_names() {
        let dir = std::env::temp_dir().join(format!(
            "nros-launch-pkgs-{}-{}",
            std::process::id(),
            line!()
        ));
        let launch = dir.join("launch");
        std::fs::create_dir_all(&launch).expect("mkdir");
        std::fs::write(
            launch.join("rust_only.launch.xml"),
            r#"<launch>
  <node pkg="rust_safety_listener_pkg" exec="safe_listener"/>
  <node pkg='rust_safety_talker_pkg' exec="talker"/>
  <node pkg="rust_safety_listener_pkg" exec="second_instance"/>
</launch>"#,
        )
        .expect("write");

        let img = ImageBlock {
            launch: Some("rust_only.launch.xml".to_string()),
            ..ImageBlock::default()
        };
        assert_eq!(
            launch_node_pkgs(&img, &dir),
            vec![
                "rust_safety_listener_pkg".to_string(),
                "rust_safety_talker_pkg".to_string()
            ],
            "both quote styles read, duplicates collapsed, sorted"
        );

        // No launch, and a launch naming no file, are both "nothing known" —
        // the caller falls back to the workspace answer rather than guessing.
        assert!(launch_node_pkgs(&ImageBlock::default(), &dir).is_empty());
        let missing = ImageBlock {
            launch: Some("nonesuch.launch.xml".to_string()),
            ..ImageBlock::default()
        };
        assert!(launch_node_pkgs(&missing, &dir).is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parses_a_minimal_embedded_image() {
        let b = parse(r#"board = "mps2-an385-freertos""#);
        assert_eq!(b.board.as_deref(), Some("mps2-an385-freertos"));
        assert!(b.launch.is_none());
        assert!(b.conf.is_empty());
    }

    #[test]
    fn parses_launch_args_as_a_map() {
        let b = parse(
            r#"
            board = "linux-x86_64"
            launch = "multihost.launch.xml"
            args = { host = "robot1" }
            "#,
        );
        assert_eq!(b.args.get("host").map(String::as_str), Some("robot1"));
    }

    #[test]
    fn parses_the_per_image_conf_list_in_order() {
        // phase-383 F3 — rt-eval builds one app on one board twice, differing
        // only in this list.
        let b = parse(
            r#"
            board = "zephyr-native-sim"
            conf = ["prj-zenoh.conf", "prj-edf.conf"]
            "#,
        );
        assert_eq!(b.conf, vec!["prj-zenoh.conf", "prj-edf.conf"]);
    }

    #[test]
    fn a_mistyped_key_is_an_error_not_a_silent_drop() {
        // Upstream's reasoning, applied on our side: a silently ignored `board`
        // is a build for the wrong target that reports success.
        let e = toml::from_str::<ImageBlock>(r#"bord = "mps2-an385-freertos""#)
            .expect_err("unknown key must be rejected");
        assert!(
            e.to_string().contains("bord") || e.to_string().contains("unknown field"),
            "error should name the offending key: {e}"
        );
    }

    #[test]
    fn base_supplies_what_the_specific_block_omits() {
        let base = parse(r#"rmw = "zenoh""#);
        let specific = parse(r#"board = "mps2-an385-freertos""#);
        let merged = specific.with_base(&base);
        assert_eq!(merged.rmw.as_deref(), Some("zenoh"));
        assert_eq!(merged.board.as_deref(), Some("mps2-an385-freertos"));
    }

    #[test]
    fn specific_block_wins_over_base_for_scalars() {
        let base = parse(r#"rmw = "zenoh""#);
        let specific = parse(r#"rmw = "cyclonedds""#);
        assert_eq!(specific.with_base(&base).rmw.as_deref(), Some("cyclonedds"));
    }

    #[test]
    fn conf_lists_concatenate_base_first() {
        // Order is load-bearing: later Zephyr fragments override earlier ones,
        // so a specific block EXTENDS the base set rather than replacing it.
        let base = parse(r#"conf = ["prj-common.conf"]"#);
        let specific = parse(r#"conf = ["prj-edf.conf"]"#);
        assert_eq!(
            specific.with_base(&base).conf,
            vec!["prj-common.conf", "prj-edf.conf"]
        );
    }

    #[test]
    fn args_merge_with_the_specific_key_winning() {
        let base = parse(r#"args = { host = "robot1", mode = "sim" }"#);
        let specific = parse(r#"args = { host = "robot2" }"#);
        let merged = specific.with_base(&base);
        assert_eq!(merged.args.get("host").map(String::as_str), Some("robot2"));
        assert_eq!(merged.args.get("mode").map(String::as_str), Some("sim"));
    }
}

/// How a bare `nros build` resolved its image set (phase-383 W1.c).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageSelection {
    /// Exactly these images, from an explicit argument or `default_images`.
    Images(Vec<String>),
    /// Nothing to build — the workspace declares no images at all.
    None,
    /// Several images and no declared default. `nros build` must LIST these and
    /// fail, never guess (RFC-0065 D1).
    Ambiguous(Vec<String>),
}

/// Resolve which images a bare `nros build` should build.
///
/// Order: an explicit `default_images` wins; a single declared image is
/// unambiguous; anything else is [`ImageSelection::Ambiguous`].
///
/// A `default_images` entry naming no declared image is an ERROR rather than a
/// silent skip — the same reasoning as `deny_unknown_fields` on [`ImageBlock`].
pub fn select_default_images(
    declared: &BTreeMap<String, ImageBlock>,
    default_images: &[String],
) -> Result<ImageSelection, String> {
    if !default_images.is_empty() {
        let unknown: Vec<&str> = default_images
            .iter()
            .filter(|n| !declared.contains_key(*n))
            .map(String::as_str)
            .collect();
        if !unknown.is_empty() {
            let mut known: Vec<&str> = declared.keys().map(String::as_str).collect();
            known.sort_unstable();
            return Err(format!(
                "`default_images` names {} that no `[image.*]` declares: {}. Declared: {}",
                if unknown.len() == 1 {
                    "an image"
                } else {
                    "images"
                },
                unknown.join(", "),
                if known.is_empty() {
                    "(none)".to_string()
                } else {
                    known.join(", ")
                }
            ));
        }
        return Ok(ImageSelection::Images(default_images.to_vec()));
    }
    let mut names: Vec<String> = declared.keys().cloned().collect();
    names.sort();
    match names.len() {
        0 => Ok(ImageSelection::None),
        1 => Ok(ImageSelection::Images(names)),
        _ => Ok(ImageSelection::Ambiguous(names)),
    }
}

/// Panic policies `nros::main!` accepts — the EXISTING RFC-0077 enum.
///
/// Listed here so `[image.<id>] panic = …` is validated at parse time rather
/// than at macro-expansion time, where the error names a generated file the
/// user never wrote. NOT a new vocabulary: `"semihosting"` is a crate, and the
/// policy that admits it is `own`.
pub const PANIC_POLICIES: &[&str] = &["platform", "halt", "own"];

/// Validate an image's `panic` against [`PANIC_POLICIES`].
pub fn validate_panic(policy: Option<&str>) -> Result<(), String> {
    match policy {
        None => Ok(()),
        Some(p) if PANIC_POLICIES.contains(&p) => Ok(()),
        Some(other) => Err(format!(
            "`panic = \"{other}\"` is not a policy (expected one of: {}). \
             A crate such as `panic-semihosting` or `esp-backtrace` is selected \
             UNDER the `own` policy, not instead of one.",
            PANIC_POLICIES.join(" | ")
        )),
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;

    fn images(names: &[&str]) -> BTreeMap<String, ImageBlock> {
        names
            .iter()
            .map(|n| ((*n).to_string(), ImageBlock::default()))
            .collect()
    }

    #[test]
    fn a_single_image_is_unambiguous() {
        let got = select_default_images(&images(&["native"]), &[]).expect("ok");
        assert_eq!(got, ImageSelection::Images(vec!["native".to_string()]));
    }

    #[test]
    fn several_images_without_a_default_are_ambiguous() {
        // RFC-0065 D1 — list and fail, never guess. `examples/workspaces/rust`
        // declares eight images across three cross toolchains.
        let got =
            select_default_images(&images(&["native", "freertos", "zephyr"]), &[]).expect("ok");
        assert_eq!(
            got,
            ImageSelection::Ambiguous(vec![
                "freertos".to_string(),
                "native".to_string(),
                "zephyr".to_string()
            ])
        );
    }

    #[test]
    fn default_images_disambiguates() {
        let got = select_default_images(&images(&["native", "freertos"]), &["native".to_string()])
            .expect("ok");
        assert_eq!(got, ImageSelection::Images(vec!["native".to_string()]));
    }

    #[test]
    fn no_images_declared_selects_nothing() {
        assert_eq!(
            select_default_images(&images(&[]), &[]).expect("ok"),
            ImageSelection::None
        );
    }

    #[test]
    fn a_default_naming_no_declared_image_is_an_error() {
        let e = select_default_images(&images(&["native"]), &["nativ".to_string()])
            .expect_err("must reject");
        assert!(e.contains("nativ"), "error names the typo: {e}");
        assert!(e.contains("native"), "error lists what IS declared: {e}");
    }

    #[test]
    fn panic_accepts_only_the_rfc_0077_policies() {
        for p in PANIC_POLICIES {
            validate_panic(Some(p)).expect("policy accepted");
        }
        validate_panic(None).expect("absent is fine");
    }

    #[test]
    fn panic_rejects_a_crate_name_and_says_why() {
        let e = validate_panic(Some("semihosting")).expect_err("must reject");
        assert!(
            e.contains("own"),
            "error points at the policy that admits it: {e}"
        );
    }
}

/// Resolve an image's `board` to a descriptor (phase-383 W1.e, RFC-0065 D9).
///
/// **Delegates to [`BoardCatalog::resolve_deploy`]; it does not re-implement
/// resolution.** That function is the single rule issue 0606 established after
/// three consumers — the site-config gate, `board-facts`, and the
/// standalone-leaf path — each grew a private opinion about what a board is
/// called. A fourth opinion here would be the same defect with a new name.
///
/// The one thing added is the ERROR: `resolve_deploy` answers `Unknown` /
/// `Ambiguous` without knowing which image asked, and a user needs the image id
/// to find the line to fix.
///
/// Note this is also why D9 needs no new registry field. A descriptor already
/// carries the downstream ecosystem's board id among its `names` — Zephyr's
/// `native_sim/native/64` sits beside `zephyr` in
/// `packages/boards/zephyr/nros-board.toml` — so both spellings already resolve
/// to one descriptor.
/// An image's `launch`, checked against the bringup's `launch/` directory.
///
/// phase-383 W10.a. W9.a wrote three images whose `launch` was a fragment of
/// PROSE — `launch = "…`"` in `workspaces/cpp`, `launch = "names"` in both
/// `realtime-c` bringups — and nothing noticed for two waves, because nothing
/// built from those declarations until the migration reached them. The macro
/// would have failed at expansion with a message about a missing model, several
/// layers from the typo.
///
/// Checked HERE because every consumer resolves an image through this module,
/// and the check is a `is_file()` against a name the author wrote.
///
/// `None`, and the conventional `default`, both mean "the bringup's own default
/// launch" and are always valid.
pub fn validate_image_launch(
    image_id: &str,
    image: &ImageBlock,
    bringup_dir: &std::path::Path,
) -> Result<(), String> {
    let Some(launch) = image.launch.as_deref() else {
        return Ok(());
    };
    if launch == "default" {
        return Ok(());
    }
    let path = bringup_dir.join("launch").join(launch);
    if path.is_file() {
        return Ok(());
    }
    let mut have: Vec<String> = std::fs::read_dir(bringup_dir.join("launch"))
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(".launch.xml"))
                .collect()
        })
        .unwrap_or_default();
    have.sort();
    let have = if have.is_empty() {
        "(none)".to_string()
    } else {
        have.join(", ")
    };
    Err(format!(
        "`[image.{image_id}] launch = \"{launch}\"` names no launch file: {} \
         does not exist. Available in this bringup: {have}. Drop the key to use \
         the bringup's default.",
        path.display()
    ))
}

/// The node packages an image's launch names, as written in the XML.
///
/// A `<node pkg="…" exec="…"/>` names its package; that set — not the
/// workspace — is the image's graph. Nested `<include>` is deliberately NOT
/// followed: the caller uses this to pick a DRIVER, and a mis-picked driver
/// fails loudly at configure with the package named, whereas a resolver that
/// silently walks includes would need the whole play_launch stack to run
/// before the first driver decision. When the file cannot be read the answer
/// is "nothing known", which the caller reads as "no evidence".
#[must_use]
pub fn launch_node_pkgs(image: &ImageBlock, bringup_dir: &std::path::Path) -> Vec<String> {
    let Some(launch) = image.launch.as_deref() else {
        return Vec::new();
    };
    let Ok(xml) = std::fs::read_to_string(bringup_dir.join("launch").join(launch)) else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    for rest in xml.split("pkg=").skip(1) {
        let mut it = rest.chars();
        let Some(quote) = it.next() else { continue };
        if quote != '"' && quote != '\'' {
            continue;
        }
        let value: String = it.take_while(|c| *c != quote).collect();
        if !value.is_empty() && !out.contains(&value) {
            out.push(value);
        }
    }
    out.sort();
    out
}

pub fn resolve_image_board<'c>(
    catalog: &'c BoardCatalog,
    image_id: &str,
    image: &ImageBlock,
) -> Result<&'c BoardDescriptor, String> {
    let Some(board) = image.board.as_deref() else {
        return Err(format!(
            "`[image.{image_id}]` declares no `board`. An image is a \
             (launch, args, board) bake; without a board there is nothing to \
             compile for."
        ));
    };
    match catalog.resolve_deploy(board) {
        DeployResolution::Board(d) => Ok(d),
        DeployResolution::Ambiguous(labels) => Err(format!(
            "`[image.{image_id}] board = \"{board}\"` is ambiguous — {} descriptors claim it: {}",
            labels.len(),
            labels.join(", ")
        )),
        DeployResolution::Unknown => {
            let mut known: Vec<&str> = catalog
                .descriptors()
                .iter()
                .flat_map(|d| d.names.iter().map(String::as_str))
                .collect();
            known.sort_unstable();
            known.dedup();
            Err(format!(
                "`[image.{image_id}] board = \"{board}\"` matches no board. \
                 Known boards: {}. Out-of-tree boards are added through \
                 `$NROS_EXTRA_BOARD_PATH`.",
                known.join(", ")
            ))
        }
    }
}

#[cfg(test)]
mod board_tests {
    use super::*;

    /// Local mirror of the descriptor file's shape — `BoardFile` is private,
    /// and widening production visibility for a test's convenience is the
    /// wrong trade.
    #[derive(serde::Deserialize)]
    struct BoardFile {
        #[serde(rename = "board")]
        boards: Vec<BoardDescriptor>,
    }

    const BOARDS: &str = r##"
[[board]]
names = ["zephyr", "native_sim/native/64"]
platform = "zephyr"
toolchain = "stable"
platform_feature = "platform-zephyr"
link_kind = "none"
entry_kind = "zephyr-staticlib"

[[board]]
names = ["mps2-an385-freertos"]
platform = "freertos"
toolchain = "stable"
platform_feature = "platform-freertos"
link_kind = "none"
entry_kind = "board-run"
"##;

    fn catalog() -> BoardCatalog {
        let f: BoardFile = toml::from_str(BOARDS).expect("parse");
        BoardCatalog::from_descriptors(f.boards)
    }

    fn image_with_board(b: &str) -> ImageBlock {
        ImageBlock {
            board: Some(b.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn resolves_a_nano_ros_board_id() {
        let cat = catalog();
        let d = resolve_image_board(&cat, "freertos", &image_with_board("mps2-an385-freertos"))
            .expect("resolves");
        assert_eq!(d.platform_feature, "platform-freertos");
    }

    #[test]
    fn resolves_the_framework_board_string_as_an_alias() {
        // D9 needs no new registry field: the descriptor already carries the
        // downstream ecosystem's id among its `names`.
        let cat = catalog();
        let d = resolve_image_board(&cat, "zephyr", &image_with_board("native_sim/native/64"))
            .expect("resolves");
        assert_eq!(d.platform_feature, "platform-zephyr");
    }

    #[test]
    fn both_spellings_reach_the_same_descriptor() {
        let cat = catalog();
        let a = resolve_image_board(&cat, "z", &image_with_board("zephyr")).expect("a");
        let b =
            resolve_image_board(&cat, "z", &image_with_board("native_sim/native/64")).expect("b");
        assert_eq!(a.platform_feature, b.platform_feature);
    }

    #[test]
    fn an_unknown_board_names_the_image_and_lists_what_is_known() {
        let cat = catalog();
        let e = resolve_image_board(&cat, "freertos", &image_with_board("mps2-an385-freertoss"))
            .expect_err("must reject");
        assert!(e.contains("[image.freertos]"), "names the image: {e}");
        assert!(e.contains("mps2-an385-freertos"), "lists known boards: {e}");
    }

    #[test]
    fn an_image_without_a_board_says_why_that_is_not_buildable() {
        let cat = catalog();
        let e = resolve_image_board(&cat, "native", &ImageBlock::default()).expect_err("reject");
        assert!(e.contains("declares no `board`"), "{e}");
    }
}

/// Fields on an upstream `[deploy.<id>]` that a nano-ros table now owns, each
/// paired with WHERE it went (phase-383 W1.f, issue 0951).
///
/// `kind`, `nodes` and `launch` are absent by design: those are PLACEMENT, they
/// stay upstream's, and they are not being retired.
///
/// The destination is part of the datum because it is not uniform, and a lint
/// that guesses one destination for every field sends people to a table with no
/// such key. `domain_id` and `locator` are RUNTIME facts about a machine and
/// live on `[host.<name>]`; `nros` is SITE config about a BOARD and lives on
/// `[board_config.<board>]`; only the rest are build description.
pub const DEPRECATED_DEPLOY_FIELDS: &[(&str, DeployFieldHome)] = &[
    ("board", DeployFieldHome::Image),
    ("target", DeployFieldHome::Image),
    ("rmw", DeployFieldHome::Image),
    ("profile", DeployFieldHome::Image),
    ("optimize", DeployFieldHome::Image),
    ("features", DeployFieldHome::Image),
    ("domain_id", DeployFieldHome::Host),
    ("locator", DeployFieldHome::Host),
    ("nros", DeployFieldHome::BoardConfig),
];

/// Which table a retired `[deploy.*]` field moved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeployFieldHome {
    /// `[image.<id>]` — build description, keyed by the same id.
    Image,
    /// `[host.<name>]` — a machine's runtime facts.
    Host,
    /// `[board_config.<board>]` — site config, keyed by BOARD.
    BoardConfig,
}

impl DeployFieldHome {
    fn destination(self, id: &str) -> String {
        match self {
            DeployFieldHome::Image => format!("`[image.{id}]`"),
            DeployFieldHome::Host => format!("`[host.{id}]`"),
            DeployFieldHome::BoardConfig => {
                "`[board_config.<board>]`, keyed by the BOARD rather than by \
                 this block's name"
                    .to_string()
            }
        }
    }

    /// Whether a same-named `[image.<id>]` means this field has already moved.
    ///
    /// Only true for fields whose home IS that image. A site block is keyed by
    /// board, so an image of the same name says nothing about whether the site
    /// config was migrated — skipping on it would silence the one warning that
    /// most needs to fire.
    fn satisfied_by_image(self) -> bool {
        matches!(self, DeployFieldHome::Image)
    }
}

/// One deprecation warning per `[deploy.<id>]` still carrying build fields with
/// no `[image.<id>]` beside it.
///
/// Follows phase-222's shipped pattern: warn on every invocation while still
/// working, `NROS_SUPPRESS_DEPRECATION=1` to opt out, removal at a VERSION
/// boundary rather than after a period.
///
/// Measured before writing this: across 63 `[deploy.*]` blocks in the tree, not
/// one uses `rmw`, `domain_id`, `locator`, `profile`, `optimize` or `features`.
/// Only `board` and `target` fire in practice, which is why this is a lint and
/// not a migration.
/// Suppression is a PARAMETER, not an env read inside the function: a lint that
/// consults ambient state cannot be tested deterministically when the suite
/// runs in one process. [`deprecation_suppressed`] is the one place that reads
/// the variable.
#[must_use]
pub fn deprecated_deploy_build_field_warnings(
    deploy_blocks: &BTreeMap<String, Vec<String>>,
    images: &BTreeMap<String, ImageBlock>,
    suppressed: bool,
) -> Vec<String> {
    if suppressed {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (id, present) in deploy_blocks {
        let has_image = images.contains_key(id);
        // Group by destination so one block yields one warning per table it
        // must migrate into, not one per field.
        let mut grouped: BTreeMap<String, Vec<&str>> = BTreeMap::new();
        for field in present.iter().map(String::as_str) {
            let Some((_, home)) = DEPRECATED_DEPLOY_FIELDS.iter().find(|(f, _)| *f == field) else {
                continue;
            };
            if has_image && home.satisfied_by_image() {
                continue;
            }
            grouped.entry(home.destination(id)).or_default().push(field);
        }
        for (destination, mut hits) in grouped {
            hits.sort_unstable();
            out.push(format!(
                "[deploy.{id}] carries {} — {} now owns {}. `[deploy.*]` keeps \
                 PLACEMENT (kind / nodes / launch) — that half is upstream's \
                 schema and is not being retired. Set \
                 NROS_SUPPRESS_DEPRECATION=1 to silence.",
                hits.join(", "),
                destination,
                if hits.len() == 1 { "it" } else { "them" },
            ));
        }
    }
    out
}

/// Whether the caller asked for deprecation warnings to be silenced.
/// phase-222's spelling, reused verbatim.
#[must_use]
pub fn deprecation_suppressed() -> bool {
    std::env::var_os("NROS_SUPPRESS_DEPRECATION").is_some()
}

#[cfg(test)]
mod deprecation_tests {
    use super::*;

    fn blocks(pairs: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
        pairs
            .iter()
            .map(|(id, fs)| {
                (
                    (*id).to_string(),
                    fs.iter().map(|f| (*f).to_string()).collect(),
                )
            })
            .collect()
    }

    /// The site block is keyed by BOARD, so a same-named `[image.<id>]` says
    /// NOTHING about whether it was migrated. Skipping on the image's presence
    /// would silence exactly the workspaces furthest along — the ones that have
    /// images for everything and an unmigrated site table underneath.
    #[test]
    fn a_site_block_still_warns_when_a_same_named_image_exists() {
        let mut images = BTreeMap::new();
        images.insert("freertos".to_string(), ImageBlock::default());
        let w = deprecated_deploy_build_field_warnings(
            &blocks(&[("freertos", &["board", "nros"])]),
            &images,
            false,
        );
        assert_eq!(w.len(), 1, "the image satisfies `board`, not `nros`: {w:?}");
        assert!(w[0].contains("nros"), "{}", w[0]);
        assert!(w[0].contains("board_config"), "{}", w[0]);
        assert!(
            !w[0].contains("[image.freertos]"),
            "must not send the reader to a table with no such key: {}",
            w[0]
        );
    }

    /// `domain_id` / `locator` are a machine's RUNTIME facts and live on
    /// `[host.*]`. The first version of this list called them build fields, so
    /// the warning named `[image.*]` — a table that has no such key.
    #[test]
    fn runtime_fields_point_at_the_host_table() {
        let w = deprecated_deploy_build_field_warnings(
            &blocks(&[("robot1", &["domain_id", "locator"])]),
            &BTreeMap::new(),
            false,
        );
        assert_eq!(w.len(), 1, "one destination, one warning: {w:?}");
        assert!(w[0].contains("[host.robot1]"), "{}", w[0]);
    }

    #[test]
    fn a_build_field_without_an_image_warns_once() {
        let w = deprecated_deploy_build_field_warnings(
            &blocks(&[("freertos", &["kind", "board"])]),
            &BTreeMap::new(),
            false,
        );
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("board"), "names the field: {}", w[0]);
        assert!(
            w[0].contains("[image.freertos]"),
            "names the replacement: {}",
            w[0]
        );
    }

    #[test]
    fn placement_only_blocks_never_warn() {
        // robot1/robot2 are the whole PLACEMENT-ONLY population in-tree, and
        // they must stay silent forever: their table is upstream's.
        let w = deprecated_deploy_build_field_warnings(
            &blocks(&[("robot1", &["kind", "nodes", "launch"])]),
            &BTreeMap::new(),
            false,
        );
        assert!(w.is_empty(), "placement must not warn: {w:?}");
    }

    #[test]
    fn a_migrated_block_stops_warning() {
        let mut images = BTreeMap::new();
        images.insert("freertos".to_string(), ImageBlock::default());
        let w = deprecated_deploy_build_field_warnings(
            &blocks(&[("freertos", &["kind", "board"])]),
            &images,
            false,
        );
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn suppression_silences_every_warning() {
        let w = deprecated_deploy_build_field_warnings(
            &blocks(&[("freertos", &["board"])]),
            &BTreeMap::new(),
            true,
        );
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn target_is_a_build_field_and_kind_is_not() {
        let field = |n: &str| DEPRECATED_DEPLOY_FIELDS.iter().find(|(f, _)| *f == n);
        assert_eq!(
            field("target").map(|(_, h)| *h),
            Some(DeployFieldHome::Image)
        );
        // Placement stays upstream's and is not being retired.
        assert!(field("kind").is_none());
        assert!(field("nodes").is_none());
        assert!(field("launch").is_none());
        // The two fields that do NOT move to the image — a warning naming
        // `[image.*]` for these would send the reader to a table with no such
        // key.
        assert_eq!(
            field("domain_id").map(|(_, h)| *h),
            Some(DeployFieldHome::Host)
        );
        assert_eq!(
            field("nros").map(|(_, h)| *h),
            Some(DeployFieldHome::BoardConfig)
        );
    }
    #[test]
    fn a_launch_that_names_no_file_is_refused_with_the_alternatives() {
        // phase-383 W10.a. W9.a wrote `launch = "…`"` and `launch = "names"` —
        // fragments of PROSE — into three shipped bringups, and they survived
        // two waves because nothing built from those declarations. The macro
        // would have failed at expansion, several layers from the typo.
        let tmp = tempfile::tempdir().unwrap();
        let bringup = tmp.path();
        std::fs::create_dir_all(bringup.join("launch")).unwrap();
        std::fs::write(bringup.join("launch/system.launch.xml"), "<launch/>").unwrap();

        let mut img = ImageBlock {
            board: Some("native".to_string()),
            ..Default::default()
        };

        // The real corruption, verbatim.
        img.launch = Some("…`".to_string());
        let e = validate_image_launch("native", &img, bringup).expect_err("refused");
        assert!(e.contains("names no launch file"), "{e}");
        assert!(
            e.contains("system.launch.xml"),
            "the message must list what IS available: {e}"
        );

        // A real one passes.
        img.launch = Some("system.launch.xml".to_string());
        assert!(validate_image_launch("native", &img, bringup).is_ok());

        // Both spellings of "the bringup's own default" are always valid.
        img.launch = Some("default".to_string());
        assert!(validate_image_launch("native", &img, bringup).is_ok());
        img.launch = None;
        assert!(validate_image_launch("native", &img, bringup).is_ok());
    }
}
