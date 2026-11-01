//! phase-528 W1 — the RunPlan resolver (RFC-0118 D3/D5), and `nros run`'s
//! `--dry-run` / `--why`.
//!
//! One function from (verb, image, invocation) to an
//! [`nros_extension::plan::RunPlan`]: [`resolve`]. Its three consumers are
//! `nros build`'s next step ([`crate::cmd::build_next::next_steps`] renders
//! the plan), the `nros run` / `nros flash` verbs, and the test harness
//! (phase-528 W5). None of them derives a launch of its own.
//!
//! # The ladder (RFC-0118 D5)
//!
//! Highest precedence first; every field the plan carries records the rung
//! that supplied it in `provenance`, under these words:
//!
//! | rung | word | source |
//! | --- | --- | --- |
//! | 1 | `invocation` | `--port`, `--probe`, `--runner`, arguments after `--` |
//! | 2 | `image` | `[image.<id>.run\|flash\|console]` over `[image_defaults]`'s, already merged key by key when `system.toml` was read |
//! | 3 | `bench` | `$NROS_HOME/bench.toml` — the rung exists and reads nothing until phase-529 W4 writes the file |
//! | 4 | `board` | `[board.run]`, `[board.run.qemu]`, `[board.flash]`, `[board.console]` |
//! | 5 | `method` | the runner's own defaults, and what the board's platform and triple imply |
//!
//! Tables merge KEY BY KEY across rungs: an image stating `[image.<id>.flash]
//! chip` keeps the board's `method` and `prepare`.
//!
//! # Runner choice (RFC-0119 D8)
//!
//! `run`: a host image is `nros_runner_native`; an emulator on a west build
//! (a Zephyr board, whose own definition selects the emulator) is
//! `nros_runner_west`, except the FVP, which is `nros_runner_fvp`; otherwise
//! the emulator kind names it (`qemu`, `renode`, `wokwi`). `flash`: a
//! `command` template is `nros_runner_command`, any other method is the runner
//! package of that name. The packages are W3's and phase-529's to write; the
//! plan names them now.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use nros_extension::plan::{
    Console, ConsoleKind as PlanConsoleKind, Link, LinkKind, PrepareStep as PlanStep, RunPlan,
    RunnerRef, Target, Verb,
};
use nros_orchestration_ir::target_exec::{
    ConsoleKind, ConsoleTable, FlashTable, PrepareStep, over,
};

use crate::{
    cmd::build_next::BuildOutput,
    orchestration::board_descriptor::{BoardDescriptor, PlatformKind, triple_of_build_target},
};

/// Rung words, as `provenance` records them.
pub const INVOCATION: &str = "invocation";
/// See [`INVOCATION`].
pub const IMAGE: &str = "image";
/// See [`INVOCATION`]. Unused until phase-529 W4 (`$NROS_HOME/bench.toml`).
pub const BENCH: &str = "bench";
/// See [`INVOCATION`].
pub const BOARD: &str = "board";
/// See [`INVOCATION`].
pub const METHOD: &str = "method";

/// What an image's `system.toml` says about how it is run, flashed and
/// observed — `[image.<id>.flash|console]` over `[image_defaults]`'s (merged
/// key by key at parse time), and its `locator` for the plan's link.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImageTables {
    pub flash: Option<FlashTable>,
    pub console: Option<ConsoleTable>,
    /// The image's `locator` (`tcp/10.0.2.2:7447`, `serial/…`).
    pub locator: Option<String>,
}

impl ImageTables {
    /// From a workspace image (already overlaid on `[image_defaults]`).
    #[must_use]
    pub fn from_block(b: &crate::orchestration::image::ImageBlock) -> Self {
        Self {
            flash: b.flash.clone(),
            console: b.console.clone(),
            locator: b.locator.clone(),
        }
    }

    /// From a leaf's `system.toml`.
    #[must_use]
    pub fn from_leaf(l: &nros_orchestration_ir::leaf_system::LeafSystem) -> Self {
        Self {
            flash: l.network.flash.clone(),
            console: l.network.console.clone(),
            locator: l.network.locator.clone(),
        }
    }
}

/// D5 rung 1: what the command line said.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Invocation {
    /// `--port`: the console (and, for a flasher, the target) device.
    pub port: Option<String>,
    /// `--probe`: a debug probe's serial.
    pub probe: Option<String>,
    /// `--runner`: passed to the method (`west flash --runner jlink`).
    pub runner: Option<String>,
    /// Arguments after `--`, unchanged.
    pub passthrough: Vec<String>,
}

/// D5 rung 3: `$NROS_HOME/bench.toml`, which phase-529 W4 writes (the ports
/// and probes on THIS machine). The rung is here so the order and the
/// provenance word are fixed now; it reads nothing until that file exists.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bench {
    pub port: Option<String>,
    pub probe: Option<String>,
}

