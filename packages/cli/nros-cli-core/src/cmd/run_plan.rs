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
//! | 3 | `bench` | `$NROS_HOME/bench.toml` and the attached devices ([`bench_for`], RFC-0118 D6), for a flash target |
//! | 4 | `board` | `[board.run]`, `[board.run.qemu]`, `[board.flash]`, `[board.console]` |
//! | 5 | `method` | the runner's own defaults, and what the board's platform and triple imply (never the target itself: a board states `[board.run]` or `[board.flash]`) |
//!
//! Tables merge KEY BY KEY across rungs: an image stating `[image.<id>.flash]
//! chip` keeps the board's `method` and `prepare`.
//!
//! # Runner choice (RFC-0119 D8)
//!
//! `run`: a host image is `nros_runner_native`; an emulator Zephyr's own build
//! runs ([`WEST_EMULATORS`]) on a west build is `nros_runner_west`; the FVP
//! is `nros_runner_fvp`; otherwise the emulator kind names it (`qemu`,
//! `renode`), and a kind no package drives yet ([`RUNNERLESS_EMULATORS`]:
//! `wokwi`, issue 2230) is refused naming its issue. `flash`: a
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
use nros_orchestration_ir::target_exec::{ConsoleKind, ConsoleTable, FlashTable, over};

use crate::{
    cmd::build_next::BuildOutput,
    orchestration::board_descriptor::{BoardDescriptor, PlatformKind, triple_of_build_target},
};

/// Rung words, as `provenance` records them.
pub const INVOCATION: &str = "invocation";
/// See [`INVOCATION`].
pub const IMAGE: &str = "image";
/// See [`INVOCATION`]. The machine's bench ([`bench_for`]).
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
    /// The image's resolved network as the build receives it (the
    /// `NROS_IMAGE_NET` text), which carries `[image.<id>.run] nic` and the
    /// MAC the firmware was built with. `None`: the image states no network.
    pub net: Option<String>,
}

impl ImageTables {
    /// From a workspace image (already overlaid on `[image_defaults]`) and
    /// its resolved network.
    #[must_use]
    pub fn from_block(
        b: &crate::orchestration::image::ImageBlock,
        net: Option<&crate::orchestration::image_net::ImageNet>,
    ) -> Self {
        Self {
            flash: b.flash.clone(),
            console: b.console.clone(),
            locator: b.locator.clone(),
            net: net.map(crate::orchestration::image_net::ImageNet::text),
        }
    }

