//! Subcommand dispatch surface.
//!
//! Each verb lives in its own submodule and exposes:
//!   * a clap `Args` struct (when the verb takes options)
//!   * a `run(args) -> Result<()>` function
//!
//! `Cmd` is the clap-derived enum the binary front-ends parse into;
//! [`crate::run`] dispatches it.

use clap::Subcommand;

pub mod board;
pub mod board_facts;
pub mod board_new;
pub mod board_support;
pub mod bringup;
pub mod build;
pub mod build_next;
pub mod check;
pub mod check_workspace;
pub mod codegen;
pub mod codegen_cyclonedds_descriptors;
pub mod codegen_system;
pub mod completions;
pub mod config;
pub mod dev;
pub mod doctor;
pub mod emit_package_xml;
pub mod entity_census;
pub mod entity_inventory;
pub mod explain;
pub mod generate;
pub mod generate_px4;
pub mod graph;
pub mod image_facts;
pub mod init;
pub mod interface;
pub mod leaf_kconfig;
pub mod leaf_settings;
pub mod leaf_system;
pub mod locate;
pub mod managed_deps;
pub mod materialize;
pub mod message_bound_knobs;
pub mod metadata;
pub mod model_path;
pub mod new;
pub mod new_entry;
pub mod new_node;
pub mod new_platform;
pub mod new_system;
pub mod pin;
pub mod plan;
pub mod profile;
pub mod reserved;
pub mod run_plan;
pub mod scaffold_deploy;
pub mod sdk_front;
pub mod sdk_path;
pub mod sdk_root;
pub mod setup;
/// phase-440 W6 — `nros store list|gc` (RFC-0095 D11).
pub mod sizing_descriptor;
pub mod store;
/// phase-440 W6 — `nros toolchain uninstall` (RFC-0095 D11).
pub mod toolchain;
pub mod update;
pub mod user;
pub mod version;
/// Issue 1662 — `nros ws west-leaf-sizing`.
pub mod west_leaf_sizing;
pub mod ws;

#[cfg(feature = "release")]
pub mod release;