impl Bench {
    /// The bench facts for `board`. Empty until phase-529 W4.
    #[must_use]
    pub fn for_board(_board: &str) -> Self {
        Self::default()
    }
}

/// Everything [`resolve`] reads.
#[derive(Debug, Clone, Copy)]
pub struct Inputs<'a> {
    /// The image name the plan carries (`<bringup>:<image>`).
    pub image: &'a str,
    /// The board as the image names it, for messages.
    pub board: &'a str,
    pub descriptor: &'a BoardDescriptor,
    /// Where the build put its artifact — the planner's values.
    pub output: Option<&'a BuildOutput>,
    pub tables: &'a ImageTables,
    pub bench: &'a Bench,
    pub invocation: &'a Invocation,
}

/// Why no plan resolved. Every arm is RFC-0118 D2's exit 3.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResolveError {
    /// The verb does not fit the board (`run` on real silicon).
    #[error("{0}")]
    WrongVerb(String),
    /// Nothing on the ladder states what the plan needs.
    #[error("{0}")]
    Unresolved(String),
}

/// A field-by-field provenance recorder.
#[derive(Default)]
struct Prov(BTreeMap<String, String>);

impl Prov {
    fn set(&mut self, key: &str, rung: &str) {
        self.0.insert(key.to_string(), rung.to_string());
    }
}

/// Resolve the plan for `verb`.
///
/// # Errors
///
/// A [`ResolveError`]: the verb does not fit the board, or the ladder states
/// no launcher/flasher, or a `prepare` step the plan contract cannot carry.
pub fn resolve(verb: Verb, i: Inputs<'_>) -> Result<RunPlan, ResolveError> {
    match verb {
        Verb::Run => resolve_run(i),
        Verb::Flash => resolve_flash(i),
        Verb::Console => Err(ResolveError::Unresolved(
            "`nros monitor` (the console verb) is phase-529 W3; no plan resolves for it yet".into(),
        )),
    }
}

fn plan_shell(verb: Verb, i: &Inputs<'_>, runner: RunnerRef, target: Target) -> RunPlan {
    RunPlan {
        protocol: nros_extension::protocol::PROTOCOL,
        image: i.image.to_string(),
        artifacts: BTreeMap::new(),
        build_dir: None,
        verb,
        runner,
        target,
        console: Console {
            kind: PlanConsoleKind::Stdio,
            port: None,
            baud: None,
        },
        link: None,
        args: BTreeMap::new(),
        tools: BTreeMap::new(),
        passthrough: i.invocation.passthrough.clone(),
        prepare: Vec::new(),
        provenance: BTreeMap::new(),
    }
}

fn runner(package: &str, name: &str) -> RunnerRef {
    RunnerRef {
        package: package.to_string(),
        name: name.to_string(),
    }
}

/// The runner package a method word names (`probe-rs` →
/// `nros_runner_probe_rs`).
fn method_runner(method: &str) -> RunnerRef {
    runner(&format!("nros_runner_{}", method.replace('-', "_")), method)
}

fn is_west(i: &Inputs<'_>) -> bool {
    matches!(i.output, Some(BuildOutput::West { .. }))
}

/// The fixed `nros-qemu-ladder` board a platform + triple boots under, for a
/// board whose `[board.run]` names a QEMU emulator and no `[board.run.qemu]`
/// table (its virtio-mmio NIC varies per harness pair). Method defaults.
fn ladder_board(d: &BoardDescriptor) -> Option<(nros_qemu_ladder::Board, Option<String>)> {
    use nros_qemu_ladder::{Board, threadx_riscv64_mac};
    let triple = d
        .target
        .as_deref()
        .map(triple_of_build_target)
        .unwrap_or("");
    match d.platform {
        PlatformKind::Nuttx if triple.starts_with("armv7a") => Some((Board::NuttxArmVirt, None)),
        PlatformKind::Nuttx if triple.starts_with("riscv32") => Some((Board::NuttxRvVirt, None)),
        // Peer 0's MAC — the firmware's `Config::default()`.
        PlatformKind::ThreadxRiscv64 => Some((Board::ThreadxRiscv64, Some(threadx_riscv64_mac(0)))),
        _ => None,
    }
}