    /// From a leaf's `system.toml` and its resolved network.
    #[must_use]
    pub fn from_leaf(
        l: &nros_orchestration_ir::leaf_system::LeafSystem,
        net: Option<&crate::orchestration::image_net::ImageNet>,
    ) -> Self {
        Self {
            flash: l.network.flash.clone(),
            console: l.network.console.clone(),
            locator: l.network.locator.clone(),
            net: net.map(crate::orchestration::image_net::ImageNet::text),
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

/// D5 rung 3: what THIS machine's bench says for the board — the port and
/// probe [`bench_for`] resolves from `$NROS_HOME/bench.toml` and the attached
/// devices (phase-529 W4, [`crate::cmd::bench`]). The resolver applies it to
/// a FLASH target only: an emulator's console is the emulator's, never a
/// device on this machine's bus. `Bench::default()` is "this machine adds
/// nothing", which is what `nros build`'s next step and `nros run` pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bench {
    pub port: Option<String>,
    pub probe: Option<String>,
}

/// The bench rung for `board` under `verb`: its port by RFC-0118 D6's
/// ladder past the invocation (the bench file's entry found on the bus, else
/// exactly one device matching the console's USB id), and the entry's probe.
/// Nothing for `run`. `invocation.port` (`--port`, or `NROS_PORT` folded in by
/// the verb) answers first, so the bench is not consulted.
///
/// # Errors
///
/// Two candidate devices, or a recorded device that is not attached: a
/// refusal listing what was found (RFC-0118 D2 exit 3).
pub fn bench_for(
    verb: Verb,
    board: &str,
    descriptor: &BoardDescriptor,
    tables: &ImageTables,
    invocation: &Invocation,
    machine: &crate::cmd::bench::Machine,
) -> Result<Bench, ResolveError> {
    if verb == Verb::Run {
        return Ok(Bench::default());
    }
    let mut out = Bench {
        probe: machine.file.entry(board).and_then(|e| e.probe.clone()),
        ..Bench::default()
    };
    if invocation.port.is_some() {
        return Ok(out);
    }
    let usb = over(
        tables.console.as_ref(),
        descriptor.console.as_ref(),
        ConsoleTable::over,
    )
    .and_then(|c| c.usb)
    .map(|u| nros_extension::usb::UsbId {
        vid: u.vid,
        pid: u.pid,
    });
    out.port = crate::cmd::bench::resolve_port(None, None, machine, board, usb)
        .map_err(ResolveError::Unresolved)?
        .map(|(p, _)| p);
    Ok(out)
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
        Verb::Console => resolve_console(i),
    }
}

/// The console of a real board (`nros monitor`, and `nros flash --monitor`
/// after the flash): the board's flash target, observed through the `serial`
/// runner. It is the flash plan's resolution, so the port, baud, console
/// command and provenance are the ones the flash used, minus what only a
/// flash needs (the prepare steps, the flash command and chip).
fn resolve_console(i: Inputs<'_>) -> Result<RunPlan, ResolveError> {
    let mut plan = resolve_flash(i).map_err(|e| match e {
        ResolveError::WrongVerb(_) => ResolveError::WrongVerb(format!(
            "board `{}` is not real silicon: its console is the process or emulator `nros run` \
             starts, and `nros run` streams it",
            i.board
        )),
        other => other,
    })?;
    console_of(&mut plan);
    Ok(plan)
}

/// Turn a flash plan into the console plan of the same board: same target,
/// port, baud and console command, the `serial` runner, and none of what only
/// a flash needs. `nros flash --monitor` hands the flashed port on this way,
/// so the console opens the device the flash just wrote.
pub fn console_of(plan: &mut RunPlan) {
    plan.verb = Verb::Console;
    plan.runner = runner("nros_runner_serial", "serial");
    plan.prepare.clear();
    plan.args.remove("command");
    plan.args.remove("chip");
    for k in ["prepare", "args.command", "args.chip"] {
        plan.provenance.remove(k);
    }
    plan.provenance.insert("runner".into(), METHOD.into());
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
            command: None,
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

/// The emulator kinds a Zephyr build runs itself: its board declares them and
/// the build emits a `run_<kind>` target, so a Zephyr image on one goes to
/// the `west` runner. (The FVP is Zephyr's `armfvp`, which has its own runner
/// for the model path.) Any other kind on a west build, such as `wokwi`, is
/// not Zephyr's to run, and is named by its own runner like on any other
/// platform.
pub const WEST_EMULATORS: &[&str] = &["qemu", "renode"];

/// Emulator kinds no runner package drives yet, each with the issue that
/// tracks the package. A plan for one is refused at resolve time, naming the
/// issue, rather than handed to a runner that would refuse it after the build.
/// `check-board-runner-resolves` reports these as known gaps and fails once a
/// package provides the runner while the row is still here.
pub const RUNNERLESS_EMULATORS: &[(&str, &str)] = &[("wokwi", "2230")];

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
        // The descriptor is the one source of how a board runs (RFC-0118 D4):
        // nothing is guessed from a missing triple. The support registry
        // records whether a run was verified, never whether one exists.
        _ => {
            return Err(ResolveError::Unresolved(format!(
                "board `{}` states neither `[board.run]` nor `[board.flash]`, so nothing says how \
                 it runs. Add `[board.run]` to its nros-board.toml with `host = true` for a host \
                 process or `emulator = \"<kind>:<machine>\"` for an emulator, or `[board.flash]` \
                 for real silicon.",
                i.board
            )));
        }
    }

