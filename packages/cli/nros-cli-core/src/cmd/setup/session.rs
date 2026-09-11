//! The session's plan, resolved once — RFC-0099 D7 / phase-447 E2.
//!
//! Every `nros setup` invocation that installs a SET — a board's packages, a
//! repeated `--tool`, the lazy `ensure_tools` — goes through here in the same
//! way: RESOLVE all of it first (one index read, every `plan_install`, and one
//! system-package ask for the UNION of what the set's tools declare), then
//! EXECUTE, then FINISH (one lock write, one smoke report, one failure report).
//!
//! Resolution is pure: a probe is injected, so the plan is a function of the
//! index, the host and what the probes answer (the property issue 0374 gave
//! `plan_install`). Execution is sequential in E2. E3 replaces
//! [`run_sequential`] with a pipeline bounded by the CPU count, and the four
//! things that must stay ORDERED under that concurrency are properties of the
//! types here rather than of the loop, so a pipeline cannot break them by
//! accident:
//!
//! 1. **The lock file's single writer.** No step touches the lock.
//!    [`SessionRun::finish`] is the only place that loads, records and saves
//!    it, once, walking the steps in PLAN order.
//! 2. **`front_newest`** (issue 0500: newest-version-first, where a stale entry
//!    shadowing a fresh one prints success on BOTH paths). It runs inside
//!    `sdk_store::execute` and recomputes from the store, so it is idempotent
//!    per tool; what it cannot survive is two concurrent installs of the SAME
//!    tool. [`SessionPlan::resolve`] makes that unrepresentable: a plan holds at
//!    most one step per package name.
//! 3. **`bin_dirs`, folded onto the emitted CMakePreset `PATH` in plan order.**
//!    [`SessionPlan::bin_dirs`] is DERIVED from the step order, never
//!    accumulated by whoever finishes first.
//! 4. **Per-package output in plan order.** Every per-step line goes through an
//!    [`OrderedLog`], which emits a step's lines only once every earlier step
//!    has closed. Sequentially that is live output; under a pipeline it is
//!    buffering, with the same text in the same order.
//!
//! # One ask per SESSION, across processes
//!
//! RFC-0099 D6 keeps `just <platform> setup` as the command a user would run,
//! so a bootstrap is still several `nros setup` processes. The system-package
//! ask is de-duplicated across them by a LEDGER the session's driver declares
//! (`NROS_SETUP_SESSION`, a file the `just setup` recipe or
//! `runner-provision.sh` creates and removes): a key already asked for in this
//! session is not asked again, only a key nobody has asked for yet is. That is
//! the scope of the dedup and all of it — not a cache with a TTL (D6 rejected
//! that because the same command would answer differently depending on WHEN it
//! ran). Outside a driver the variable is unset and every invocation asks its
//! whole set, exactly as before; and `--sudo` never consults the ledger, since
//! "you were told" is not "it is installed".

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use eyre::{Result, bail};

use super::SmokeFailures;
use crate::orchestration::{
    sdk_index::{PrereqContext, PrereqDep, SdkIndex, SourcePackage, ToolPackage},
    sdk_store::{
        InstallAction, Provenance, SdkLock, SourceDisposition, plan_install, provision_source,
        tool_prefix,
    },
};

/// The environment variable naming this session's ledger file.
pub(super) const SESSION_ENV: &str = "NROS_SETUP_SESSION";

// ---------------------------------------------------------------------------
// What a probe says about one `[prereq.*]` key.
// ---------------------------------------------------------------------------

/// One prereq key's state on this host, as the ask needs to know it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PrereqState {
    Present,
    /// Missing, and the only remedy is the OS package manager.
    Missing,
    /// No probe could answer here.
    Unknown,
    /// Missing from the system, and the key's provider chain falls through to
    /// this `[tool.*]` — a sudo-less store install (phase-404 chains). Asking
    /// apt for it would be wrong twice over: it needs sudo when the remedy does
    /// not, and the distro package is usually below the version floor that
    /// made the key a chain in the first place (Ubuntu 22.04's ninja is 1.10,
    /// the floor is 1.13).
    Store(String),
}

// ---------------------------------------------------------------------------
// The ledger.
// ---------------------------------------------------------------------------

/// The keys already asked for in this session — see the module docs.
pub(super) struct SessionLedger {
    path: Option<PathBuf>,
}

impl SessionLedger {
    /// The ledger the session's driver declared, or none (every ask is whole).
    pub(super) fn from_env() -> Self {
        Self {
            path: std::env::var_os(SESSION_ENV)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
        }
    }

    /// A ledger at an explicit path — the tests' spelling, so none of them has
    /// to `set_var` a process-global (issue 1101).
    #[cfg(test)]
    pub(super) fn at(path: &Path) -> Self {
        Self {
            path: Some(path.to_path_buf()),
        }
    }