fn resolve_run(i: Inputs<'_>) -> Result<RunPlan, ResolveError> {
    let d = i.descriptor;
    let mut prov = Prov::default();
    let mut target = Target::default();
    match d.run.as_ref() {
        Some(r) if r.host => {
            target.host = true;
            prov.set("target.host", BOARD);
        }
        Some(r) if r.emulator.is_some() => {
            target.emulator = r.emulator.clone();
            prov.set("target.emulator", BOARD);
        }
        _ if d.flash.is_some() => {
            return Err(ResolveError::WrongVerb(format!(
                "board `{}` is real silicon (its descriptor states `[board.flash]` and no \
                 `[board.run]`): `nros run` launches host processes and emulators. To program \
                 it and watch its console, use `nros flash --monitor`.",
                i.board
            )));
        }
        // A board with no triple, on any platform but Zephyr (whose host image
        // is a west build), runs on the host.
        _ if d.target.is_none() && d.platform != PlatformKind::Zephyr => {
            target.host = true;
            prov.set("target.host", METHOD);
        }
        _ => {
            return Err(ResolveError::Unresolved(format!(
                "no run or flash command is derivable for board `{}` (its descriptor names no \
                 `[board.run]`)",
                i.board
            )));
        }
    }

    let emu_kind = target
        .emulator
        .as_deref()
        .and_then(|e| e.split_once(':'))
        .map(|(k, _)| k);
    let r = match (target.host, emu_kind) {
        (true, _) => runner("nros_runner_native", "native"),
        (false, Some("fvp")) => runner("nros_runner_fvp", "fvp"),
        (false, Some(_)) if is_west(&i) => runner("nros_runner_west", "west"),
        (false, Some(k)) => runner(&format!("nros_runner_{k}"), k),
        (false, None) => unreachable!("a target was set above"),
    };
    prov.set("runner", METHOD);
    let mut plan = plan_shell(Verb::Run, &i, r, target);

    // ---- args: the QEMU line's facts ----------------------------------------
    if plan.runner.name == "qemu" {
        let run = d.run.as_ref();
        if let Some(q) = run.and_then(|r| r.qemu.as_ref()) {
            let mut put = |k: &str, v: String| {
                plan.args.insert(k.to_string(), v);
                prov.set(&format!("args.{k}"), BOARD);
            };
            put("system", q.system.clone());
            if let Some(c) = &q.cpu {
                put("cpu", c.clone());
            }
            let machine = q.machine.clone().or_else(|| {
                run.and_then(|r| r.emulator_parts())
                    .map(|(_, m)| m.to_string())
            });
            if let Some(m) = machine {
                put("machine", m);
            }
            if let Some(v) = &q.icount {
                put("icount", v.clone());
            }
            if q.semihosting {
                put("semihosting", "enable=on,target=native".into());
            }
            if let Some(n) = &q.nic_model {
                put("nic", format!("user,model={n}"));
            }
        } else if let Some((board, mac)) = ladder_board(d) {
            plan.args.insert("board".into(), board.word().into());
            prov.set("args.board", METHOD);
            if let Some(mac) = mac {
                plan.args.insert("mac".into(), mac);
                prov.set("args.mac", METHOD);
            }
        }
    }

    finish(&mut plan, &mut prov, &i, None)?;
    plan.provenance = prov.0;
    Ok(plan)
}

fn resolve_flash(i: Inputs<'_>) -> Result<RunPlan, ResolveError> {
    let d = i.descriptor;
    let mut prov = Prov::default();
    let board_t = d.flash.as_ref();
    let image_t = i.tables.flash.as_ref();
    // Key by key: what the image states wins, the rest is the board's.
    let who = |img: bool| if img { IMAGE } else { BOARD };
    let merged = over(image_t, board_t, FlashTable::over);
    let img_how = image_t.is_some_and(|t| t.method.is_some() || t.command.is_some());
    let (method, command) = match &merged {
        Some(f) if f.method.is_some() || f.command.is_some() => {
            (f.method.clone(), f.command.clone())
        }
        // Zephyr's own board definition names the flash runner
        // (`runners.yaml`), so a Zephyr board flashes through west unless it
        // says otherwise.
        _ if d.platform == PlatformKind::Zephyr => (Some("west".to_string()), None),
        _ if d.run.is_some() => {
            return Err(ResolveError::WrongVerb(format!(
                "board `{}` states no `[board.flash]`; it runs under {}: use `nros run`",
                i.board,
                d.run
                    .as_ref()
                    .and_then(|r| r.emulator.clone())
                    .map_or_else(|| "the host".to_string(), |e| format!("`{e}`")),
            )));
        }
        _ => {
            return Err(ResolveError::Unresolved(format!(
                "board `{}` states no `[board.flash]` (a flasher `method` or a `command`), and \
                 its image no `[image.<id>.flash]`",
                i.board
            )));
        }
    };
    let how_rung = if merged
        .as_ref()
        .is_some_and(|f| f.method.is_some() || f.command.is_some())
    {
        who(img_how)
    } else {
        METHOD
    };
    let (r, flasher) = match (&method, &command) {
        (_, Some(_)) => (
            runner("nros_runner_command", "command"),
            "command".to_string(),
        ),
        (Some(m), None) => (method_runner(m), m.clone()),
        (None, None) => unreachable!("one is set above"),
    };
    let mut plan = plan_shell(
        Verb::Flash,
        &i,
        r,
        Target {
            flasher: Some(flasher),
            ..Target::default()
        },
    );
    prov.set("target.flasher", how_rung);
    prov.set("runner", how_rung);
    if let Some(c) = &command {
        plan.args.insert("command".into(), c.as_str().to_string());
        prov.set("args.command", how_rung);
    }
    if let Some(f) = &merged {
        if let Some(chip) = &f.chip {
            plan.args.insert("chip".into(), chip.clone());
            prov.set("args.chip", who(image_t.is_some_and(|t| t.chip.is_some())));
        }
        let steps_from_image = image_t.is_some_and(|t| !t.prepare.is_empty());
        for s in &f.prepare {
            match s {
                PrepareStep::Say(t) => plan.prepare.push(PlanStep::Say(t.clone())),
                PrepareStep::WaitFor(w) => plan.prepare.push(PlanStep::WaitFor(w.to_string())),
                PrepareStep::SayThenWait { say, wait_for } => {
                    plan.prepare.push(PlanStep::Say(say.clone()));
                    plan.prepare.push(PlanStep::WaitFor(wait_for.to_string()));
                }
                PrepareStep::Run(cmd) => {
                    return Err(ResolveError::Unresolved(format!(
                        "a `prepare` step runs `{cmd}`, and the plan contract \
                         (`nros_extension::plan::PrepareStep`) has no `run` step yet"
                    )));
                }
            }
        }
        if !f.prepare.is_empty() {
            prov.set("prepare", who(steps_from_image));
        }
    }
    finish(&mut plan, &mut prov, &i, Some(PlanConsoleKind::Serial))?;
    plan.provenance = prov.0;
    Ok(plan)
}

