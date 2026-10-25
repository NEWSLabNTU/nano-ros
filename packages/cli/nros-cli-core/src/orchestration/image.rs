//! `[image.<id>]` — the buildable unit (RFC-0065 D6, phase-383 W1).
//!
//! **History:** the argument below is why `[image.*]` was introduced beside
//! `[deploy.*]`. Since phase-486 W8 `[deploy.*]` is retired outright and
//! refused by name: placement is `[host.<name>]`, the build is `[image.<id>]`.
//!
//! ## Why this is a NEW table and not a rename of `[deploy.*]`
//!
//! `[deploy.*]` was **not ours**. It is
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
    /// nano-ros board id — resolved through the board catalog
    /// (`packages/boards/**/nros-board.toml`, RFC-0065 D9). NEVER a
    /// framework's own board string: the descriptor carries that
    /// ([`BoardDescriptor::framework_board`] — `[board.zephyr] west_board`),
    /// so `native_sim/native/64` is a resolution RESULT, not something
    /// authored here. Authoring it is refused (issue 1519,
    /// [`refuse_framework_board`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board: Option<String>,

    /// Launch file this image bakes, relative to the bringup pkg. Absent ⇒ the
    /// system's `default_launch`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch: Option<String>,

    /// Launch arguments bound at resolve time — `{ host = "robot1" }`.
    ///
    /// This is how an image selects a MACHINE: `linux_robot1_entry` differs
    /// from `linux_robot2_entry` only here. Placement itself is
    /// `[host.*]`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, String>,

    /// Panic policy, forwarded verbatim to `nros::main!` — the EXISTING
    /// RFC-0077 enum (`platform` | `halt` | `own`), never a crate name.
    /// `own` means something else in the image carries the `#[panic_handler]`;
    /// a support package (RFC-0065 D12) is that slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panic: Option<String>,

    /// RFC-0065 D5 amendment (2026-10-24) — a function the generated entry
    /// calls FIRST, before the board boots: `before_boot = "my_pkg::open_can"`,
    /// typed `fn() -> Result<(), &'static str>`. Forwarded to `nros::main!`
    /// as a path. It is the declared escape for a pre-boot step, so an image
    /// needing one keeps a generated entry instead of materialising.
    /// Rust entries only; the C/C++ roads refuse it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_boot: Option<String>,

    /// RMW backend for this image. Absent ⇒ the system header's `rmw`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rmw: Option<String>,

    /// Deployment identity (RFC-0098 D5, phase-445 W3) — what this image dials
    /// and what it IS on the network. Per IMAGE because one board instance has
    /// one address: the esp32 talker and listener are the same board at
    /// `10.0.2.50` and `10.0.2.51`.
    ///
    /// `domain_id` / `locator` override `[system]`'s; `transport` overlays the
    /// board's compiled-in `Config::default()`. The address is
    /// [`Self::net`]. These replace the retiring
    /// `[package.metadata.nros.deploy.<board>]` table; the ONE reader is
    /// `nros_orchestration_ir::leaf_system`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<String>,
    /// RFC-0103 D10g (phase-497 W1) — `ip` / `gateway` / `netmask` are retired:
    /// accepted by the schema only so they are refused BY NAME, pointing at
    /// [`Self::net`], rather than as serde's generic "unknown field".
    #[serde(
        default,
        rename = "ip",
        deserialize_with = "refuse_flat_ip",
        skip_serializing
    )]
    pub retired_ip: super::cargo_metadata_schema::RetiredTable,
    #[serde(
        default,
        rename = "gateway",
        deserialize_with = "refuse_flat_gateway",
        skip_serializing
    )]
    pub retired_gateway: super::cargo_metadata_schema::RetiredTable,
    #[serde(
        default,
        rename = "netmask",
        deserialize_with = "refuse_flat_netmask",
        skip_serializing
    )]
    pub retired_netmask: super::cargo_metadata_schema::RetiredTable,
    /// `[image.<id>.net.*]` — RETIRED (RFC-0109 D4, phase-514 W4): refused by
    /// name, pointing at the board network file `os_config` names.
    #[serde(
        default,
        rename = "net",
        deserialize_with = "refuse_retired_net",
        skip_serializing
    )]
    pub retired_net: super::cargo_metadata_schema::RetiredTable,
    /// This image's address on each interface — RESOLVED, never authored here:
    /// read from the board network files of [`Self::os_config`] by
    /// [`super::cargo_metadata_schema::SystemToml::fold_os_config`].
    #[serde(skip)]
    pub net: BTreeMap<String, nros_orchestration_ir::system_net::ImageNetIf>,
    /// The `[network.<name>]` of those files, resolved the same way.
    #[serde(skip)]
    pub networks: BTreeMap<String, nros_orchestration_ir::system_net::NetworkDef>,
    /// `[image.<id>.run]` — how the image is run (RFC-0109 D6, phase-514 W6):
    /// read by the QEMU launchers and the test harness, never by the build.
    #[serde(default, skip_serializing_if = "is_default_run")]
    pub run: nros_orchestration_ir::system_net::ImageRun,
    /// RFC-0109 D4/D6 (phase-514 W2) — this image's layer-1 files, relative to
    /// its `system.toml`. A `.toml` one is the board package's network file,
    /// read into [`Self::net`] by
    /// [`super::cargo_metadata_schema::SystemToml::fold_os_config`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub os_config: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,

    /// Serialization format for this image, by PROVIDER NAME (phase-421 W4,
    /// RFC-0088 D6). Absent ⇒ the system header's `serdes`, then `cdr`.
    ///
    /// Per IMAGE and not per topic or per publisher: ROS 2's semantic is one
    /// middleware, one encoding, and RFC-0088 keeps it. The name is resolved
    /// against the provider search path — an in-tree format and one from a
    /// package outside this repo are selected the same way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serdes: Option<String>,

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
    /// Set it when that derivation cannot answer. Two cases, both once real
    /// in `examples/workspaces/realtime-cpp` (issue 1517):
    ///
    /// * AMBIGUOUS — two packages claim the board, so deriving is a coin flip
    ///   between right-looking answers and the resolver names the candidates
    ///   instead of picking one.
    /// * NOT FOUND — no package claims it. A Zephyr entry's `DEPLOY` token
    ///   names its PLATFORM, and the board it really targets comes from
    ///   `nano_ros_use_board(<board>)` in its `CMakeLists.txt`, which the
    ///   token scan does not read. `[image.fvp]` (board `fvp-aemv8r-smp`,
    ///   entry `fvp_entry`) was this one until issue 1288 generated it.
    ///
    /// Leaving it unset on Zephyr GENERATES the application from the row, so
    /// the two cannot disagree; only a hand-written application needs it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,

    /// `producers = false` — build this image WITHOUT the host board's
    /// build-time producers, the entity-census recorder and the profiler
    /// (`nros-board-linux/census` + `/profile`, issue 1419 / phase-463 W7).
    ///
    /// Unset (the default) keeps them: the host image is the census and
    /// profile producer for its workspace, and every other road relies on
    /// that. They are not free — `nros::metadata_mode::STATE` alone is
    /// ~76 KB of `.bss` — so an image whose own MEMORY is the measurement
    /// opts out (issue 1340's witness, `bins/in-place-subscriptions`). It is
    /// not a capability axis ([`Self::features`] only ADDS registry
    /// capabilities); it removes two crate features the selection writer
    /// otherwise adds by board, and has no effect on a board that has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producers: Option<bool>,

    /// Capability axes for this image, over the system's own list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,

    /// `env = { KEY = "VALUE" }` — the RFC-0049 APP rung for a build knob this
    /// IMAGE states (RFC-0098 D4, phase-445 W6).
    ///
    /// The rung that replaces the `[env]` block a leaf used to hand-write in
    /// its own `.cargo/config.toml`. The key is the knob's env front-end, so
    /// nothing needs a second name; the value lands in
    /// `build/<image>/nros-cargo.toml` WITHOUT `force`, so a lane that exports
    /// the variable still wins.
    ///
    /// Board facts belong in the board descriptor's `[knobs]`, not here:
    /// this is for a number that differs between two images of the SAME board.
    /// The ONE reader is `nros_orchestration_ir::leaf_system`, the same reader
    /// that answers for the deployment identity beside it.
    ///
    /// A row whose key is a REGISTRY knob is the deprecated spelling of
    /// [`Self::knobs`] (phase-484 W8): it works and warns. Rows the registry
    /// does not hold are this table's lasting use.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,

    /// `[image.<id>.knobs.<group>] <name> = <value>` — the same APP rung,
    /// spelled the way a board's `[knobs.<group>]` is (RFC-0103 D6, phase-484
    /// W8). Each key is a registry knob, rendered into the carrier [`Self::env`]
    /// rides (`cmd::leaf_settings::app_rung`); an `env` row naming a registry
    /// knob is the deprecated spelling of one of these.
    #[serde(default, skip_serializing_if = "ImageKnobs::is_empty")]
    pub knobs: ImageKnobs,
}