#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// Build a nano-ros workspace.
    ///
    /// Discovers packages, resolves the image, runs preflight, generates the
    /// root, and hands off to the native tool. The final stage is an `exec`,
    /// so compiler diagnostics are byte-identical to running cargo/cmake/west
    /// directly.
    // RFC-0065 / phase-383.
    Build(build::Args),

    /// Take ownership of a generated entry package.
    ///
    /// The last resort: `panic` and `profile` are declarations on the image and
    /// need no materialising. One way — `nros build` stops regenerating it.
    // RFC-0065 D5.
    Materialize(materialize::Args),

    /// Scaffold a new nano-ros package (`--template`: talker, listener, server,
    /// client, action-server, action-client), workspace, image or component
    New(Box<new::Args>),

    /// Generate Rust / C / C++ message bindings from `package.xml`.
    // RFC-0117 D5: the old spelling of `nros interface generate`.
    #[command(hide = true)]
    Generate(generate::Args),

    /// Generate message bindings, list the interfaces the workspace can
    /// reach, or show one interface's definition.
    #[command(subcommand)]
    Interface(interface::Args),

    /// List the workspace's packages: kind, name and directory per row.
    List(ws::ListArgs),

    /// Print the workspace's packages in dependency order, each with the
    /// workspace packages it depends on.
    Graph(graph::Args),

    /// Remove the generated bindings and the managed patch block from each
    /// Rust package. Leaves user-written sections alone.
    Clean(ws::CleanArgs),

    /// Run an image on its target: a host process or an emulator
    /// (RFC-0118 D2). `--dry-run [--why]` prints the resolved RunPlan;
    /// launching it is phase-528 W4. A hardware image is `nros flash
    /// --monitor`.
    Run(run_plan::Args),

    /// Flash an image onto a board (reserved: not available yet).
    Flash(reserved::Args),

    /// Attach to a running board's console (reserved: not available yet).
    Monitor(reserved::Args),

    /// Run an image on a bench of boards (reserved: not available yet).
    Bench(reserved::Args),

    /// Debug an image on its target (reserved: not available yet).
    Debug(reserved::Args),

    /// Generate Rust message bindings from `package.xml`
    #[command(name = "generate-rust", hide = true)]
    GenerateRust(generate::RustArgs),

    /// Print this binary's codegen fingerprint.
    ///
    /// The fingerprint is a hash of the bytes its emitters produce for a
    /// compiled-in probe corpus. The fixture-freshness signature keys on this
    /// instead of the binary's own hash, so a rebuild that changes no emitted
    /// byte invalidates no fixture.
    /// Hidden: it is a build-system seam, not a user verb.
    // RFC-0061 / phase-318 W1.
    #[command(name = "codegen-fingerprint", hide = true)]
    CodegenFingerprint,

    /// Report whether this binary matches the CLI sources in its checkout.
    ///
    /// Exits non-zero when stale. A stale binary can still run this verb.
    // Issue 0363. Hidden for the same reason as `codegen-fingerprint`: a
    // build-system seam. It exists so `scripts/check-cli-fresh.sh` can ASK the
    // binary rather than reimplement the predicate — the freshness logic lives
    // in exactly one place, and this is its read-only door.
    //
    // Deliberately NOT in the guarded set: a stale binary must still be able
    // to report that it is stale.
    #[command(name = "source-stamp", hide = true)]
    SourceStamp,

    /// Sync generated message bindings and the cargo patch config to the declared deps.
    ///
    /// Reads `package.xml` / `Cargo.toml`, for a standalone package or a
    /// workspace (single-package vs colcon mode is picked by layout). Writes
    /// the `[patch.crates-io]` patch into each Rust consumer's
    /// `.cargo/config.toml` (never `Cargo.toml`). Pre-cargo step; run after
    /// editing `*.msg` files.
    // Phase-265 W5 — promoted from the retired `ws`-scoped `sync` alias.
    // (`nros generate-rust` stays the codegen-only primitive.)
    Sync(ws::SyncArgs),

    /// Generate CDR `px4_msgs::msg::*` from a PX4-Autopilot `.msg` tree.
    ///
    /// Needs no ament dependency. Serves the XRCE companion path.
    // Phase 233 / RFC-0039 Track B.
    #[command(name = "generate-px4-msgs")]
    GeneratePx4Msgs(generate_px4::Args),

    /// Build-tool C/C++ binding generation (`--args-file` / `resolve-deps`).
    ///
    /// The interface the cmake / build.rs consumers speak.
    // Phase 195.A — folds in the former standalone `nros-codegen` binary.
    #[command(hide = true)]
    Codegen(codegen::Args),

    /// Host-time system bake.
    ///
    /// Reads `<bringup>/system.toml` + `<bringup>/launch/system.launch.xml`
    /// and emits the baked compile-time C config + component registration glue
    /// consumed by every embedded RTOS adapter.
    // Phase 212.E.
    #[command(name = "codegen-system", hide = true)]
    CodegenSystem(codegen_system::Args),

    /// Print the resolved image as data.
    ///
    /// A west build derives its cargo invocation from `[image.*]` through this
    /// instead of re-spelling it from Kconfig. A query, not a build: it cannot
    /// recurse into `west build`.
    // RFC-0085 D2.
    #[command(name = "image-facts", hide = true)]
    ImageFacts(image_facts::Args),

    /// Print the resolved SystemModel path for (bringup, launch, args).
    ///
    /// The cmake bridge for `nano_ros_entry(LAUNCH …)`.
    // phase-330 W7.
    #[command(name = "model-path", hide = true)]
    ModelPath(model_path::Args),

    /// Print the constructed install path of a provisioned tool.
    ///
    /// The cmake/just/shell bridge to `sdk_store::tool_dir`, so the store is
    /// never searched.
    // phase-365 W2; never searching the store is issue 0625.
    #[command(name = "sdk-path", hide = true)]
    SdkPath(sdk_path::Args),

    /// Print where a resource is, and with `--why`, which rung answered.
    ///
    /// The cmake/just/shell front door to `nros_build_paths::locate`.
    // phase-484 W1 (RFC-0103 D4).
    Locate(locate::Args),

    /// Point `$NROS_HOME/bin/<name>` at the newest installed version of a tool.
    ///
    /// The bridge for `scripts/bootstrap.sh`, which unpacks the CLI itself and
    /// has no `nros setup` to run until it has.
    // phase-431 W3.
    #[command(name = "sdk-front", hide = true)]
    SdkFront(sdk_front::Args),

    /// Print the nano-ros SDK root this toolchain resolves.
    ///
    /// The cmake/shell bridge to the four-rung ladder, so a scaffolded project
    /// asks where the runtime is instead of baking in an absolute path that is
    /// right on exactly one machine.
    // phase-447 A2 (RFC-0099 D3).
    #[command(name = "sdk-root", hide = true)]
    SdkRoot(sdk_root::Args),

    /// Inspect and shrink the SDK store.
    // phase-440 W6 (RFC-0095 D11). The store is additive by design, so `list`
    // says what is in it and `gc` proposes what to reclaim — dry-run by
    // default, and it never removes anything a pin names. RFC-0117 D5: the
    // old spelling of `nros toolchain list|gc`.
    #[command(hide = true)]
    Store(store::Args),

    /// List, remove and garbage-collect installed toolchains and SDK packages.
    // phase-440 W6 (RFC-0095 D11); `list` and `gc` absorbed `nros store`
    // (RFC-0117 D2).
    Toolchain(toolchain::Args),

    /// Report this project's `nros-toolchain.toml`, or move it.
    ///
    /// A bump reports the codegen delta between the two toolchains' declared
    /// `share/nros/manifest.toml` first, so an upgrade that would invalidate
    /// already-generated code warns before anything is written.
    // phase-443 W2; the codegen-delta warning is RFC-0097 D7.
    Pin(pin::Args),

    /// Move one pin, editing the pin set and leaving one diff.
    ///
    /// A pin is a registry crate (`--crate`), a vendored source (`--source`), a
    /// host tool (`--tool`) or a west project (`--west`).
    // phase-485 W7 (RFC-0104 D7).
    Update(update::Args),

    /// Print the cargo build-profile table.
    ///
    /// The table holds the CMAKE_BUILD_TYPE mapping, flags, artifact dir and
    /// env-injected definitions. The bridge cmake/bash use so the derivations
    /// are not re-spelled per language.
    // phase-336.
    #[command(subcommand_negates_reqs = true, hide = true)]
    Profile(profile::Args),

    /// Collect component source metadata for orchestration planning
    #[command(hide = true)]
    Metadata(metadata::Args),

    /// Resolve launch files, manifests, and metadata into nros-plan.json
    Plan(plan::Args),

    /// Validate a generated nros-plan.json
    Check(check::Args),

    /// Render a generated nros-plan.json in human-readable form
    Explain(explain::Args),

    /// Inspect or validate the current project's resolved configuration
    #[command(subcommand)]
    Config(config::Args),

    /// Resolve + fetch a board's toolchain/SDK packages
    // Phase 187.
    Setup(setup::Args),

    /// Generate a project `CMakePresets.json` including the per-board presets
    /// `nros setup` wrote. Then `cmake --preset <board>` works.
    // RFC-0048 §6. RFC-0117 D5: the old spelling of `nros setup presets`.
    #[command(hide = true)]
    Init(init::Args),

    /// Health-check the workspace (SDK paths, toolchains, env)
    Doctor(doctor::Args),

    /// Inspect supported boards
    #[command(subcommand)]
    Board(board::Args),

    /// Workspace-level message-package utilities: env, list, status, clean, doctor.
    // Phase 210.B.3 + 210.D.1. `sync` was promoted to the top-level `nros
    // sync`. RFC-0117 D2: hidden. `list` and `clean` are top-level user verbs now,
    // and the build-system verbs live under `nros dev`.
    #[command(hide = true)]
    Ws(ws::Args),

    /// Print toolchain + library versions
    Version,

    /// Generate shell completions (bash | zsh | fish | powershell)
    Completions(completions::Args),

    /// The developer and build-system tier: the verbs cmake, west, `just` and the gates call.
    ///
    /// Hidden from `nros --help`; `nros help --all` lists it.
    // RFC-0117.
    #[command(hide = true)]
    Dev(dev::Args),

    /// Maintainer-only release subcommands (hidden unless built with
    /// `--features release`)
    #[cfg(feature = "release")]
    #[command(subcommand)]
    Release(release::Args),
}