/// The fields every verb fills the same way: console, link, artifacts,
/// invocation args. `console_default` is the method's console kind when the
/// ladder states none (`None`: stdio, the process's or the emulator's).
fn finish(
    plan: &mut RunPlan,
    prov: &mut Prov,
    i: &Inputs<'_>,
    console_default: Option<PlanConsoleKind>,
) -> Result<(), ResolveError> {
    // ---- console: invocation > image > bench > board > method ---------------
    let board_c = i.descriptor.console.as_ref();
    let image_c = i.tables.console.as_ref();
    let merged = over(image_c, board_c, ConsoleTable::over).unwrap_or_default();
    let rung = |img: Option<bool>, brd: bool| match (img, brd) {
        (Some(true), _) => IMAGE,
        (_, true) => BOARD,
        _ => METHOD,
    };
    plan.console.kind = match merged.kind {
        Some(k) => {
            prov.set(
                "console.kind",
                rung(
                    image_c.map(|c| c.kind.is_some()),
                    board_c.is_some_and(|c| c.kind.is_some()),
                ),
            );
            match k {
                ConsoleKind::Stdio => PlanConsoleKind::Stdio,
                ConsoleKind::Serial => PlanConsoleKind::Serial,
                ConsoleKind::Rtt => PlanConsoleKind::Rtt,
                ConsoleKind::Semihosting => PlanConsoleKind::Semihosting,
            }
        }
        None => {
            prov.set("console.kind", METHOD);
            console_default.unwrap_or(PlanConsoleKind::Stdio)
        }
    };
    if let Some(b) = merged.baud {
        plan.console.baud = Some(b);
        prov.set(
            "console.baud",
            rung(
                image_c.map(|c| c.baud.is_some()),
                board_c.is_some_and(|c| c.baud.is_some()),
            ),
        );
    }
    if let Some(p) = &i.invocation.port {
        plan.console.port = Some(p.clone());
        prov.set("console.port", INVOCATION);
    } else if let Some(p) = &i.bench.port {
        plan.console.port = Some(p.clone());
        prov.set("console.port", BENCH);
    }
    if let Some(u) = merged.usb {
        plan.args.insert("usb".into(), u.to_string());
        prov.set(
            "args.usb",
            rung(
                image_c.map(|c| c.usb.is_some()),
                board_c.is_some_and(|c| c.usb.is_some()),
            ),
        );
    }
    if let Some(p) = i.invocation.probe.as_ref().or(i.bench.probe.as_ref()) {
        plan.args.insert("probe".into(), p.clone());
        prov.set(
            "args.probe",
            if i.invocation.probe.is_some() {
                INVOCATION
            } else {
                BENCH
            },
        );
    }
    if let Some(r) = &i.invocation.runner {
        plan.args.insert("runner".into(), r.clone());
        prov.set("args.runner", INVOCATION);
    }
    if !plan.passthrough.is_empty() {
        prov.set("passthrough", INVOCATION);
    }

    // ---- link: the image's locator (RFC-0109 D6) ------------------------------
    if let Some(loc) = &i.tables.locator {
        let kind = match loc.split(['/', ':']).next() {
            Some("tcp") => Some(LinkKind::Tcp),
            Some("udp") => Some(LinkKind::Udp),
            Some("serial") => Some(LinkKind::Serial),
            _ => None,
        };
        if let Some(kind) = kind {
            plan.link = Some(Link {
                kind,
                locator: loc.clone(),
            });
            prov.set("link", IMAGE);
        }
    }

    // ---- artifacts: the planner's values, never a second derivation ---------
    match i.output {
        Some(BuildOutput::West {
            build_dir: Some(dir),
        }) => {
            plan.build_dir = Some(dir.clone());
            // A host image on a west build is native_sim's `zephyr.exe`.
            let z = dir.join("zephyr");
            let elf = if plan.target.host {
                z.join("zephyr.exe")
            } else {
                z.join("zephyr.elf")
            };
            plan.artifacts.insert("elf".into(), elf);
        }
        Some(BuildOutput::CMake { build_dir, .. }) => {
            plan.build_dir = Some(build_dir.clone());
            if let Some(a) = i.output.and_then(BuildOutput::artifact) {
                plan.artifacts.insert("elf".into(), a);
            }
        }
        Some(o) => {
            if let Some(a) = o.artifact() {
                plan.artifacts.insert("elf".into(), a);
            }
        }
        None => {}
    }
    plan.validate()
        .map_err(|e| ResolveError::Unresolved(e.to_string()))
}