/// `[image.<id>.knobs]`, flattened to `<group>.<name>` → value by the one
/// reader both roads share (`nros_orchestration_ir::leaf_system::flatten_knobs`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImageKnobs(pub BTreeMap<String, nros_orchestration_ir::leaf_system::KnobValue>);

impl ImageKnobs {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Serialize for ImageKnobs {
    /// Back to the nested shape it was written in.
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use nros_orchestration_ir::leaf_system::KnobValue;
        let mut root = toml::Table::new();
        for (key, v) in &self.0 {
            let mut parts: Vec<&str> = key.split('.').collect();
            let leaf = parts.pop().unwrap_or_default();
            let mut t = &mut root;
            for p in parts {
                t = t
                    .entry(p.to_string())
                    .or_insert_with(|| toml::Value::Table(toml::Table::new()))
                    .as_table_mut()
                    .ok_or_else(|| serde::ser::Error::custom(format!("knob `{key}`")))?;
            }
            let value = match v {
                KnobValue::Int(n) => toml::Value::Integer(*n),
                KnobValue::Bool(b) => toml::Value::Boolean(*b),
                KnobValue::Str(x) => toml::Value::String(x.clone()),
            };
            t.insert(leaf.to_string(), value);
        }
        root.serialize(s)
    }
}