// #186 — the hidden `nros migrate workspace` one-shot (Phase 212.I) is
// retired: its emitter never adopted the post-212.I component sub-table
// spec and pre-212 workspaces have aged out. A tree that still needs the
// migration runs it from the `nros-v0.5.0` tag's CLI.

/// Lower an alias spelling into the command its canonical handler takes.
///
/// A `nros dev` command lowers to the command its old spelling parses to
/// ([`dev::lower`]); a user-tier verb that absorbed an older one lowers the
/// same way ([`user::lower`]). Every caller, the dispatch and the tests alike,
/// goes through this one function, so two spellings cannot reach different
/// code.
pub fn lower(cmd: Cmd) -> Cmd {
    match cmd {
        Cmd::Dev(a) => dev::lower(a),
        other => user::lower(other),
    }
}

/// Every alias the CLI resolves, both tiers, as JSON for
/// `nros help --aliases --json`: one object per old spelling,
/// `{"old", "new", "tier"}`. `tier` is `"dev"` for a silent alias and
/// `"user"` for one that prints a note on a terminal (RFC-0117 D5).
/// `check-cli-old-spellings` reads this, so the gate never carries a list of
/// its own.
pub fn aliases_json() -> String {
    let rows: Vec<_> = all_aliases()
        .map(|(old, new, tier)| serde_json::json!({ "old": old, "new": new, "tier": tier }))
        .collect();
    serde_json::to_string_pretty(&rows).expect("a list of string objects serialises")
}

/// [`dev::ALIASES`] then [`user::ALIASES`], each row tagged with its tier.
pub fn all_aliases() -> impl Iterator<Item = (&'static str, &'static str, &'static str)> {
    dev::ALIASES
        .iter()
        .map(|(o, n)| (*o, *n, "dev"))
        .chain(user::ALIASES.iter().map(|(o, n)| (*o, *n, "user")))
}