    /// Keys asked for earlier in this session. A ledger that does not exist
    /// yet is an empty one: the driver creates the variable, the first ask
    /// creates the file.
    pub(super) fn asked(&self) -> BTreeSet<String> {
        let Some(path) = &self.path else {
            return BTreeSet::new();
        };
        std::fs::read_to_string(path)
            .map(|s| {
                s.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Record `keys` as asked. Best-effort: a ledger that cannot be written
    /// costs a repeated line later, which is the pre-E2 behaviour, and must
    /// never fail the provisioning that is printing the ask.
    pub(super) fn record<'k>(&self, keys: impl IntoIterator<Item = &'k str>) {
        use std::io::Write as _;
        let Some(path) = &self.path else { return };
        let keys: Vec<&str> = keys.into_iter().collect();
        if keys.is_empty() {
            return;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path);
        match file {
            Ok(mut f) => {
                for k in keys {
                    let _ = writeln!(f, "{k}");
                }
            }
            Err(e) => eprintln!(
                "nros setup: could not record the ask in {} ({e}); a later step may repeat it",
                path.display()
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// The ask.
// ---------------------------------------------------------------------------

/// The system-package ask for everything one invocation touches — the union,
/// composed once (issue 1274).
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct SystemAsk {
    /// Keys only the OS package manager can provide, each with WHO needs it
    /// (tool names in plan order, or `--system`).
    pub(super) os: BTreeMap<String, Vec<String>>,
    /// Keys a store tool provides without sudo: key -> tool.
    pub(super) store: BTreeMap<String, String>,
    /// Keys whose store tool is itself a step of THIS plan — this session
    /// satisfies them, so they are not asked of anyone.
    pub(super) by_plan: Vec<String>,
    /// Keys asked for earlier in this session and still missing.
    pub(super) already_asked: Vec<String>,
}

impl SystemAsk {
    /// Fold `needs` — `(key, who needs it)` pairs, in the order they were met —
    /// into one ask. `state` answers for a key; it is called once per key.
    ///
    /// `unknown_asks`: whether a key no probe could answer is put in the ask.
    /// `--system`'s print mode says "not confirmed present" and always has; a
    /// tool's `system = [..]` gate only ever acted on a MEASURED miss, and a
    /// gate that blocks an install on "could not tell" would be a new failure.
    pub(super) fn collect<'n>(
        needs: impl IntoIterator<Item = (&'n str, &'n str)>,
        state: &mut dyn FnMut(&str) -> PrereqState,
        planned_tools: &BTreeSet<&str>,
        asked: &BTreeSet<String>,
        unknown_asks: bool,
    ) -> Self {
        let mut ask = Self::default();
        let mut seen: BTreeMap<String, PrereqState> = BTreeMap::new();
        for (key, who) in needs {
            let st = seen
                .entry(key.to_string())
                .or_insert_with(|| state(key))
                .clone();
            match st {
                PrereqState::Present => {}
                PrereqState::Unknown if !unknown_asks => {}
                PrereqState::Store(tool) if planned_tools.contains(tool.as_str()) => {
                    if !ask.by_plan.iter().any(|k| k == key) {
                        ask.by_plan.push(key.to_string());
                    }
                }
                _ if asked.contains(key) => {
                    if !ask.already_asked.iter().any(|k| k == key) {
                        ask.already_asked.push(key.to_string());
                    }
                }
                PrereqState::Store(tool) => {
                    ask.store.insert(key.to_string(), tool);
                }
                PrereqState::Missing | PrereqState::Unknown => {
                    let whos = ask.os.entry(key.to_string()).or_default();
                    if !whos.iter().any(|w| w == who) {
                        whos.push(who.to_string());
                    }
                }
            }
        }
        ask
    }

    /// Nothing new to ask (keys already asked may still be missing).
    pub(super) fn is_empty(&self) -> bool {
        self.os.is_empty() && self.store.is_empty()
    }

    /// The keys this ask puts in front of the user — what the ledger records.
    pub(super) fn asked_keys(&self) -> impl Iterator<Item = &str> {
        self.os.keys().chain(self.store.keys()).map(String::as_str)
    }

    /// The OS packages for `manager`, deduped and sorted — through the index's
    /// per-manager mapping, never a list beside it (RFC-0062).
    pub(super) fn packages(
        &self,
        prereqs: &BTreeMap<String, PrereqDep>,
        manager: &str,
        ctx: &PrereqContext,
    ) -> Vec<String> {
        let entries: Vec<(&String, &PrereqDep)> = prereqs
            .iter()
            .filter(|(k, _)| self.os.contains_key(k.as_str()))
            .collect();
        super::compose_packages(&entries, manager, ctx)
    }

    /// The one native install command for the OS half, if there is one.
    pub(super) fn os_command(
        &self,
        prereqs: &BTreeMap<String, PrereqDep>,
        manager: &str,
        ctx: &PrereqContext,
    ) -> Option<String> {
        let pkgs = self.packages(prereqs, manager, ctx);
        (!pkgs.is_empty()).then(|| super::native_install_command(manager, &pkgs))
    }

    /// The sudo-less half as ONE command — `--tool` is repeatable (E1).
    pub(super) fn store_command(&self) -> Option<String> {
        let tools: BTreeSet<&str> = self.store.values().map(String::as_str).collect();
        (!tools.is_empty()).then(|| {
            let mut cmd = String::from("nros setup");
            for t in tools {
                cmd.push_str(" --tool ");
                cmd.push_str(t);
            }
            cmd
        })
    }

    /// The whole ask as text: one line per key, then ONE command per remedy.
    ///
    /// Built rather than printed so a test can count the commands — the
    /// acceptance is "one `apt install` line", which is a claim about text.
    pub(super) fn render(
        &self,
        prereqs: &BTreeMap<String, PrereqDep>,
        manager: Option<&str>,
        ctx: &PrereqContext,
        headline: &str,
    ) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        if !self.is_empty() {
            let _ = writeln!(out, "{headline}");
            for (key, who) in &self.os {
                let why = prereqs
                    .get(key)
                    .and_then(|d| d.why.as_deref())
                    .unwrap_or("(no why recorded)");
                let by = if who.iter().all(|w| w == "--system") {
                    String::new()
                } else {
                    format!(" [needed by {}]", who.join(", "))
                };
                let _ = writeln!(out, "  {key:<28} {why}{by}");
            }
            for (key, tool) in &self.store {
                let _ = writeln!(
                    out,
                    "  {key:<28} provided by the store's [tool.{tool}] (no sudo)"
                );
            }
        }
        out.push_str(&self.remedy(prereqs, manager, ctx));
        out
    }

    /// Only the remedies — the sudo-less command, then the ONE OS command,
    /// then the note about keys asked earlier. `--system --check` lists its
    /// keys itself and wants just this part.
    ///
    /// The OS command is kept LAST of the commands: `scripts/esp_idf/setup.sh`
    /// reads the tail of this output to show the one line that installs.
    pub(super) fn remedy(
        &self,
        prereqs: &BTreeMap<String, PrereqDep>,
        manager: Option<&str>,
        ctx: &PrereqContext,
    ) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        if let Some(cmd) = self.store_command() {
            let _ = writeln!(out, "Sudo-less, from the store:\n  {cmd}");
        }
        if !self.os.is_empty() {
            match manager {
                Some(mgr) => {
                    let unmapped: Vec<&str> = self
                        .os
                        .keys()
                        .filter(|k| {
                            prereqs
                                .get(k.as_str())
                                .is_none_or(|d| d.packages_for(mgr, ctx).is_empty())
                        })
                        .map(String::as_str)
                        .collect();
                    if !unmapped.is_empty() {
                        let _ = writeln!(
                            out,
                            "  ({} key(s) have no {mgr} mapping and are omitted: {} — map them \
                             in nros-sdk-index.toml)",
                            unmapped.len(),
                            unmapped.join(", ")
                        );
                    }
                    if let Some(cmd) = self.os_command(prereqs, mgr, ctx) {
                        let _ = writeln!(out, "Install with (or re-run with --sudo):\n  {cmd}");
                    }
                }
                None => {
                    let _ = writeln!(
                        out,
                        "  (no supported package manager detected — apt/dnf/pacman/brew; map \
                         your platform in nros-sdk-index.toml)"
                    );
                }
            }
        }
        if !self.already_asked.is_empty() {
            let _ = writeln!(
                out,
                "nros setup: {} system key(s) still missing were asked for earlier in this \
                 session — not repeated: {}",
                self.already_asked.len(),
                self.already_asked.join(", ")
            );
        }
        out
    }
}

// ---------------------------------------------------------------------------
// The plan.
// ---------------------------------------------------------------------------

/// Which invocation the plan serves. It decides two policies, and nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    /// `nros setup <board>`: a best-effort SET. An unavailable tool is fatal —
    /// before any fetch — and a missing system package is ASKED, never a block
    /// (the board path never checked `[tool.*] system` before E2; a dist whose
    /// runtime library is missing fails its smoke check, which says so).
    Board,
    /// `nros setup --tool a --tool b`: exactly the named tools. A tool whose
    /// own `system = [..]` is missing is BLOCKED — not built 40 minutes into a
    /// configure — and the others still install (issue 1038's gate, per tool).
    Tools,
    /// `ensure_tools`, the lazy path under `nros build`: an unavailable tool
    /// warns and is skipped, and no system ask is printed mid-build.
    Lazy,
}

/// One package in the plan.
pub(super) struct Step<'i> {
    pub(super) name: &'i str,
    pub(super) kind: StepKind<'i>,
}

pub(super) enum StepKind<'i> {
    /// A `[tool.*]`, installed into `prefix`.
    Tool {
        tool: &'i ToolPackage,
        prefix: PathBuf,
        action: InstallAction,
        /// Tracked by the lock — false only for an explicit `--prefix`,
        /// which places a tool outside the shared store on purpose.
        locked: bool,
        /// System keys this tool needs and the host lacks, in `Mode::Tools`.
        blocked_on: Vec<String>,
    },
    /// A `[source.*]`, provisioned into its index-declared location.
    Source(&'i SourcePackage),
    /// Gated or not in the index: reported (its disposition, resolved at plan
    /// time), never installed.
    Other(String),
}

/// Everything one invocation will touch, resolved before any fetch.
pub(super) struct SessionPlan<'i> {
    pub(super) mode: Mode,
    pub(super) steps: Vec<Step<'i>>,
    pub(super) system: SystemAsk,
}