// ── `nros run` ──────────────────────────────────────────────────────────

/// `nros run` — launch a host process or an emulator (RFC-0118 D2). Only
/// `--dry-run` and `--why` are implemented (phase-528 W1); executing the plan
/// is phase-528 W4.
#[derive(clap::Args, Debug, Clone)]
pub struct Args {
    /// The image to run (`<image>` or `<bringup>:<image>`), as `nros build`
    /// names it. Omitted: the workspace's one image.
    pub image: Option<String>,
    /// Workspace root (auto-detected from cwd if omitted).
    #[arg(long)]
    pub workspace: Option<PathBuf>,
    /// nano-ros checkout (auto-detected if omitted).
    #[arg(long)]
    pub nano_ros_path: Option<PathBuf>,
    /// Print the resolved plan and run nothing.
    #[arg(long)]
    pub dry_run: bool,
    /// Add the ladder rung that supplied each field.
    #[arg(long)]
    pub why: bool,
    /// The console device (RFC-0118 D5 rung 1).
    #[arg(long)]
    pub port: Option<String>,
    /// Print the plan as the JSON a runner is handed (`--plan`), not TOML.
    #[arg(long)]
    pub json: bool,
    /// Arguments for the target, after `--`.
    #[arg(last = true)]
    pub passthrough: Vec<String>,
}