    let emu_kind = target
        .emulator
        .as_deref()
        .and_then(|e| e.split_once(':'))
        .map(|(k, _)| k);
    if let Some((kind, issue)) = emu_kind.and_then(|k| {
        RUNNERLESS_EMULATORS
            .iter()
            .find(|(runnerless, _)| *runnerless == k)
    }) {
        return Err(ResolveError::Unresolved(format!(
            "board `{}` runs under `{}`, and no runner package drives `{kind}` yet (issue \
             {issue}). Zephyr's west build declares no `{kind}` run target, so the `west` \
             runner cannot boot it either.",
            i.board,
            target.emulator.as_deref().unwrap_or(kind),
        )));
    }
    let r = match (target.host, emu_kind) {
        (true, _) => runner("nros_runner_native", "native"),
        (false, Some("fvp")) => runner("nros_runner_fvp", "fvp"),
        (false, Some(k)) if is_west(&i) && WEST_EMULATORS.contains(&k) => {
            runner("nros_runner_west", "west")
        }
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
            if let Some(model) = &q.nic_model {
                // The image's network, when it states one, says how the NIC
                // attaches (`[image.<id>.run] nic`) and with which MAC; the
                // board says only the model.
                let (nic, rung) = match i.tables.net.as_deref() {
                    Some(text) => (
                        image_nic(text, model).map_err(|e| {
                            ResolveError::Unresolved(format!("image `{}`: {e}", i.image))
                        })?,
                        IMAGE,
                    ),
                    None => (nros_board_common::image_net::slirp_nic(model), BOARD),
                };
                plan.args.insert("nic".into(), nic);
                prov.set("args.nic", rung);
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

/// QEMU's `-nic` value for the primary interface of an image's resolved
/// network (`text`), on a NIC of `model`.
fn image_nic(text: &str, model: &str) -> Result<String, String> {
    use nros_board_common::image_net;
    let primary = image_net::primary(text)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "its network resolved no interface".to_string())?;
    image_net::qemu_nic(&primary, model)
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
            // The table's step and the plan's step have one JSON form
            // (RFC-0118 D7), held equal in both directions by
            // `tests/prepare_step_wire_agreement.rs`, so the step crosses
            // unchanged: `run` and the `say` + `wait_for` pair included.
            let step: PlanStep = serde_json::to_value(s)
                .and_then(serde_json::from_value)
                .map_err(|e| {
                    ResolveError::Unresolved(format!(
                        "a `prepare` step does not carry onto the plan: {e}"
                    ))
                })?;
            plan.prepare.push(step);
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
    if let Some(c) = &merged.command {
        plan.console.command = Some(c.as_str().to_string());
        prov.set(
            "console.command",
            rung(
                image_c.map(|c| c.command.is_some()),
                board_c.is_some_and(|c| c.command.is_some()),
            ),
        );
    }
    if let Some(p) = &i.invocation.port {
        plan.console.port = Some(p.clone());
        prov.set("console.port", INVOCATION);
    } else if let Some(p) = i
        .bench
        .port
        .as_ref()
        .filter(|_| plan.target.flasher.is_some())
    {
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
    let bench_probe = i
        .bench
        .probe
        .as_ref()
        .filter(|_| plan.target.flasher.is_some());
    if let Some(p) = i.invocation.probe.as_ref().or(bench_probe) {
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
                rmw: None,
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

/// `nros run` — launch a host process or an emulator. `--dry-run` prints the
/// plan; otherwise [`crate::cmd::run_exec`] executes it.
// RFC-0118 D2; resolving is phase-528 W1, executing is W4.
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
    /// The console device.
    // RFC-0118 D5 rung 1.
    #[arg(long)]
    pub port: Option<String>,
    /// Print the plan as the JSON a runner is handed (`--plan`), not TOML.
    #[arg(long)]
    pub json: bool,
    /// Arguments for the target, after `--`.
    #[arg(last = true)]
    pub passthrough: Vec<String>,
    #[command(flatten)]
    pub exec: crate::cmd::run_exec::ExecArgs,
}

/// `nros run`.
///
/// # Errors
///
/// Planning the build failed. A plan that cannot resolve exits 3 (RFC-0118
/// D2); otherwise the process exits with the code the execution gives.
pub fn run(args: Args) -> eyre::Result<()> {
    if let Some(file) = &args.exec.plan {
        return crate::cmd::run_exec::run_plan_file(&args, file);
    }
    // RFC-0120 D3 -- configure implicitly first, as `nros build` does; never
    // from inside planning, which configure itself runs under its lock.
    let bargs = selection_build_args(&Selection::from(&args)).map_err(|e| eyre::eyre!(e))?;
    crate::cmd::build::configure_implicitly(&bargs)?;
    let one = match select_target(&Selection::from(&args), "run") {
        Ok(one) => one,
        Err(e) => unresolved(&e),
    };
    let one = &one;
    let descriptor = one.descriptor.as_ref().expect("select_target checked it");
    let invocation = Invocation {
        // RFC-0118 D6: `--port`, then `NROS_PORT` — both the invocation rung.
        port: args
            .port
            .clone()
            .or_else(|| std::env::var("NROS_PORT").ok().filter(|p| !p.is_empty())),
        passthrough: args.passthrough.clone(),
        ..Invocation::default()
    };
    // A `run` target is a host process or an emulator: this machine's bench
    // adds nothing to it (`bench_for` answers empty for `run`).
    let bench = Bench::default();
    let mut plan = match resolve(
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
    // The emulator executable the runner is handed, through the QEMU ladder.
    // Attached before the dry-run prints, so the printed plan is the run one.
    let root = crate::orchestration::nano_ros_root::resolve(
        args.nano_ros_path.clone(),
        &std::env::current_dir().unwrap_or_default(),
    );
    let ctx = crate::orchestration::qemu_bin::for_sdk(root.as_deref());
    for note in attach_tools(&mut plan, &|exe| {
        crate::orchestration::qemu_bin::locate(exe, &ctx)
    }) {
        eprintln!("nros run: {note}");
    }
    crate::cmd::run_exec::decorate(&mut plan, one.rmw.as_deref(), &args.exec);
    if args.dry_run {
        print!("{}", render_plan_text(&plan, args.why, args.json));
        // `--why` also names the package that answers the runner name, and
        // what it overrides (RFC-0119 D4): the plan names the runner, the
        // provider search path decides who it is.
        if args.why && !args.json {
            let ws = args
                .workspace
                .clone()
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_default();
            let located = crate::cmd::run_exec::locate_runner(
                &ws,
                root.as_deref(),
                &plan.runner.name,
                &args.exec.allow_overriding,
                &args.exec.sources.base_paths,
            );
            print!(
                "{}",
                crate::cmd::run_exec::why_runner(&located, &plan.runner.name)
            );
            print!(
                "{}",
                crate::cmd::run_exec::why_source_roots(&ws, &args.exec.sources.base_paths)
            );
        }
        return Ok(());
    }
    crate::cmd::run_exec::build_and_execute(&args, &one.qualified, &plan)
}

/// Which image a target verb acts on: the workspace, the nano-ros checkout
/// and the image as the user named them.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    /// `--workspace`.
    pub workspace: Option<PathBuf>,
    /// `--nano-ros-path`.
    pub nano_ros_path: Option<PathBuf>,
    /// The positional image (`<image>` or `<bringup>:<image>`).
    pub image: Option<String>,
    /// `--base-paths`, handed to the planning and the implicit build as
    /// `nros build` takes it (phase-527 W2d).
    pub base_paths: Vec<PathBuf>,
}

impl From<&Args> for Selection {
    fn from(a: &Args) -> Self {
        Self {
            workspace: a.workspace.clone(),
            nano_ros_path: a.nano_ros_path.clone(),
            image: a.image.clone(),
            base_paths: a.exec.sources.base_paths.clone(),
        }
    }
}

impl Selection {
    /// The `nros build` arguments this selection stands for, after the verb:
    /// what `select_target` plans with and what the implicit build runs, so
    /// the two cannot see different workspaces.
    #[must_use]
    pub fn build_args(&self) -> Vec<String> {
        let mut v = Vec::new();
        if let Some(w) = &self.workspace {
            v.extend(["--workspace".into(), w.display().to_string()]);
        }
        if let Some(n) = &self.nano_ros_path {
            v.extend(["--nano-ros-path".into(), n.display().to_string()]);
        }
        if !self.base_paths.is_empty() {
            v.push("--base-paths".into());
            v.extend(self.base_paths.iter().map(|p| p.display().to_string()));
        }
        if let Some(i) = &self.image {
            v.push(i.clone());
        }
        v
    }
}

/// The dry `nros build` arguments a selection plans with: one spelling for
/// [`select_target`] and for `nros run`'s implicit configure before it.
fn selection_build_args(sel: &Selection) -> Result<crate::cmd::build::Args, String> {
    let mut build_argv = vec!["build".to_string(), "--dry-run".to_string()];
    build_argv.extend(sel.build_args());
    <crate::cmd::build::Args as clap::Parser>::try_parse_from(build_argv).map_err(|e| e.to_string())
}

/// THE target resolution of `nros run`, `nros flash` and `nros monitor`: the
/// one image `sel` names (or the workspace's only one), with its board
/// descriptor. Every rung of "which image" lives here and nowhere else.
///
/// # Errors
///
/// No image, more than one, or one with no board descriptor: a message for
/// RFC-0118 D2's exit 3. `verb` is the user's word (`run`, `flash`) for it.
pub fn select_target(
    sel: &Selection,
    verb: &str,
) -> Result<crate::cmd::build::ResolvedBuild, String> {
    let bargs = selection_build_args(sel)?;
    let mut plans = crate::cmd::build::plan_builds(&bargs).map_err(|e| format!("{e:#}"))?;
    let one = match plans.len() {
        1 => plans.remove(0),
        0 => return Err(format!("no image to {verb}: the workspace declares none")),
        _ => {
            return Err(format!(
                "`nros {verb}` acts on one image; name it — one of: {}",
                plans
                    .iter()
                    .map(|p| p.qualified.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    };
    if one.descriptor.is_none() {
        return Err(format!(
            "`{}` resolved to no board descriptor, so there is nothing to {verb}",
            one.qualified
        ));
    }
    Ok(one)
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

/// Put the plan's emulator executable in `plan.tools`, where its runner reads
/// it (RFC-0118 D5: a tool is resolved by nros, never from the runner's
/// `PATH`). For a `qemu` plan that is `qemu-system-<arch>` through the ONE
/// QEMU ladder (`qemu`, [`crate::orchestration::qemu_bin`]): a rung's answer
/// is used as is, and a target the store does not build
/// ([`QemuBin::Host`](nros_qemu_ladder::QemuBin::Host)) is the host's own,
/// looked up on `PATH` HERE, by the ladder's policy, so the plan still names
/// an absolute path. Returns the notes for what could not be resolved; the
/// plan is then left without the tool and the runner refuses, naming it.
pub fn attach_tools(
    plan: &mut RunPlan,
    qemu: &dyn Fn(&str) -> nros_qemu_ladder::QemuBin,
) -> Vec<String> {
    use nros_qemu_ladder::QemuBin;
    let mut notes = Vec::new();
    if plan.runner.name != "qemu" {
        return notes;
    }
    let Ok(line) = qemu_line(plan) else {
        return notes;
    };
    let found = match qemu(&line.exe) {
        QemuBin::Found { path, .. } => Some(path),
        QemuBin::Host { why } => {
            let on_path = std::env::var_os("PATH").and_then(|p| {
                std::env::split_paths(&p)
                    .map(|d| d.join(&line.exe))
                    .find(|c| c.is_file())
            });
            if on_path.is_none() {
                notes.push(why);
            }
            on_path
        }
        QemuBin::Missing { why } | QemuBin::RefusedPrefix { why, .. } => {
            notes.push(why);
            None
        }
    };
    if let Some(path) = found {
        plan.tools.insert(line.exe.clone(), path);
        plan.provenance
            .insert(format!("tools.{}", line.exe), METHOD.to_string());
    }
    notes
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
    use nros_orchestration_ir::target_exec::{CommandTemplate, PrepareStep};

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

    /// The image rung's NIC: an image with a resolved network gets the `-nic`
    /// its `[image.<id>.run] nic` and MAC say, on the board's NIC model; one
    /// on a physical-only network refuses rather than booting on no network.
    #[test]
    fn the_image_network_states_the_emulator_nic() {
        let cat = BoardCatalog::load(&repo_root()).unwrap();
        let d = board(&cat, "mps2-an385-baremetal");
        let out = cargo_out(Some("thumbv7m-none-eabi"));
        let inv = Invocation::default();
        let with = |net: &str| ImageTables {
            net: Some(net.into()),
            ..ImageTables::default()
        };
        let p = go(
            Verb::Run,
            &d,
            &out,
            &with("eth0,ethernet,lab,192.168.50.10,24,-,02:00:00:00:00:0a,tap=tap0"),
            &inv,
        )
        .unwrap();
        assert_eq!(
            p.args["nic"],
            "tap,ifname=tap0,script=no,downscript=no,model=lan9118,mac=02:00:00:00:00:0a"
        );
        assert_eq!(p.provenance["args.nic"], IMAGE);
        let line = qemu_line(&p).unwrap().argv("K").join(" ");
        assert!(line.contains("-nic tap,ifname=tap0"), "{line}");

        let p = go(
            Verb::Run,
            &d,
            &out,
            &with("eth0,ethernet,qemu,10.0.2.15,24,10.0.2.2,02:00:00:00:00:0b,slirp"),
            &inv,
        )
        .unwrap();
        assert_eq!(p.args["nic"], "user,model=lan9118,mac=02:00:00:00:00:0b");

        let e = go(
            Verb::Run,
            &d,
            &out,
            &with("eth0,ethernet,lab,192.168.50.10,24,-,02:00:00:00:00:0a,-"),
            &inv,
        )
        .unwrap_err();
        assert!(e.to_string().contains("physical-only"), "{e}");
    }

    /// A console attached through a command template reaches the plan, key by
    /// key over the board's console like every other console fact.
    #[test]
    fn a_console_command_reaches_the_plan() {
        let cat = BoardCatalog::load(&repo_root()).unwrap();
        let mut d = board(&cat, "nucleo-f767zi-zephyr");
        d.console = Some(ConsoleTable {
            kind: Some(ConsoleKind::Serial),
            baud: Some(115_200),
            command: Some(
                CommandTemplate::try_from("picocom -b {baud} {port}".to_string()).unwrap(),
            ),
            ..ConsoleTable::default()
        });
        let out = BuildOutput::West {
            build_dir: Some("/b".into()),
        };
        let inv = Invocation {
            port: Some("/dev/ttyACM0".into()),
            ..Invocation::default()
        };
        let p = go(Verb::Flash, &d, &out, &ImageTables::default(), &inv).unwrap();
        assert_eq!(
            p.console.command.as_deref(),
            Some("picocom -b {baud} {port}")
        );
        assert_eq!(p.provenance["console.command"], BOARD);
        assert_eq!(
            p.expand(p.console.command.as_deref().unwrap()).unwrap(),
            "picocom -b 115200 /dev/ttyACM0"
        );

        let tables = ImageTables {
            console: Some(ConsoleTable {
                command: Some(CommandTemplate::try_from("minicom -D {port}".to_string()).unwrap()),
                ..ConsoleTable::default()
            }),
            ..ImageTables::default()
        };
        let p = go(Verb::Flash, &d, &out, &tables, &inv).unwrap();
        assert_eq!(p.console.command.as_deref(), Some("minicom -D {port}"));
        assert_eq!(p.provenance["console.command"], IMAGE);
        assert_eq!(p.console.baud, Some(115_200), "the board's baud survives");
    }

    /// The method rung fills the console kind and the NuttX board's line
    /// (the ladder's, keyed on platform + triple), but never the target: a
    /// board stating neither `[board.run]` nor `[board.flash]` is refused,
    /// naming both tables, rather than guessed onto the host from a missing
    /// triple.
    #[test]
    fn the_method_rung_fills_what_the_board_leaves_out() {
        let cat = BoardCatalog::load(&repo_root()).unwrap();
        let mut linux = board(&cat, "linux");
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
        assert_eq!(p.provenance["target.host"], BOARD);
        assert_eq!(p.provenance["console.kind"], METHOD);
        assert_eq!(p.runner.name, "native");

        linux.run = None;
        assert!(linux.target.is_none(), "the case the retired guess covered");
        let e = go(
            Verb::Run,
            &linux,
            &cargo_out(None),
            &ImageTables::default(),
            &Invocation::default(),
        )
        .unwrap_err();
        assert!(matches!(e, ResolveError::Unresolved(_)), "{e:?}");
        let msg = e.to_string();
        for t in ["[board.run]", "host = true", "emulator = ", "[board.flash]"] {
            assert!(msg.contains(t), "the refusal names `{t}`: {msg}");
        }

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
                prepare: vec![
                    PrepareStep::Run("./relay.sh 3".into()),
                    PrepareStep::SayThenWait {
                        say: "Press RESET".into(),
                        wait_for: "usb:0483:374b".parse().unwrap(),
                    },
                ],
                ..FlashTable::default()
            }),
            console: Some(ConsoleTable {
                baud: Some(921_600),
                ..ConsoleTable::default()
            }),
            locator: Some("serial/serial@40004800#baudrate=115200".into()),
            net: None,
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
                PlanStep::Run("./relay.sh 3".into()),
                PlanStep::SayThenWait {
                    say: "Press RESET".into(),
                    wait_for: "usb:0483:374b".parse().unwrap(),
                },
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

        // The Pico W's twin is Wokwi, which Zephyr's build does not run, so
        // it is NOT handed to west (which would refuse it after the build).
        // No runner package drives Wokwi yet: refused at plan time, naming
        // the issue (2230). Its silicon flashes through west.
        let pico = board(&cat, "rpi-pico-w-zephyr");
        let e = go(Verb::Run, &pico, &out, &none, &inv).unwrap_err();
        assert!(matches!(e, ResolveError::Unresolved(_)), "{e:?}");
        assert!(
            e.to_string().contains("wokwi:pi-pico-w") && e.to_string().contains("issue 2230"),
            "{e}"
        );
        let p = go(Verb::Flash, &pico, &out, &none, &inv).unwrap();
        assert_eq!(p.runner.name, "west");
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

    /// The qemu runner reads its executable from `plan.tools`: a ladder
    /// answer lands there under the line's own name; no answer leaves the
    /// tool out and says why; a non-QEMU plan is untouched.
    #[test]
    fn attach_tools_hands_the_runner_the_ladders_qemu() {
        use nros_qemu_ladder::QemuBin;
        let cat = BoardCatalog::load(&repo_root()).unwrap();
        let d = board(&cat, "mps2-an385-baremetal");
        let none = ImageTables::default();
        let inv = Invocation::default();
        let mut p = go(
            Verb::Run,
            &d,
            &cargo_out(Some("thumbv7m-none-eabi")),
            &none,
            &inv,
        )
        .unwrap();
        let found = |exe: &str| QemuBin::Found {
            path: PathBuf::from(format!("/store/qemu/bin/{exe}")),
            rung: "store",
        };
        assert!(attach_tools(&mut p, &found).is_empty());
        assert_eq!(
            p.tools["qemu-system-arm"],
            PathBuf::from("/store/qemu/bin/qemu-system-arm")
        );
        assert_eq!(p.provenance["tools.qemu-system-arm"], METHOD);

        p.tools.clear();
        let missing = |_: &str| QemuBin::Missing {
            why: "provision it".into(),
        };
        assert_eq!(
            attach_tools(&mut p, &missing),
            vec!["provision it".to_string()]
        );
        assert!(p.tools.is_empty());

        let mut linux = board(&cat, "linux");
        linux.console = None;
        let mut host = go(Verb::Run, &linux, &cargo_out(None), &none, &inv).unwrap();
        assert!(attach_tools(&mut host, &|_: &str| -> QemuBin { unreachable!() }).is_empty());
        assert!(host.tools.is_empty());
    }
}