/// What resolution reads besides the index.
pub(super) struct PlanInputs<'a> {
    pub(super) root: &'a Path,
    pub(super) host: &'a str,
    /// `--prefix`, only ever with a single `--tool` (the caller refuses more).
    pub(super) prefix_override: Option<&'a Path>,
    pub(super) mode: Mode,
}

impl<'i> SessionPlan<'i> {
    /// Resolve `names` — in the order given, first occurrence wins — into a
    /// plan. `state` answers for a `[prereq.*]` key; `asked` is the ledger.
    ///
    /// In `Mode::Tools` a name with no `[tool.*]` is refused HERE, so a typo in
    /// the third `--tool` fails before the first one downloads.
    pub(super) fn resolve(
        index: &'i SdkIndex,
        names: &[&'i str],
        inputs: &PlanInputs<'_>,
        state: &mut dyn FnMut(&str) -> PrereqState,
        asked: &BTreeSet<String>,
    ) -> Result<Self> {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut steps = Vec::new();
        for &name in names {
            if !seen.insert(name) {
                continue;
            }
            let kind = if let Some(tool) = index.tool.get(name) {
                let prefix = inputs
                    .prefix_override
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| tool_prefix(inputs.root, name, &tool.version));
                let action = plan_install(tool, inputs.host, &prefix);
                StepKind::Tool {
                    tool,
                    prefix,
                    action,
                    locked: inputs.prefix_override.is_none(),
                    blocked_on: Vec::new(),
                }
            } else if inputs.mode == Mode::Tools {
                bail!("nros setup --tool: no [tool.{name}] in the index (see `nros setup --list`)");
            } else if let Some(src) = index.source.get(name) {
                StepKind::Source(src)
            } else {
                StepKind::Other(super::disposition(index, name, inputs.host))
            };
            steps.push(Step { name, kind });
        }

        let mut plan = Self {
            mode: inputs.mode,
            steps,
            system: SystemAsk::default(),
        };
        if plan.mode == Mode::Lazy {
            return Ok(plan);
        }

        // The union. Only tools this plan will INSTALL contribute: a present
        // tool is not re-examined (RFC-0099 D6 keeps a repeat cheap and quiet;
        // `--check` is the verb that asks about what is already there).
        let needs: Vec<(&str, &str)> = plan
            .steps
            .iter()
            .filter_map(|s| match &s.kind {
                StepKind::Tool { tool, action, .. } if will_install(action) => Some(
                    tool.system
                        .iter()
                        .map(move |k| (k.as_str(), s.name))
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .flatten()
            .collect();
        let planned: BTreeSet<&str> = plan
            .steps
            .iter()
            .filter(|s| matches!(&s.kind, StepKind::Tool { action, .. } if will_install(action)))
            .map(|s| s.name)
            .collect();
        let mut memo: BTreeMap<String, PrereqState> = BTreeMap::new();
        let mut cached = |k: &str| {
            memo.entry(k.to_string())
                .or_insert_with(|| state(k))
                .clone()
        };
        plan.system =
            SystemAsk::collect(needs.iter().copied(), &mut cached, &planned, asked, false);

        if plan.mode == Mode::Tools {
            for step in &mut plan.steps {
                if let StepKind::Tool {
                    tool,
                    action,
                    blocked_on,
                    ..
                } = &mut step.kind
                    && will_install(action)
                {
                    *blocked_on = tool
                        .system
                        .iter()
                        .filter(|k| {
                            matches!(
                                cached(k.as_str()),
                                PrereqState::Missing | PrereqState::Store(_)
                            ) && !plan.system.by_plan.contains(k)
                        })
                        .cloned()
                        .collect();
                }
            }
        }
        Ok(plan)
    }

    /// Store `bin/` dirs in PLAN order — derived, never accumulated, so the
    /// CMakePreset `PATH` cannot depend on which install finished first.
    pub(super) fn bin_dirs(&self) -> Vec<PathBuf> {
        self.steps
            .iter()
            .filter_map(|s| match &s.kind {
                StepKind::Tool { prefix, action, .. }
                    if !matches!(action, InstallAction::Unavailable) =>
                {
                    Some(prefix.join("bin"))
                }
                _ => None,
            })
            .collect()
    }

    /// Tools this host will BUILD FROM SOURCE (issue 0374's heads-up).
    pub(super) fn source_builds(&self) -> Vec<&'i str> {
        self.steps
            .iter()
            .filter(|s| {
                matches!(
                    &s.kind,
                    StepKind::Tool {
                        action: InstallAction::Source { .. },
                        ..
                    }
                )
            })
            .map(|s| s.name)
            .collect()
    }

    /// Tools with no prebuilt for this host and no source recipe.
    pub(super) fn unavailable(&self) -> Vec<(&'i str, &'i str)> {
        self.steps
            .iter()
            .filter_map(|s| match &s.kind {
                StepKind::Tool {
                    tool,
                    action: InstallAction::Unavailable,
                    ..
                } => Some((s.name, tool.version.as_str())),
                _ => None,
            })
            .collect()
    }
}

fn will_install(action: &InstallAction) -> bool {
    matches!(
        action,
        InstallAction::Prebuilt { .. } | InstallAction::Source { .. }
    )
}

// ---------------------------------------------------------------------------
// Plan-order output.
// ---------------------------------------------------------------------------

/// Emits per-step lines in PLAN order, whatever order the steps finish in.
///
/// Pure: [`OrderedLog::push`] and [`OrderedLog::close`] RETURN the lines that
/// are ready, and the caller prints them. A step's lines are ready once every
/// earlier step has closed — so a sequential run streams live, and a pipelined
/// one (E3) buffers without reordering.
#[derive(Default)]
pub(super) struct OrderedLog {
    /// The first step not yet closed; its lines stream straight through.
    head: usize,
    pending: BTreeMap<usize, Vec<String>>,
    closed: BTreeSet<usize>,
}

impl OrderedLog {
    pub(super) fn push(&mut self, step: usize, line: String) -> Vec<String> {
        if step == self.head {
            vec![line]
        } else {
            self.pending.entry(step).or_default().push(line);
            Vec::new()
        }
    }

    pub(super) fn close(&mut self, step: usize) -> Vec<String> {
        self.closed.insert(step);
        let mut ready = Vec::new();
        while self.closed.remove(&self.head) {
            self.head += 1;
            // The new head's buffered lines are ready, and from here it streams.
            if let Some(lines) = self.pending.remove(&self.head) {
                ready.extend(lines);
            }
        }
        ready
    }
}

// ---------------------------------------------------------------------------
// Execution.
// ---------------------------------------------------------------------------

/// What one step produced. Nothing here touches the lock.
#[derive(Default)]
pub(super) struct Outcome {
    pub(super) provenance: Option<Provenance>,
    pub(super) installed: bool,
    pub(super) smoke: SmokeFailures,
    pub(super) error: Option<String>,
}

/// A plan being executed: outcomes land in any order, output leaves in plan
/// order, and [`SessionRun::finish`] is the one writer of the lock.
pub(super) struct SessionRun<'p, 'i> {
    plan: &'p SessionPlan<'i>,
    log: OrderedLog,
    outcomes: Vec<Option<Outcome>>,
}

/// What a finished run amounts to.
#[derive(Default)]
pub(super) struct SessionReport {
    pub(super) installed: bool,
    pub(super) lock_written: bool,
    pub(super) smoke: SmokeFailures,
    /// `(package, error)`, in plan order.
    pub(super) errors: Vec<(String, String)>,
}

impl<'p, 'i> SessionRun<'p, 'i> {
    pub(super) fn new(plan: &'p SessionPlan<'i>) -> Self {
        Self {
            plan,
            log: OrderedLog::default(),
            outcomes: plan.steps.iter().map(|_| None).collect(),
        }
    }

    /// A per-step line, routed through the plan-order log.
    pub(super) fn say(&mut self, step: usize, line: String) {
        for l in self.log.push(step, line) {
            eprintln!("{l}");
        }
    }

    pub(super) fn complete(&mut self, step: usize, outcome: Outcome) {
        self.outcomes[step] = Some(outcome);
        for l in self.log.close(step) {
            eprintln!("{l}");
        }
    }

    /// The single writer: walk the steps in PLAN order, record every locked
    /// provenance, save the lock ONCE, and fold smoke verdicts and failures in
    /// the same order. Recorded before any verdict on purpose — the files are
    /// on disk whatever the probes say, and a lock that omitted them would make
    /// the next run re-download the same broken dist to reach the same answer.
    pub(super) fn finish(self, lock_path: Option<&Path>) -> Result<SessionReport> {
        let mut report = SessionReport::default();
        let mut lock: Option<SdkLock> = None;
        for (step, outcome) in self.plan.steps.iter().zip(self.outcomes) {
            let Some(outcome) = outcome else {
                bail!(
                    "nros setup: `{}` was planned and never completed — the executor dropped a \
                     step",
                    step.name
                );
            };
            if let (Some(prov), StepKind::Tool { locked: true, .. }, Some(path)) =
                (&outcome.provenance, &step.kind, lock_path)
            {
                if lock.is_none() {
                    lock = Some(SdkLock::load(path)?);
                }
                if let Some(l) = lock.as_mut() {
                    l.record(step.name, prov);
                }
            }
            report.installed |= outcome.installed;
            report.smoke.absorb(outcome.smoke);
            if let Some(e) = outcome.error {
                report.errors.push((step.name.to_string(), e));
            }
        }
        if let (Some(l), Some(path)) = (lock, lock_path) {
            l.save(path)?;
            report.lock_written = true;
        }
        Ok(report)
    }
}

impl SessionReport {
    /// Print the smoke report and every failure, together; non-zero when
    /// anything failed. Collected, never raised mid-run: a set of twenty is not
    /// hostage to its one bad package (phase-447 C1).
    pub(super) fn conclude(self, index: &SdkIndex) -> Result<()> {
        let text = self.smoke.render(index);
        if !text.is_empty() {
            eprint!("{text}");
        }
        for (name, err) in &self.errors {
            eprintln!("  [FAILED]  {name} — {err}");
        }
        let broken = self.smoke.broken();
        match (self.errors.len(), broken) {
            (0, 0) => Ok(()),
            (0, b) => bail!("{b} newly installed package(s) failed their smoke check"),
            (e, 0) => bail!("{e} package(s) failed to install (see [FAILED] above)"),
            (e, b) => bail!(
                "{e} package(s) failed to install (see [FAILED] above); {b} newly installed \
                 package(s) failed their smoke check"
            ),
        }
    }
}

/// Execute `plan` one step at a time, in plan order. E3 replaces this loop and
/// nothing else.
///
/// Every `[tool.*]` install goes through [`SmokeFailures::execute_and_probe`] —
/// the ONE place installing and asking "does it run?" are paired. Reaching
/// `sdk_store::execute` from here without it is the regression
/// `a_board_install_smokes_every_package_and_a_broken_one_stops_nothing`
/// exists to catch.
pub(super) fn run_sequential<'p, 'i>(
    plan: &'p SessionPlan<'i>,
    workspace: &Path,
    shallow: Option<bool>,
    dry_run: bool,
) -> SessionRun<'p, 'i> {
    let host = crate::orchestration::sdk_index::host_key();
    let mut run = SessionRun::new(plan);
    for (i, step) in plan.steps.iter().enumerate() {
        let mut out = Outcome::default();
        match &step.kind {
            StepKind::Tool {
                tool,
                prefix,
                action,
                blocked_on,
                ..
            } => {
                // The lazy path runs under `nros build`: it speaks only when it
                // does something, never to list what is already there.
                if plan.mode != Mode::Lazy {
                    let what = super::describe(action, &tool.version, &host);
                    run.say(i, tool_line(plan.mode, step.name, &what, prefix));
                }
                if dry_run {
                    run.complete(i, out);
                    continue;
                }
                match action {
                    InstallAction::Present => {}
                    InstallAction::Unavailable if plan.mode == Mode::Lazy => {
                        run.say(
                            i,
                            format!(
                                "nros: {} {} unavailable for {host} (no prebuilt, no source) — \
                                 install it yourself if the build needs it",
                                step.name, tool.version
                            ),
                        );
                    }
                    InstallAction::Unavailable => {
                        out.error = Some(format!(
                            "{} has no prebuilt for {host} and no source recipe",
                            tool.version
                        ));
                    }
                    _ if !blocked_on.is_empty() => {
                        out.error = Some(format!(
                            "not installed — needs system package(s) this host is missing: {} \
                             (the one ask for them is printed above; declared as \
                             [tool.{}] system = [..])",
                            blocked_on.join(", "),
                            step.name
                        ));
                    }
                    other => {
                        if plan.mode == Mode::Lazy {
                            run.say(
                                i,
                                format!(
                                    "nros: auto-installing {} {} (set NROS_NO_AUTO_SETUP to skip)",
                                    step.name, tool.version
                                ),
                            );
                        }
                        match out.smoke.execute_and_probe(other, step.name, tool, prefix) {
                            Ok(prov) => {
                                out.provenance = Some(prov);
                                out.installed = true;
                                run.say(i, format!("    → {}", prefix.display()));
                            }
                            Err(e) => {
                                out.error = Some(format!("install {}: {e:#}", tool.version));
                            }
                        }
                    }
                }
            }
            StepKind::Source(src) => {
                match provision_source(step.name, src, workspace, dry_run, shallow) {
                    Ok(disp) => {
                        out.installed = matches!(disp, SourceDisposition::Provisioned);
                        if plan.mode != Mode::Lazy || out.installed {
                            let text = super::describe_source(step.name, src, workspace, &disp);
                            run.say(i, source_line(plan.mode, step.name, &text));
                        }
                    }
                    Err(e) => {
                        let msg = format!("provision source: {e:#}");
                        if plan.mode == Mode::Lazy {
                            // Best-effort, as before: the build names the miss.
                            run.say(
                                i,
                                format!(
                                    "nros: source {} provisioning failed ({msg}) — provide it \
                                     yourself if the build needs it",
                                    step.name
                                ),
                            );
                        } else {
                            out.error = Some(msg);
                        }
                    }
                }
            }
            StepKind::Other(disposition) => {
                if plan.mode != Mode::Lazy {
                    run.say(i, format!("  {:<22} {disposition}", step.name));
                }
            }
        }
        run.complete(i, out);
    }
    run
}

fn tool_line(mode: Mode, name: &str, what: &str, prefix: &Path) -> String {
    match mode {
        Mode::Tools => format!("nros setup --tool {name}: {what} → {}", prefix.display()),
        Mode::Board | Mode::Lazy => format!("  {name:<22} {what}"),
    }
}

fn source_line(mode: Mode, name: &str, text: &str) -> String {
    match mode {
        Mode::Lazy => format!("nros: {name}: {text}"),
        Mode::Board | Mode::Tools => format!("  {name:<22} {text}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::sdk_index::host_key;

    fn index(raw: &str) -> SdkIndex {
        SdkIndex::parse(raw).unwrap()
    }

    fn no_asks() -> BTreeSet<String> {
        BTreeSet::new()
    }

    fn inputs<'a>(root: &'a Path, host: &'a str, mode: Mode) -> PlanInputs<'a> {
        PlanInputs {
            root,
            host,
            prefix_override: None,
            mode,
        }
    }

    // ---- the ask: one, for the union ---------------------------------------

    /// Acceptance for issue 1274, at the level it can be measured: three tools
    /// whose `system` lists overlap produce ONE install command naming the
    /// union, each package once.
    #[test]
    fn a_plan_asks_once_for_the_union_of_what_its_tools_declare() {
        let idx = index(
            "[prereq.k1]\napt = [\"p1\"]\n[prereq.k2]\napt = [\"p2\"]\n[prereq.k3]\napt = [\"p3\"]\n\
             [tool.a]\nversion = \"1\"\nsystem = [\"k1\", \"k2\"]\n\
             [tool.a.source]\ngit = \"x\"\nref = \"y\"\n\
             [tool.b]\nversion = \"1\"\nsystem = [\"k2\", \"k3\"]\n\
             [tool.b.source]\ngit = \"x\"\nref = \"y\"\n\
             [tool.c]\nversion = \"1\"\nsystem = [\"k3\"]\n\
             [tool.c.source]\ngit = \"x\"\nref = \"y\"\n",
        );
        let root = crate::test_support::scratch_dir("e2_union");
        let host = host_key();
        let mut calls = Vec::new();
        let plan = SessionPlan::resolve(
            &idx,
            &["a", "b", "c"],
            &inputs(&root, &host, Mode::Board),
            &mut |k| {
                calls.push(k.to_string());
                PrereqState::Missing
            },
            &no_asks(),
        )
        .unwrap();

        // Each key is probed once, however many tools name it.
        calls.sort();
        assert_eq!(calls, ["k1", "k2", "k3"]);
        assert_eq!(
            plan.system.os.keys().collect::<Vec<_>>(),
            ["k1", "k2", "k3"]
        );
        assert_eq!(
            plan.system.os["k2"],
            ["a", "b"],
            "who needs it, in plan order"
        );

        let prereqs = idx.prereqs();
        let ctx = PrereqContext::from_env();
        let text = plan.system.render(&prereqs, Some("apt"), &ctx, "ask:");
        let commands: Vec<&str> = text
            .lines()
            .filter(|l| l.trim_start().starts_with("sudo apt-get install"))
            .collect();
        assert_eq!(commands.len(), 1, "ONE install line:\n{text}");
        for p in ["p1", "p2", "p3"] {
            // Twice per command: install, then the retry behind `apt-get update`.
            assert_eq!(commands[0].matches(p).count(), 2, "{p} in:\n{text}");
        }
    }

    /// The first of the three lines issue 1274 measured asked apt for
    /// `make ninja-build`, which the SAME session then provisioned itself from
    /// the store. A key whose chain falls through to a tool this plan installs
    /// is satisfied by the plan and asked of nobody; one whose tool is NOT in
    /// the plan is offered as the sudo-less command, never as an apt package.
    #[test]
    fn a_key_the_store_provides_is_never_asked_of_the_os() {
        let idx = index(
            "[prereq.unzip]\napt = [\"unzip\"]\n\
             [prereq.ninja]\napt = [\"ninja-build\"]\n\
             [tool.ninja]\nversion = \"1\"\nsystem = [\"unzip\"]\n\
             [tool.ninja.source]\ngit = \"x\"\nref = \"y\"\n\
             [tool.widget]\nversion = \"1\"\nsystem = [\"ninja\", \"make\"]\n\
             [tool.widget.source]\ngit = \"x\"\nref = \"y\"\n",
        );
        let root = crate::test_support::scratch_dir("e2_store_routed");
        let host = host_key();
        let state = |k: &str| match k {
            "ninja" => PrereqState::Store("ninja".into()),
            "make" => PrereqState::Store("make".into()),
            _ => PrereqState::Missing,
        };
        let plan = SessionPlan::resolve(
            &idx,
            &["ninja", "widget"],
            &inputs(&root, &host, Mode::Board),
            &mut { state },
            &no_asks(),
        )
        .unwrap();
        assert_eq!(
            plan.system.by_plan,
            ["ninja"],
            "this session installs ninja"
        );
        assert_eq!(
            plan.system.store.get("make").map(String::as_str),
            Some("make")
        );
        assert!(!plan.system.os.contains_key("ninja"));
        assert!(!plan.system.os.contains_key("make"));
        let text = plan.system.render(
            &idx.prereqs(),
            Some("apt"),
            &PrereqContext::from_env(),
            "ask:",
        );
        assert!(!text.contains("ninja-build"), "{text}");
        assert!(text.contains("nros setup --tool make"), "{text}");
    }

    /// Across PROCESSES: a key asked for earlier in the session is not asked
    /// again, and a key nobody has asked for yet still is — the ledger dedups,
    /// it never hides.
    #[test]
    fn a_key_asked_earlier_in_the_session_is_not_asked_again() {
        let dir = crate::test_support::scratch_dir("e2_ledger");
        let ledger = SessionLedger::at(&dir.join("asked"));
        assert!(ledger.asked().is_empty(), "no file yet is an empty ledger");
        ledger.record(["k1", "k2"]);

        let mut state = |_: &str| PrereqState::Missing;
        let ask = SystemAsk::collect(
            [("k1", "--system"), ("k2", "--system"), ("k3", "--system")],
            &mut state,
            &BTreeSet::new(),
            &ledger.asked(),
            false,
        );
        assert_eq!(ask.os.keys().collect::<Vec<_>>(), ["k3"]);
        assert_eq!(ask.already_asked, ["k1", "k2"]);
        ledger.record(ask.asked_keys());
        assert_eq!(
            ledger.asked().into_iter().collect::<Vec<_>>(),
            ["k1", "k2", "k3"]
        );

        // The third process of the session: nothing new, and it says so in ONE
        // line rather than repeating the command.
        let again = SystemAsk::collect(
            [("k1", "--system"), ("k3", "--system")],
            &mut state,
            &BTreeSet::new(),
            &ledger.asked(),
            false,
        );
        assert!(again.is_empty());
        let idx = index("[prereq.k1]\napt = [\"p1\"]\n[prereq.k3]\napt = [\"p3\"]\n");
        let text = again.render(
            &idx.prereqs(),
            Some("apt"),
            &PrereqContext::from_env(),
            "ask:",
        );
        assert!(!text.contains("apt-get"), "{text}");
        assert!(text.contains("asked for earlier in this session"), "{text}");
    }

    /// No driver, no ledger: every invocation asks its whole set, as before.
    #[test]
    fn without_a_session_nothing_is_deduplicated() {
        let ledger = SessionLedger { path: None };
        ledger.record(["k1"]);
        assert!(ledger.asked().is_empty());
    }

    // ---- the plan's structure ----------------------------------------------

    /// One step per package, in first-seen order — which is what keeps two
    /// concurrent installs of one tool (and so two `front_newest` runs racing
    /// for its link) unrepresentable under E3 — and `bin_dirs` in that order.
    #[test]
    fn a_plan_holds_each_package_once_and_derives_bin_dirs_in_plan_order() {
        let idx = index(
            "[tool.z]\nversion = \"1\"\n[tool.z.source]\ngit = \"x\"\nref = \"y\"\n\
             [tool.a]\nversion = \"2\"\n[tool.a.source]\ngit = \"x\"\nref = \"y\"\n",
        );
        let root = crate::test_support::scratch_dir("e2_order");
        let host = host_key();
        let plan = SessionPlan::resolve(
            &idx,
            &["z", "a", "z", "gated-thing"],
            &inputs(&root, &host, Mode::Board),
            &mut |_| PrereqState::Present,
            &no_asks(),
        )
        .unwrap();
        let names: Vec<&str> = plan.steps.iter().map(|s| s.name).collect();
        assert_eq!(names, ["z", "a", "gated-thing"]);
        assert_eq!(
            plan.bin_dirs(),
            [root.join("z/1/bin"), root.join("a/2/bin")],
            "plan order, not alphabetical and not completion order"
        );
        assert_eq!(plan.source_builds(), ["z", "a"]);
    }

    /// A typo in the THIRD `--tool` fails before the first one downloads.
    #[test]
    fn an_unknown_tool_is_refused_before_anything_is_fetched() {
        let idx = index("[tool.a]\nversion = \"1\"\n[tool.a.source]\ngit = \"x\"\nref = \"y\"\n");
        let root = crate::test_support::scratch_dir("e2_unknown_tool");
        let host = host_key();
        let Err(err) = SessionPlan::resolve(
            &idx,
            &["a", "a", "nope"],
            &inputs(&root, &host, Mode::Tools),
            &mut |_| PrereqState::Present,
            &no_asks(),
        ) else {
            panic!("an unknown --tool must be refused at plan time");
        };
        assert!(format!("{err}").contains("[tool.nope]"), "{err}");
    }

    /// Output in plan order, whatever order steps finish in — E3's fourth
    /// ordered thing, pinned before E3 exists.
    #[test]
    fn output_leaves_in_plan_order_whatever_order_steps_finish_in() {
        let mut log = OrderedLog::default();
        let mut seen: Vec<String> = Vec::new();
        // Step 0 streams live.
        seen.extend(log.push(0, "0a".into()));
        // Steps 2 and 1 run concurrently and 2 finishes first.
        seen.extend(log.push(2, "2a".into()));
        seen.extend(log.push(1, "1a".into()));
        seen.extend(log.close(2));
        assert_eq!(seen, ["0a"], "nothing of step 1 or 2 may pass step 0");
        seen.extend(log.push(0, "0b".into()));
        seen.extend(log.close(0));
        seen.extend(log.push(1, "1b".into()));
        seen.extend(log.close(1));
        assert_eq!(seen, ["0a", "0b", "1a", "1b", "2a"]);

        // Sequential completion is live streaming: nothing is ever held.
        let mut seq = OrderedLog::default();
        for i in 0..3 {
            assert_eq!(seq.push(i, format!("{i}")), [format!("{i}")]);
            assert!(seq.close(i).is_empty());
        }
    }

    // ---- execution, through the real install path --------------------------

    /// Pack `bin/widget` printing `says` into a `file://` dist; `(url, sha256)`.
    #[cfg(unix)]
    fn pack(dir: &Path, tag: &str, says: &str) -> (String, String) {
        let stage = dir.join(format!("stage-{tag}"));
        std::fs::create_dir_all(stage.join("bin")).unwrap();
        crate::test_support::write_executable_stub(
            &stage.join("bin").join("widget"),
            &format!("#!/bin/sh\necho '{says}'\n"),
        );
        let archive = dir.join(format!("{tag}.tar.gz"));
        let ok = std::process::Command::new("tar")
            .args([
                "-czf".as_ref(),
                archive.as_os_str(),
                "-C".as_ref(),
                stage.as_os_str(),
                ".".as_ref(),
            ])
            .status()
            .expect("spawn tar (the install path itself needs it)");
        assert!(ok.success());
        let out = std::process::Command::new("sha256sum")
            .arg(&archive)
            .output()
            .expect("spawn sha256sum (the install path itself needs it)");
        let sha = String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string();
        (format!("file://{}", archive.display()), sha)
    }

    #[cfg(unix)]
    fn widget_entry(name: &str, url: &str, sha: &str) -> String {
        format!(
            "[tool.{name}]\nversion = \"1.0\"\n\
             dist.{host} = {{ url = \"{url}\", sha256 = \"{sha}\" }}\n\
             smoke = [{{ run = \"bin/widget --version\", expect = \"widget 1.0\" }}]\n",
            host = host_key(),
        )
    }

    /// The test C1's mutation pass found missing — a BOARD set containing a
    /// package whose smoke fails:
    ///
    /// (a) every package is smoked, the broken one included — so the set's
    ///     install loop cannot reach `execute` without `execute_and_probe`;
    /// (b) the broken one stops nothing: the package AFTER it installs, the
    ///     lock records all three (the files are on disk), and the verdict is
    ///     collected and reported at the end, non-zero.
    ///
    /// It runs the real executor over real `file://` dists — download, sha256,
    /// `tar -xf`, provenance — into a scratch store with a scratch lock, so no
    /// process-global is touched.
    #[cfg(unix)]
    #[test]
    fn a_board_install_smokes_every_package_and_a_broken_one_stops_nothing() {
        let dir = crate::test_support::scratch_dir("e2_board_smoke");
        let (good_url, good_sha) = pack(&dir, "good", "widget 1.0");
        let (bad_url, bad_sha) = pack(&dir, "bad", "widget: error while loading shared libraries");
        let idx = index(&format!(
            "{}{}{}",
            widget_entry("first-ok", &good_url, &good_sha),
            widget_entry("broken", &bad_url, &bad_sha),
            widget_entry("later-ok", &good_url, &good_sha),
        ));
        let root = dir.join("store");
        let lock = dir.join("nros-sdk.lock");
        let host = host_key();
        let plan = SessionPlan::resolve(
            &idx,
            &["first-ok", "broken", "later-ok"],
            &inputs(&root, &host, Mode::Board),
            &mut |_| PrereqState::Present,
            &no_asks(),
        )
        .unwrap();

        let report = run_sequential(&plan, &dir, None, false)
            .finish(Some(&lock))
            .unwrap();

        // (a) smoke ran on EVERY package.
        assert_eq!(report.smoke.passed, ["first-ok", "later-ok"]);
        assert_eq!(
            report.smoke.broken(),
            1,
            "the broken one was probed and failed"
        );
        // (b) nothing was aborted: the package after the broken one is there…
        assert!(root.join("later-ok/1.0/bin/widget").is_file());
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        // …the lock was written once, with all three…
        assert!(report.lock_written);
        let locked = SdkLock::load(&lock).unwrap();
        assert_eq!(
            locked.tool.keys().map(String::as_str).collect::<Vec<_>>(),
            ["broken", "first-ok", "later-ok"]
        );
        // …and the verdict is collected, reported at the end, non-zero.
        let err = report
            .conclude(&idx)
            .expect_err("a broken package must fail the run");
        assert!(
            format!("{err:#}").contains("failed their smoke check"),
            "{err:#}"
        );
    }

    /// A `--tool` blocked on a missing system package is not installed, and
    /// the tools beside it still are.
    #[cfg(unix)]
    #[test]
    fn a_tool_blocked_on_a_system_package_does_not_stop_the_others() {
        let dir = crate::test_support::scratch_dir("e2_tools_blocked");
        let (url, sha) = pack(&dir, "good", "widget 1.0");
        let idx = index(&format!(
            "[prereq.libfoo]\napt = [\"libfoo1\"]\n{}{}system = [\"libfoo\"]\n",
            widget_entry("fine", &url, &sha),
            widget_entry("needy", &url, &sha),
        ));
        let root = dir.join("store");
        let host = host_key();
        let plan = SessionPlan::resolve(
            &idx,
            &["needy", "fine"],
            &inputs(&root, &host, Mode::Tools),
            &mut |_| PrereqState::Missing,
            &no_asks(),
        )
        .unwrap();
        assert_eq!(plan.system.os["libfoo"], ["needy"]);

        let report = run_sequential(&plan, &dir, None, false)
            .finish(Some(&dir.join("lock")))
            .unwrap();
        assert!(
            root.join("fine/1.0/bin/widget").is_file(),
            "fine still installs"
        );
        assert!(!root.join("needy/1.0").exists(), "needy is never fetched");
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].0, "needy");
        assert!(report.errors[0].1.contains("libfoo"), "{:?}", report.errors);
    }

    /// The lock is written once, by `finish`, whatever order steps complete in
    /// — the single writer does not depend on the executor being sequential.
    #[test]
    fn the_lock_has_one_writer_and_it_walks_plan_order() {
        let idx = index(
            "[tool.a]\nversion = \"1\"\n[tool.a.source]\ngit = \"x\"\nref = \"y\"\n\
             [tool.b]\nversion = \"2\"\n[tool.b.source]\ngit = \"x\"\nref = \"y\"\n",
        );
        let dir = crate::test_support::scratch_dir("e2_single_writer");
        let host = host_key();
        let plan = SessionPlan::resolve(
            &idx,
            &["a", "b"],
            &inputs(&dir, &host, Mode::Board),
            &mut |_| PrereqState::Present,
            &no_asks(),
        )
        .unwrap();
        let prov = |v: &str| Provenance {
            kind: crate::orchestration::sdk_store::ProvenanceKind::Source,
            version: v.into(),
            sha256: None,
        };
        let lock = dir.join("lock");
        let mut run = SessionRun::new(&plan);
        // Completed out of order, as a pipeline would.
        run.complete(
            1,
            Outcome {
                provenance: Some(prov("2")),
                installed: true,
                ..Outcome::default()
            },
        );
        assert!(!lock.exists(), "a step never writes the lock");
        run.complete(
            0,
            Outcome {
                provenance: Some(prov("1")),
                installed: true,
                ..Outcome::default()
            },
        );
        assert!(!lock.exists(), "a step never writes the lock");
        let report = run.finish(Some(&lock)).unwrap();
        assert!(report.lock_written);
        let l = SdkLock::load(&lock).unwrap();
        assert_eq!(l.tool["a"].version, "1");
        assert_eq!(l.tool["b"].version, "2");
    }

    /// A step the executor never completed is an error, not a silent gap in
    /// the lock.
    #[test]
    fn a_dropped_step_is_refused_at_finish() {
        let idx = index("[tool.a]\nversion = \"1\"\n[tool.a.source]\ngit = \"x\"\nref = \"y\"\n");
        let dir = crate::test_support::scratch_dir("e2_dropped");
        let host = host_key();
        let plan = SessionPlan::resolve(
            &idx,
            &["a"],
            &inputs(&dir, &host, Mode::Board),
            &mut |_| PrereqState::Present,
            &no_asks(),
        )
        .unwrap();
        let Err(err) = SessionRun::new(&plan).finish(None) else {
            panic!("an uncompleted step must be refused");
        };
        assert!(format!("{err}").contains("never completed"), "{err}");
    }
}