/// `nros run`.
///
/// # Errors
///
/// Planning the build failed. A plan that cannot resolve exits 3 (RFC-0118
/// D2); a run without `--dry-run` exits 2 until phase-528 W4.
pub fn run(args: Args) -> eyre::Result<()> {
    if !args.dry_run {
        eprintln!(
            "nros run: launching is not implemented until phase-528 W4; `nros run --dry-run \
             [--why]` prints the plan it will execute"
        );
        std::process::exit(i32::from(nros_extension::exit::RUNNER_FAILED));
    }
    let mut build_argv = vec!["build".to_string(), "--dry-run".to_string()];
    if let Some(w) = &args.workspace {
        build_argv.extend(["--workspace".into(), w.display().to_string()]);
    }
    if let Some(n) = &args.nano_ros_path {
        build_argv.extend(["--nano-ros-path".into(), n.display().to_string()]);
    }
    if let Some(i) = &args.image {
        build_argv.push(i.clone());
    }
    let bargs = <crate::cmd::build::Args as clap::Parser>::try_parse_from(build_argv)?;
    let plans = crate::cmd::build::plan_builds(&bargs)?;
    let one = match plans.as_slice() {
        [one] => one,
        [] => unresolved("no image to run: the workspace declares none"),
        many => unresolved(&format!(
            "`nros run` runs one image; name it — one of: {}",
            many.iter()
                .map(|p| p.qualified.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    };
    let Some(descriptor) = one.descriptor.as_ref() else {
        unresolved(&format!(
            "`{}` resolved to no board descriptor, so there is nothing to run",
            one.qualified
        ));
    };
    let invocation = Invocation {
        port: args.port.clone(),
        passthrough: args.passthrough.clone(),
        ..Invocation::default()
    };
    let bench = Bench::for_board(&one.board);
    let plan = match resolve(
        Verb::Run,
        Inputs {
            image: &one.qualified,
            board: &one.board,
            descriptor,
            output: one.output.as_ref(),
            tables: &one.run_tables,
            bench: &bench,
            invocation: &invocation,
        },
    ) {
        Ok(p) => p,
        Err(e) => unresolved(&e.to_string()),
    };
    print!("{}", render_plan_text(&plan, args.why, args.json));
    Ok(())
}

fn unresolved(msg: &str) -> ! {
    eprintln!("nros run: {msg}");
    std::process::exit(i32::from(nros_extension::exit::PLAN_UNRESOLVED));
}

/// The plan as `nros run --dry-run` prints it: TOML for a person (RFC-0118
/// D3's form), JSON for a runner; provenance only with `--why`.
#[must_use]
pub fn render_plan_text(plan: &RunPlan, why: bool, json: bool) -> String {
    let mut p = plan.clone();
    if !why {
        p.provenance.clear();
    }
    if json {
        return p.to_json() + "\n";
    }
    // Serialised from the struct, so the fields keep the contract's order;
    // empty tables and lists are dropped from the human form.
    let text = toml::to_string(&p).expect("a RunPlan is TOML-shaped");
    let mut doc: toml_edit::DocumentMut = text.parse().expect("toml reads its own output");
    let empty: Vec<String> = doc
        .iter()
        .filter(|(_, v)| match v {
            toml_edit::Item::Table(t) => t.is_empty(),
            toml_edit::Item::Value(toml_edit::Value::Array(a)) => a.is_empty(),
            toml_edit::Item::Value(toml_edit::Value::InlineTable(t)) => t.is_empty(),
            _ => false,
        })
        .map(|(k, _)| k.to_string())
        .collect();
    for k in empty {
        doc.remove(&k);
    }
    doc.to_string()
}

/// The QEMU line of a `qemu`-runner plan up to the kernel, through the ONE
/// renderer the qemu runner executes ([`nros_qemu_ladder::plan_line`]).
///
/// # Errors
///
/// The plan's args name no line.
pub fn qemu_line(plan: &RunPlan) -> Result<nros_qemu_ladder::PlanLine, String> {
    nros_qemu_ladder::plan_line(&plan.args)
}

/// The `elf` artifact of a plan, when it has one.
#[must_use]
pub fn elf(plan: &RunPlan) -> Option<&Path> {
    plan.artifacts.get("elf").map(PathBuf::as_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::board_descriptor::BoardCatalog;
    use nros_orchestration_ir::target_exec::CommandTemplate;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap()
            .to_path_buf()
    }

    fn board(cat: &BoardCatalog, name: &str) -> BoardDescriptor {
        cat.descriptors()
            .iter()
            .find(|d| d.names.iter().any(|n| n == name))
            .unwrap_or_else(|| panic!("{name}"))
            .clone()
    }

    fn cargo_out(triple: Option<&str>) -> BuildOutput {
        BuildOutput::Cargo {
            target_dir: "/ws/target".into(),
            triple: triple.map(str::to_string),
            profile_dir: "debug".into(),
            bin: Some("app".into()),
        }
    }

    fn go(
        verb: Verb,
        d: &BoardDescriptor,
        out: &BuildOutput,
        tables: &ImageTables,
        inv: &Invocation,
    ) -> Result<RunPlan, ResolveError> {
        resolve(
            verb,
            Inputs {
                image: "b:img",
                board: d.names[0].as_str(),
                descriptor: d,
                output: Some(out),
                tables,
                bench: &Bench::default(),
                invocation: inv,
            },
        )
    }

    /// The board rung: `[board.run]` and `[board.run.qemu]` give the target,
    /// the runner and every QEMU arg, each recorded as `board`.
    #[test]
    fn the_board_rung_states_an_mps2_qemu_plan() {
        let cat = BoardCatalog::load(&repo_root()).unwrap();
        let d = board(&cat, "mps2-an385-baremetal");
        let p = go(
            Verb::Run,
            &d,
            &cargo_out(Some("thumbv7m-none-eabi")),
            &ImageTables::default(),
            &Invocation::default(),
        )
        .unwrap();
        assert_eq!(p.target.emulator.as_deref(), Some("qemu:mps2-an385"));
        assert_eq!(p.runner.package, "nros_runner_qemu");
        assert_eq!(p.args["nic"], "user,model=lan9118");
        assert_eq!(p.console.kind, PlanConsoleKind::Semihosting);
        for k in [
            "target.emulator",
            "args.system",
            "args.machine",
            "args.nic",
            "console.kind",
        ] {
            assert_eq!(p.provenance[k], BOARD, "{k}");
        }
        assert_eq!(p.provenance["runner"], METHOD);
        assert_eq!(
            qemu_line(&p).unwrap().argv("K").join(" "),
            "-cpu cortex-m3 -machine mps2-an385 -nographic -icount shift=auto \
             -semihosting-config enable=on,target=native -nic user,model=lan9118 -kernel K"
        );
    }

    /// The method rung: a board with no triple runs on the host; the NuttX
    /// board's line is the ladder's, keyed on platform + triple.
    #[test]
    fn the_method_rung_fills_what_the_board_leaves_out() {
        let cat = BoardCatalog::load(&repo_root()).unwrap();
        let mut linux = board(&cat, "linux");
        linux.run = None;
        linux.console = None;
        let p = go(
            Verb::Run,
            &linux,
            &cargo_out(None),
            &ImageTables::default(),
            &Invocation::default(),
        )
        .unwrap();
        assert!(p.target.host);
        assert_eq!(p.provenance["target.host"], METHOD);
        assert_eq!(p.provenance["console.kind"], METHOD);
        assert_eq!(p.runner.name, "native");

        let nuttx = board(&cat, "nuttx");
        let p = go(
            Verb::Run,
            &nuttx,
            &cargo_out(nuttx.target.as_deref()),
            &ImageTables::default(),
            &Invocation::default(),
        )
        .unwrap();
        assert_eq!(p.args["board"], "nuttx-arm-virt");
        assert_eq!(p.provenance["args.board"], METHOD);
        assert_eq!(p.provenance["target.emulator"], BOARD);
    }

    /// The image and invocation rungs, key by key over the board's tables.
    #[test]
    fn the_image_and_invocation_rungs_override_key_by_key() {
        let cat = BoardCatalog::load(&repo_root()).unwrap();
        let mut d = board(&cat, "nucleo-f767zi-zephyr");
        d.flash = Some(FlashTable {
            method: Some("probe-rs".into()),
            chip: Some("STM32F767ZITx".into()),
            ..FlashTable::default()
        });
        let tables = ImageTables {
            flash: Some(FlashTable {
                chip: Some("STM32F767ZI".into()),
                prepare: vec![PrepareStep::SayThenWait {
                    say: "Press RESET".into(),
                    wait_for: "usb:0483:374b".parse().unwrap(),
                }],
                ..FlashTable::default()
            }),
            console: Some(ConsoleTable {
                baud: Some(921_600),
                ..ConsoleTable::default()
            }),
            locator: Some("serial/serial@40004800#baudrate=115200".into()),
        };
        let inv = Invocation {
            port: Some("/dev/ttyACM0".into()),
            probe: Some("0670FF".into()),
            ..Invocation::default()
        };
        let out = BuildOutput::West {
            build_dir: Some("/b".into()),
        };
        let p = go(Verb::Flash, &d, &out, &tables, &inv).unwrap();
        assert_eq!(p.target.flasher.as_deref(), Some("probe-rs"));
        assert_eq!(p.runner.package, "nros_runner_probe_rs");
        assert_eq!(
            p.provenance["target.flasher"], BOARD,
            "the image did not restate the method"
        );
        assert_eq!(p.args["chip"], "STM32F767ZI");
        assert_eq!(p.provenance["args.chip"], IMAGE);
        assert_eq!(
            p.prepare,
            vec![
                PlanStep::Say("Press RESET".into()),
                PlanStep::WaitFor("usb:0483:374b".into())
            ]
        );
        assert_eq!(p.provenance["prepare"], IMAGE);
        assert_eq!(p.console.kind, PlanConsoleKind::Serial);
        assert_eq!(p.provenance["console.kind"], BOARD);
        assert_eq!(p.console.baud, Some(921_600));
        assert_eq!(p.provenance["console.baud"], IMAGE);
        assert_eq!(p.console.port.as_deref(), Some("/dev/ttyACM0"));
        assert_eq!(p.provenance["console.port"], INVOCATION);
        assert_eq!(p.provenance["args.probe"], INVOCATION);
        assert_eq!(p.link.as_ref().map(|l| l.kind), Some(LinkKind::Serial));
        assert_eq!(p.artifacts["elf"], PathBuf::from("/b/zephyr/zephyr.elf"));

        // An image `command` replaces the board's method: the command runner.
        let tables = ImageTables {
            flash: Some(FlashTable {
                command: Some(CommandTemplate::try_from("pyocd flash {hex}".to_string()).unwrap()),
                ..FlashTable::default()
            }),
            ..ImageTables::default()
        };
        let p = go(Verb::Flash, &d, &out, &tables, &Invocation::default()).unwrap();
        assert_eq!(p.runner.name, "command");
        assert_eq!(p.args["command"], "pyocd flash {hex}");
        assert_eq!(p.provenance["runner"], IMAGE);
        assert_eq!(p.args["chip"], "STM32F767ZITx", "the board's chip survives");
        assert_eq!(p.provenance["args.chip"], BOARD);
    }

    /// The verb picks the target: the Nucleo's `run` is its Renode twin through
    /// Zephyr's west, its `flash` is the silicon through west; real silicon
    /// with no twin refuses `run`, naming `nros flash --monitor`.
    #[test]
    fn the_verb_picks_the_target() {
        let cat = BoardCatalog::load(&repo_root()).unwrap();
        let d = board(&cat, "nucleo-f767zi-zephyr");
        let out = BuildOutput::West {
            build_dir: Some("/b".into()),
        };
        let none = ImageTables::default();
        let inv = Invocation::default();
        let run = go(Verb::Run, &d, &out, &none, &inv).unwrap();
        assert_eq!(run.target.emulator.as_deref(), Some("renode:nucleo_f767zi"));
        assert_eq!(run.runner.name, "west");
        let flash = go(Verb::Flash, &d, &out, &none, &inv).unwrap();
        assert_eq!(flash.target.flasher.as_deref(), Some("west"));
        assert_eq!(flash.provenance["target.flasher"], BOARD);

        let mut silicon = d.clone();
        silicon.run = None;
        let e = go(Verb::Run, &silicon, &out, &none, &inv).unwrap_err();
        assert!(e.to_string().contains("nros flash --monitor"), "{e}");

        // A Zephyr board that states no flasher flashes through west (method).
        let z = board(&cat, "mps2-an385-zephyr");
        let p = go(Verb::Flash, &z, &out, &none, &inv).unwrap();
        assert_eq!(p.provenance["target.flasher"], METHOD);

        // A host board has no flasher.
        let linux = board(&cat, "linux");
        assert!(go(Verb::Flash, &linux, &cargo_out(None), &none, &inv).is_err());
    }

    /// The `--dry-run --why` snapshots: mps2-an385 bare metal, linux, and
    /// Zephyr native_sim. The text is the human (TOML) form.
    #[test]
    fn dry_run_why_snapshots() {
        let cat = BoardCatalog::load(&repo_root()).unwrap();
        let none = ImageTables {
            locator: Some("tcp/10.0.2.2:7447".into()),
            ..ImageTables::default()
        };
        let inv = Invocation::default();
        let mps2 = go(
            Verb::Run,
            &board(&cat, "mps2-an385-baremetal"),
            &cargo_out(Some("thumbv7m-none-eabi")),
            &none,
            &inv,
        )
        .unwrap();
        assert_eq!(
            render_plan_text(&mps2, true, false),
            r#"protocol = 1
image = "b:img"
verb = "run"

[artifacts]
elf = "/ws/target/thumbv7m-none-eabi/debug/app"

[runner]
package = "nros_runner_qemu"
name = "qemu"

[target]
emulator = "qemu:mps2-an385"

[console]
kind = "semihosting"

[link]
kind = "tcp"
locator = "tcp/10.0.2.2:7447"

[args]
cpu = "cortex-m3"
icount = "shift=auto"
machine = "mps2-an385"
nic = "user,model=lan9118"
semihosting = "enable=on,target=native"
system = "arm"

[provenance]
"args.cpu" = "board"
"args.icount" = "board"
"args.machine" = "board"
"args.nic" = "board"
"args.semihosting" = "board"
"args.system" = "board"
"console.kind" = "board"
link = "image"
runner = "method"
"target.emulator" = "board"
"#
        );
        let linux = go(
            Verb::Run,
            &board(&cat, "linux"),
            &cargo_out(None),
            &ImageTables::default(),
            &inv,
        )
        .unwrap();
        assert_eq!(
            render_plan_text(&linux, true, false),
            r#"protocol = 1
image = "b:img"
verb = "run"

[artifacts]
elf = "/ws/target/debug/app"

[runner]
package = "nros_runner_native"
name = "native"

[target]
host = true

[console]
kind = "stdio"

[provenance]
"console.kind" = "board"
runner = "method"
"target.host" = "board"
"#
        );
        let sim = go(
            Verb::Run,
            &board(&cat, "zephyr"),
            &BuildOutput::West {
                build_dir: Some("/ws/build/z".into()),
            },
            &ImageTables::default(),
            &inv,
        )
        .unwrap();
        assert_eq!(
            render_plan_text(&sim, true, false),
            r#"protocol = 1
image = "b:img"
build_dir = "/ws/build/z"
verb = "run"

[artifacts]
elf = "/ws/build/z/zephyr/zephyr.exe"

[runner]
package = "nros_runner_native"
name = "native"

[target]
host = true

[console]
kind = "stdio"

[provenance]
"console.kind" = "board"
runner = "method"
"target.host" = "board"
"#
        );
        // Without `--why`, no provenance; `--json` is the runner contract's.
        assert!(!render_plan_text(&sim, false, false).contains("provenance"));
        assert!(
            RunPlan::from_json(&render_plan_text(&sim, true, true)).is_ok(),
            "the JSON form is a plan a runner reads"
        );
    }
}