impl<'de> Deserialize<'de> for ImageKnobs {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let t = toml::Table::deserialize(d)?;
        nros_orchestration_ir::leaf_system::flatten_knobs(&t, "[image.<id>.knobs]")
            .map(ImageKnobs)
            .map_err(serde::de::Error::custom)
    }
}

fn refuse_flat<'de, D: serde::Deserializer<'de>>(
    key: &str,
) -> Result<super::cargo_metadata_schema::RetiredTable, D::Error> {
    Err(serde::de::Error::custom(
        nros_orchestration_ir::system_net::flat_key_refusal("image.<id>", key),
    ))
}
fn refuse_retired_net<'de, D: serde::Deserializer<'de>>(
    _: D,
) -> Result<super::cargo_metadata_schema::RetiredTable, D::Error> {
    Err(serde::de::Error::custom(
        nros_orchestration_ir::retired_tables::retired_image_key_message("image.<id>", "net")
            .unwrap_or_default(),
    ))
}

fn is_default_run(r: &nros_orchestration_ir::system_net::ImageRun) -> bool {
    *r == nros_orchestration_ir::system_net::ImageRun::default()
}

fn refuse_flat_ip<'de, D: serde::Deserializer<'de>>(
    _: D,
) -> Result<super::cargo_metadata_schema::RetiredTable, D::Error> {
    refuse_flat::<D>("ip")
}
fn refuse_flat_gateway<'de, D: serde::Deserializer<'de>>(
    _: D,
) -> Result<super::cargo_metadata_schema::RetiredTable, D::Error> {
    refuse_flat::<D>("gateway")
}
fn refuse_flat_netmask<'de, D: serde::Deserializer<'de>>(
    _: D,
) -> Result<super::cargo_metadata_schema::RetiredTable, D::Error> {
    refuse_flat::<D>("netmask")
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
            before_boot: pick(&self.before_boot, &base.before_boot),
            rmw: pick(&self.rmw, &base.rmw),
            domain_id: self.domain_id.or(base.domain_id),
            locator: pick(&self.locator, &base.locator),
            retired_ip: Default::default(),
            retired_gateway: Default::default(),
            retired_netmask: Default::default(),
            retired_net: Default::default(),
            // Resolved from `os_config`'s files, so they follow it: the image's
            // own when it names files, else the base's.
            net: if self.os_config.is_empty() {
                base.net.clone()
            } else {
                self.net.clone()
            },
            networks: if self.os_config.is_empty() {
                base.networks.clone()
            } else {
                self.networks.clone()
            },
            run: nros_orchestration_ir::system_net::ImageRun {
                nic: self.run.nic.clone().or_else(|| base.run.nic.clone()),
            },
            // A list the image states replaces the base's whole, like a scalar.
            os_config: if self.os_config.is_empty() {
                base.os_config.clone()
            } else {
                self.os_config.clone()
            },
            transport: pick(&self.transport, &base.transport),
            serdes: pick(&self.serdes, &base.serdes),
            ros_edition: pick(&self.ros_edition, &base.ros_edition),
            profile: pick(&self.profile, &base.profile),
            variant: pick(&self.variant, &base.variant),
            // Scalar, so it replaces: a base naming an entry is a default for
            // the images that do not name one, and an image that does is
            // making a choice the base cannot have made for it.
            entry: pick(&self.entry, &base.entry),
            producers: self.producers.or(base.producers),
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
            // Map, so it MERGES like `args`: a base `env` is a default knob
            // SET, and an image that names a key it also names is overriding
            // that one knob, not replacing the table.
            env: {
                let mut merged = base.env.clone();
                merged.extend(self.env.clone());
                merged
            },
            // The same overlay, for the same reason: a base knob is a default.
            knobs: {
                let mut merged = base.knobs.0.clone();
                merged.extend(self.knobs.0.clone());
                ImageKnobs(merged)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> ImageBlock {
        toml::from_str(src).expect("parses")
    }

    /// phase-445 W6 - the APP rung parses, and MERGES like `args` rather than
    /// replacing: a base `env` is a default knob set.
    #[test]
    fn the_app_rung_parses_and_merges_key_by_key() {
        let specific = parse("env = { A = \"image\", C = \"image\" }\n");
        let base = parse("env = { A = \"base\", B = \"base\" }\n");
        let merged = specific.with_base(&base);
        assert_eq!(merged.env["A"], "image", "the image wins the key it names");
        assert_eq!(merged.env["B"], "base", "and inherits the one it does not");
        assert_eq!(merged.env["C"], "image");
        // Absent is an EMPTY table, never a missing field: a caller extends it
        // unconditionally.
        assert!(parse("board = \"linux\"\n").env.is_empty());
    }

    /// phase-484 W8 (RFC-0103 D6) — a workspace image's `[image.<id>.knobs]`
    /// parses nested, merges over the base key by key like `env`, serializes
    /// back to the shape it was written in, and renders to the SAME rows the
    /// env spelling states.
    #[test]
    fn workspace_image_knobs_merge_and_render_like_env_rows() {
        let base = parse("[knobs.executor]\nmax_cbs = 8\n[knobs.zephyr]\nheap_size = 1\n");
        let specific =
            parse("[knobs.zephyr]\nheap_size = 262144\n[knobs.zenoh.tx]\nbatch = true\n");
        let merged = specific.with_base(&base);
        let rows = crate::cmd::leaf_settings::image_block_layers(None, &merged).unwrap();
        // The rows the retired env spelling of these knobs stated.
        let env: std::collections::BTreeMap<String, String> = [
            ("NROS_EXECUTOR_MAX_CBS", "8"),
            ("NROS_ZEPHYR_HEAP_SIZE", "262144"),
            ("NROS_ZENOH_TX_BATCH", "1"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        assert_eq!(rows, env);
        let back: ImageBlock = toml::from_str(&toml::to_string(&merged).unwrap()).unwrap();
        assert_eq!(back, merged, "serializes to the nested shape it parses");
        assert!(toml::from_str::<ImageBlock>("[knobs]\nmax_cbs = 1\n").is_err());
    }

    /// Issue 1340 — `producers` parses, is unset by default, and a specific
    /// block's value wins over the base table's.
    #[test]
    fn producers_is_unset_by_default_and_the_image_wins() {
        assert_eq!(parse("board = \"linux\"\n").producers, None);
        let off = parse("producers = false\n");
        assert_eq!(off.producers, Some(false));
        assert_eq!(
            ImageBlock::default().with_base(&off).producers,
            Some(false),
            "a base `producers = false` reaches an image that says nothing"
        );
        assert_eq!(
            parse("producers = true\n").with_base(&off).producers,
            Some(true)
        );
        assert!(toml::from_str::<ImageBlock>("producer = false\n").is_err());
    }

    /// `deny_unknown_fields` is why the key had to be added here at all - a
    /// `system.toml` carrying it would otherwise be REFUSED by `nros sync`,
    /// not silently ignored.
    #[test]
    fn a_mistyped_app_rung_key_is_still_refused() {
        assert!(toml::from_str::<ImageBlock>("envv = { A = \"x\" }\n").is_err());
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

/// Validate an image's `before_boot` as a Rust path to a function in a crate.
///
/// Checked here so a typo is reported against `system.toml` rather than as a
/// macro error inside a generated file. At least two segments: the function
/// lives in a package the entry depends on (a node package of the launch), so
/// a bare name has nowhere to resolve.
pub fn validate_before_boot(path: Option<&str>) -> Result<(), String> {
    let Some(p) = path else { return Ok(()) };
    let ident = |s: &str| {
        let mut c = s.chars();
        c.next()
            .is_some_and(|f| f == '_' || f.is_ascii_alphabetic())
            && c.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
    };
    let segments: Vec<&str> = p.split("::").collect();
    if segments.len() >= 2 && segments.iter().all(|s| ident(s)) {
        Ok(())
    } else {
        Err(format!(
            "`before_boot = \"{p}\"` is not a path to a function in a package \
             (expected `<package>::<fn>`, e.g. `\"my_pkg::open_can\"`)"
        ))
    }
}

#[cfg(test)]
mod before_boot_tests {
    use super::validate_before_boot;

    #[test]
    fn before_boot_must_name_a_function_in_a_package() {
        assert!(validate_before_boot(None).is_ok());
        assert!(validate_before_boot(Some("my_pkg::open_can")).is_ok());
        assert!(validate_before_boot(Some("my_pkg::hw::open_can")).is_ok());
        for bad in [
            "open_can",
            "my-pkg::open_can",
            "my_pkg::",
            "::open_can",
            "my_pkg::open can",
        ] {
            let e = validate_before_boot(Some(bad)).unwrap_err();
            assert!(e.contains("<package>::<fn>"), "{bad}: {e}");
        }
    }
}

/// Validate an image's `transport` against the link kinds
/// (`nros_orchestration_ir::leaf_system::TRANSPORT_KINDS`).
///
/// phase-445 W6. The leaf road already refuses an unknown value — that reader
/// is `leaf_system`, the one RFC-0098 D5 names — and this is the WORKSPACE
/// road's half of it. Two readers of one file with one of them validating is
/// how `examples/mps2-an385-baremetal/rust/talker-xrce` carried
/// `transport = "xrce"` (the RMW name, in the slot that names the link) for
/// four phases with nothing to say so.
///
/// The value is load-bearing now: it IMPLIES `ZPICO_NO_SMOLTCP` and
/// `NROS_LINK_IP`, so a typo would leave both unapplied and the image would
/// build with the wrong links on — RFC-0086 D2's own reason for making an
/// unknown transport an error rather than a pass-through.
pub fn validate_transport(kind: Option<&str>) -> Result<(), String> {
    use nros_orchestration_ir::leaf_system::TRANSPORT_KINDS;
    match kind {
        None => Ok(()),
        Some(k) if TRANSPORT_KINDS.contains(&k) => Ok(()),
        Some(other) => Err(format!(
            "`transport = \"{other}\"` is not a link kind (expected one of: {}). \
             The RMW is `rmw = \"…\"`, which is a different choice.",
            TRANSPORT_KINDS.join(" | ")
        )),
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;

    #[test]
    fn a_link_kind_passes_and_an_rmw_name_does_not() {
        assert!(validate_transport(None).is_ok());
        assert!(validate_transport(Some("serial")).is_ok());
        let e = validate_transport(Some("xrce")).unwrap_err();
        assert!(e.contains("serial | tcp | udp"), "{e}");
        assert!(e.contains("rmw"), "names the key that DOES take it: {e}");
    }

    /// RFC-0109 D3 — one link vocabulary: every kind an image may name is a
    /// kind a board's `[board.net] links` may carry.
    #[test]
    fn image_transport_kinds_are_board_link_kinds() {
        use nros_board_common::platform_config::LINK_KINDS;
        use nros_orchestration_ir::leaf_system::TRANSPORT_KINDS;
        for k in TRANSPORT_KINDS {
            assert!(
                LINK_KINDS.contains(k),
                "image transport `{k}` is not in LINK_KINDS"
            );
        }
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
        let got = select_default_images(&images(&["linux"]), &[]).expect("ok");
        assert_eq!(got, ImageSelection::Images(vec!["linux".to_string()]));
    }

    #[test]
    fn several_images_without_a_default_are_ambiguous() {
        // RFC-0065 D1 — list and fail, never guess. `examples/workspaces/rust`
        // declares eight images across three cross toolchains.
        let got =
            select_default_images(&images(&["linux", "freertos", "zephyr"]), &[]).expect("ok");
        assert_eq!(
            got,
            ImageSelection::Ambiguous(vec![
                "freertos".to_string(),
                "linux".to_string(),
                "zephyr".to_string()
            ])
        );
    }

    #[test]
    fn default_images_disambiguates() {
        let got = select_default_images(&images(&["linux", "freertos"]), &["linux".to_string()])
            .expect("ok");
        assert_eq!(got, ImageSelection::Images(vec!["linux".to_string()]));
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
        let e = select_default_images(&images(&["linux"]), &["linu".to_string()])
            .expect_err("must reject");
        assert!(e.contains("linu"), "error names the typo: {e}");
        assert!(e.contains("linux"), "error lists what IS declared: {e}");
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
/// One rule is added on top, and it is the field's own: an image authors the
/// NANO-ROS id, never the framework's (issue 1519). A descriptor's framework
/// id still resolves — `[board.zephyr] west_board` answers through
/// [`BoardDescriptor::answers_to`], because a `[deploy.*].board` legitimately
/// carries it (issue 0606) — so an image spelling it would build. It is
/// refused here instead, naming the id to write
/// ([`refuse_framework_board`]).
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
    let origin = format!("[image.{image_id}] board");
    let d = resolve_board_id(catalog, &origin, board)?;
    refuse_framework_board(&origin, board, d)?;
    Ok(d)
}

/// Issue 1519 — an `[image.*] board` that spells the FRAMEWORK's board id
/// (`native_sim/native/64`, `mps2_an385`, …) rather than the nano-ros one.
///
/// [`ImageBlock::board`] says never to author it, and for a year the tree did
/// anyway: 13 rows wrote `native_sim/native/64`, and they reached the right
/// `west build -b` only because the `zephyr` descriptor carried that string as
/// a second NAME. The other 24 Zephyr rows wrote `zephyr` — both readings of
/// one rule, live at once, with the forbidden one the one that worked.
///
/// So the id is resolved first (an unknown board keeps its own, more useful
/// error), and only then compared with what the descriptor states as its
/// framework id ([`BoardDescriptor::framework_board`]). A descriptor whose ONLY
/// name is its framework id has nothing better to offer, so it is accepted —
/// refusing would leave no legal spelling.
///
/// Applies to images only. `[deploy.*].board` carries the downstream id by
/// design (issue 0606).
pub fn refuse_framework_board(
    origin: &str,
    board: &str,
    descriptor: &BoardDescriptor,
) -> Result<(), String> {
    if descriptor.framework_board() != Some(board) {
        return Ok(());
    }
    let Some(id) = descriptor.names.iter().find(|n| n.as_str() != board) else {
        return Ok(());
    };
    Err(format!(
        "`{origin} = \"{board}\"` is the FRAMEWORK's board id, not a nano-ros one. \
         Write `board = \"{id}\"`: an image names the nano-ros board, and the \
         descriptor ({}) states `{board}` as the framework's own id for it, so \
         the build is unchanged (issues 1519, 1652).",
        descriptor
            .source
            .as_deref()
            .unwrap_or("in-memory descriptor")
    ))
}

/// Resolve a board id through the catalog's ONE rule
/// ([`BoardCatalog::resolve_deploy`]). `origin` names where the id was
/// written (`[image.fw] board`), so a failure points at the line to fix.
///
/// An id no descriptor claims is an error that names every known board. So
/// is an id several descriptors claim. Never a guess.
///
/// Split out of [`resolve_image_board`] (issue 1285 follow-up) so that
/// `codegen-system`'s tier RTOS reads an image board the same way `nros build`
/// resolves it, rather than from a substring of the id.
pub fn resolve_board_id<'c>(
    catalog: &'c BoardCatalog,
    origin: &str,
    board: &str,
) -> Result<&'c BoardDescriptor, String> {
    match catalog.resolve_deploy(board) {
        DeployResolution::Board(d) => Ok(d),
        DeployResolution::Ambiguous(labels) => Err(format!(
            "`{origin} = \"{board}\"` is ambiguous — {} descriptors claim it: {}",
            labels.len(),
            labels.join(", ")
        )),
        DeployResolution::Unknown => {
            // phase-500 W1 — a retired key names its replacement, not a list.
            if let Some(hint) = nros_entry_lower::retired_board_key_hint(board) {
                return Err(format!("`{origin} = \"{board}\"`: {hint}"));
            }
            let mut known: Vec<&str> = catalog
                .descriptors()
                .iter()
                .flat_map(|d| d.names.iter().map(String::as_str))
                .collect();
            known.sort_unstable();
            known.dedup();
            Err(format!(
                "`{origin} = \"{board}\"` matches no board. \
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
names = ["zephyr"]
platform = "zephyr"
toolchain = "stable"
platform_feature = "platform-zephyr"
link_kind = "none"
entry_kind = "zephyr-staticlib"

[board.zephyr]
west_board = "native_sim/native/64"

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

    /// Issue 1519 — the framework id RESOLVES (a `[deploy.*].board` needs it
    /// to), but an image may not author it, and the refusal names the id to
    /// write instead.
    #[test]
    fn an_image_authoring_the_framework_board_string_is_refused() {
        let cat = catalog();
        let e = resolve_image_board(&cat, "zephyr", &image_with_board("native_sim/native/64"))
            .expect_err("the framework id is not an image board");
        assert!(e.contains("[image.zephyr] board"), "names the line: {e}");
        assert!(e.contains("board = \"zephyr\""), "names the fix: {e}");
        assert!(e.contains("1519"), "{e}");
        // The SAME string still resolves for a deploy (issue 0606).
        assert!(matches!(
            cat.resolve_deploy("native_sim/native/64"),
            DeployResolution::Board(_)
        ));
    }

    /// The nano-ros id reaches the same `-b` the framework id used to author.
    #[test]
    fn the_nano_ros_id_reaches_the_framework_board() {
        let cat = catalog();
        let d = resolve_image_board(&cat, "z", &image_with_board("zephyr")).expect("resolves");
        assert_eq!(d.west_build_board("zephyr"), "native_sim/native/64");
    }

    /// A descriptor whose only name IS its framework id has no better spelling
    /// to offer, so refusing would leave none.
    #[test]
    fn a_framework_id_that_is_the_only_name_is_accepted() {
        let f: BoardFile = toml::from_str(
            "[[board]]\nnames = [\"mps2_an385\"]\nplatform = \"zephyr\"\n\
             toolchain = \"stable\"\nentry_kind = \"zephyr-staticlib\"\n\
             [board.zephyr]\nwest_board = \"mps2_an385\"\n",
        )
        .expect("parse");
        let cat = BoardCatalog::from_descriptors(f.boards);
        resolve_image_board(&cat, "z", &image_with_board("mps2_an385")).expect("accepted");
    }

    #[test]
    fn an_unknown_board_names_the_image_and_lists_what_is_known() {
        let cat = catalog();
        let e = resolve_image_board(&cat, "freertos", &image_with_board("mps2-an385-freertoss"))
            .expect_err("must reject");
        assert!(e.contains("[image.freertos]"), "names the image: {e}");
        assert!(e.contains("mps2-an385-freertos"), "lists known boards: {e}");
    }

    /// phase-500 W1/W5 — neither `native` (W1) nor `posix` (W5: the
    /// platform, which the catalog's platform fallback once answered with the
    /// host board) names a board; the refusal names the replacement rather
    /// than listing every known board.
    #[test]
    fn a_retired_host_board_key_is_refused_naming_linux() {
        let cat = catalog();
        for key in ["native", "posix"] {
            let e = resolve_image_board(&cat, "linux", &image_with_board(key))
                .expect_err("must reject");
            assert!(
                e.contains("[image.linux] board"),
                "{key}: names the line: {e}"
            );
            assert!(
                e.contains("write `linux`"),
                "{key}: names the replacement: {e}"
            );
            assert!(
                !e.contains("Known boards"),
                "{key}: a hint, not a list: {e}"
            );
        }
    }

    #[test]
    fn an_image_without_a_board_says_why_that_is_not_buildable() {
        let cat = catalog();
        let e = resolve_image_board(&cat, "linux", &ImageBlock::default()).expect_err("reject");
        assert!(e.contains("declares no `board`"), "{e}");
    }
}

/// Whether the caller asked for deprecation warnings to be silenced.
/// phase-222's spelling, reused verbatim.
#[must_use]
pub fn deprecation_suppressed() -> bool {
    std::env::var_os("NROS_SUPPRESS_DEPRECATION").is_some()
}

#[cfg(test)]
mod launch_tests {
    use super::*;

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
        let e = validate_image_launch("linux", &img, bringup).expect_err("refused");
        assert!(e.contains("names no launch file"), "{e}");
        assert!(
            e.contains("system.launch.xml"),
            "the message must list what IS available: {e}"
        );

        // A real one passes.
        img.launch = Some("system.launch.xml".to_string());
        assert!(validate_image_launch("linux", &img, bringup).is_ok());

        // Both spellings of "the bringup's own default" are always valid.
        img.launch = Some("default".to_string());
        assert!(validate_image_launch("linux", &img, bringup).is_ok());
        img.launch = None;
        assert!(validate_image_launch("linux", &img, bringup).is_ok());
    }
}
