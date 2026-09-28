//! phase-291 (#211) — the canonical zephyr-leaf Kconfig→`rustc-env` bake.
//!
//! Every zephyr Rust leaf (standalone example or workspace `zephyr_entry`)
//! calls [`bake_nros_config`] from its `build.rs`, collapsing the previously
//! copy-pasted ~81-line file to:
//!
//! ```ignore
//! fn main() {
//!     zephyr_build::export_kconfig_bool_options(); // Kconfig→cfg bridge (phase-92.4)
//!     nros_zephyr_build::bake_nros_config();       // #17 locator/domain + 0163 XRCE bake
//! }
//! ```
//!
//! What it bakes (known-issue #17): `nros::main!`'s Zephyr branch and
//! `nros::zephyr_component_main!` read `option_env!("NROS_LOCATOR")` /
//! `option_env!("NROS_DOMAIN_ID")` at compile time; without a baked value the
//! locator falls back to EMPTY → zenoh-pico multicast scouting, which
//! native_sim NSOS can never satisfy (no `connect()` is ever issued). The C
//! API path consumes `CONFIG_NROS_ZENOH_LOCATOR` from Kconfig directly; this
//! helper re-exports the same Kconfig values so Kconfig stays the single
//! source of truth for BOTH languages. (`DOTCONFIG` — the generated
//! `.config` path — is exported by the Zephyr build system.)
//!
//! The bake MUST run in the LEAF's own `build.rs`: `cargo:rustc-env` from a
//! dependency's build script never reaches other crates' compilation, and the
//! `option_env!` reads expand in the leaf. That is why this is a shared
//! build-DEPENDENCY, not logic inside a runtime crate.
//!
//! Zero dependencies by design: upstream `zephyr-build` resolves as a
//! west-module PATH dep only (a leaf `Cargo.lock` entry with no `source =`),
//! so depending on it here would break host `cargo check --workspace`. The
//! `export_kconfig_bool_options()` call therefore stays in the leaf.

use std::{env, fs};

/// Bake the nros Kconfig values into `rustc-env` directives:
///
/// - `CONFIG_NROS_ZENOH_LOCATOR` → `NROS_LOCATOR` (quoted string, phase-225)
/// - `CONFIG_NROS_DOMAIN_ID` → `NROS_DOMAIN_ID` (integer, issue 0161)
/// - issue 0163 — when `CONFIG_NROS_RMW_XRCE=y`, synthesize the `host:port`
///   agent locator from `CONFIG_NROS_XRCE_AGENT_{ADDR,PORT}` (defaults
///   `127.0.0.1:2018`) into the SAME `NROS_LOCATOR` env (mutually exclusive
///   with the zenoh bake — an image selects exactly one RMW). Self-gated, so
///   zenoh-only images (and workspace entries) are unaffected.
///
/// No-op (beyond `rerun-if-env-changed`) when `DOTCONFIG` is unset or the
/// Kconfigs are absent/empty — a host `cargo check` of a leaf stays quiet.
pub fn bake_nros_config() {
    println!("cargo:rerun-if-env-changed=DOTCONFIG");
    println!("cargo:rerun-if-env-changed=NROS_LOCATOR");
    println!("cargo:rerun-if-env-changed=NROS_DOMAIN_ID");
    let Some(body) = env::var("DOTCONFIG").ok().and_then(|p| {
        println!("cargo:rerun-if-changed={p}");
        fs::read_to_string(&p).ok()
    }) else {
        return;
    };
    for line in bake_directives(&body) {
        println!("{line}");
    }
}

/// Pure core of [`bake_nros_config`]: `.config` body → the `cargo:rustc-env`
/// directive lines. Split out so tests assert emission without a cargo run.
fn bake_directives(dotconfig: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(val) = kconfig_str(dotconfig, "CONFIG_NROS_ZENOH_LOCATOR") {
        out.push(format!("cargo:rustc-env=NROS_LOCATOR={val}"));
    }
    if let Some(val) = kconfig_raw(dotconfig, "CONFIG_NROS_DOMAIN_ID") {
        out.push(format!("cargo:rustc-env=NROS_DOMAIN_ID={val}"));
    }
    // Issue 0163 — the XRCE path has no CONFIG_NROS_ZENOH_LOCATOR; its agent
    // endpoint lives in CONFIG_NROS_XRCE_AGENT_{ADDR,PORT}. Synthesize the
    // `host:port` locator the xrce session parser expects.
    if kconfig_raw(dotconfig, "CONFIG_NROS_RMW_XRCE").as_deref() == Some("y") {
        let addr = kconfig_str(dotconfig, "CONFIG_NROS_XRCE_AGENT_ADDR")
            .unwrap_or_else(|| "127.0.0.1".to_string());
        let port = kconfig_raw(dotconfig, "CONFIG_NROS_XRCE_AGENT_PORT")
            .unwrap_or_else(|| "2018".to_string());
        out.push(format!("cargo:rustc-env=NROS_LOCATOR={addr}:{port}"));
    }
    out
}

/// The AUTHORED env↔Kconfig pairings: every knob whose Kconfig symbol is NOT
/// `CONFIG_<env name>`.
///
/// phase-468 W4. There used to be two of these, one per TABULATING reader
/// (`nros-rmw-zenoh/build.rs` and `nros-zpico-build`'s runner), and a knob
/// resolved through the pairing its own crate happened to hold. That is what
/// made issue 1233 possible in the first place: `NROS_EXECUTOR_MAX_NODES` is
/// read by `nros-node` through the DERIVED spelling and by `nros-rmw-zenoh`
/// through a table with no row for it, so ONE Kconfig symbol sized one crate
/// and not the other, and the comment beside it said "keep them in sync".
/// A pairing is a property of the KNOB, not of whoever reads it, so it lives
/// here — beside the one function that consumes it.
///
/// **Only the knobs whose two names are different WORDS belong here.** A row
/// for a derived-identical pair (`NROS_SERVICE_INBOX_BYTES` ↔
/// `CONFIG_NROS_SERVICE_INBOX_BYTES`) states nothing [`kconfig_key_for`] does
/// not already compute, and its absence is then indistinguishable from the
/// 1490 defect it is meant to prevent. EIGHT of the twenty-five rows the two
/// old tables carried were exactly that; two of the three LIVE splits issue
/// 1490 measured — the `NROS_PARAM_SERVICE_INBOX_{BYTES,DEPTH}` pair — were
/// derived-identical and are now unwritable rather than merely written.
/// 25 rows became 19: 8 dropped, and 2 added for knobs that had a row in
/// NEITHER old table (see `ZPICO_MAX_LARGE_SUBSCRIBERS` below).
///
/// The pairings are declared on the producing side by `_nros_resolve_knob()` /
/// `_nros_resolve_derivable_knob()` in `zephyr/cmake/nros_cargo_build.cmake`,
/// most of them as a literal `"${CONFIG_<SYM>}"` value.
/// `check-kconfig-knob-forwarding` harvests those and holds this table to them.
pub const KCONFIG_PAIRS: &[(&str, &str)] = &[
    // The zenoh-pico C shim's own vocabulary. `ZPICO_*` is what the shim's
    // `-D` flags and its Rust build script call a knob; Kconfig names the same
    // number `CONFIG_NROS_*`, because a Kconfig symbol is RMW-agnostic
    // (phase-403's rule).
    ("ZPICO_MAX_PUBLISHERS", "CONFIG_NROS_MAX_PUBLISHERS"),
    ("ZPICO_MAX_SUBSCRIBERS", "CONFIG_NROS_MAX_SUBSCRIBERS"),
    ("ZPICO_MAX_QUERYABLES", "CONFIG_NROS_MAX_QUERYABLES"),
    ("ZPICO_MAX_LIVELINESS", "CONFIG_NROS_MAX_LIVELINESS"),
    ("ZPICO_MAX_PENDING_GETS", "CONFIG_NROS_MAX_PENDING_GETS"),
    ("ZPICO_GET_REPLY_BUF_SIZE", "CONFIG_NROS_GET_REPLY_BUF_SIZE"),
    (
        "ZPICO_GET_POLL_INTERVAL_MS",
        "CONFIG_NROS_GET_POLL_INTERVAL_MS",
    ),
    ("ZPICO_FRAG_MAX_SIZE", "CONFIG_NROS_FRAG_MAX_SIZE"),
    ("ZPICO_BATCH_UNICAST_SIZE", "CONFIG_NROS_BATCH_UNICAST_SIZE"),
    ("ZPICO_GRAPH_CACHE_SIZE", "CONFIG_NROS_GRAPH_CACHE_SIZE"),
    // D9 (safety-island demo) -- the C shim's define against the Kconfig bool,
    // which carries the `ZENOH_` segment like the tx trio below. cmake resolves
    // it to "1"/"0" from the bool, so arm 2 of `check-kconfig-knob-forwarding`
    // cannot harvest this row and it is authored.
    ("ZPICO_GRAPH_DISCOVERY", "CONFIG_NROS_ZENOH_GRAPH_DISCOVERY"),
    (
        "ZPICO_SERVICE_BUFFER_SIZE",
        "CONFIG_NROS_SERVICE_BUFFER_SIZE",
    ),
    (
        "ZPICO_SUBSCRIBER_RING_DEPTH",
        "CONFIG_NROS_SUBSCRIBER_RING_DEPTH",
    ),
    // phase-468 W4 — these two had NO row in either old table while the cmake
    // side forwards both from a `CONFIG_NROS_*` symbol, so on a Zephyr Rust
    // image they were read env-only: issue 0460, in the pair that multiplies
    // into the largest pool the tree has
    // (`ZPICO_MAX_LARGE_SUBSCRIBERS × ZPICO_SUBSCRIBER_RING_DEPTH ×
    // ZPICO_SUBSCRIBER_LARGE_SIZE`). They were invisible to
    // `check-kconfig-knob-forwarding` because it harvested `_nros_resolve_knob(`
    // only, and both are forwarded by `_nros_resolve_derivable_knob(` — issue
    // 1505.
    (
        "ZPICO_MAX_LARGE_SUBSCRIBERS",
        "CONFIG_NROS_MAX_LARGE_SUBSCRIBERS",
    ),
    (
        "ZPICO_SUBSCRIBER_LARGE_SIZE",
        "CONFIG_NROS_SUBSCRIBER_LARGE_SIZE",
    ),
    // The tx trio, where Kconfig carries an extra `ZENOH_` segment the env
    // front-end does not. A bool option reads `y`/`n` in the `.config` and the
    // C shim takes `-DZPICO_TX_BATCH=1`, which [`kconfig_usize`] maps.
    ("ZPICO_TX_BATCH", "CONFIG_NROS_ZENOH_TX_BATCH"),
    ("ZPICO_TX_SPLIT_LOCK", "CONFIG_NROS_ZENOH_TX_SPLIT_LOCK"),
    (
        "ZPICO_TX_BATCH_FLUSH_MS",
        "CONFIG_NROS_ZENOH_TX_BATCH_FLUSH_MS",
    ),
    // issue 0626's transport priorities: `ZPICO_<role>_TASK_PRIORITY` against
    // `CONFIG_NROS_ZENOH_<role>_PRIORITY`.
    (
        "ZPICO_READ_TASK_PRIORITY",
        "CONFIG_NROS_ZENOH_READ_PRIORITY",
    ),
    (
        "ZPICO_LEASE_TASK_PRIORITY",
        "CONFIG_NROS_ZENOH_LEASE_PRIORITY",
    ),
];

/// The Kconfig symbol a knob resolves from: the authored pairing if one exists,
/// else `CONFIG_<env name>`.
///
/// The derivation is the DEFAULT because it is right for 33 of the 47 knobs the
/// cmake module forwards, and because a reader that has to name a key can name
/// the wrong one. A symbol that does not exist simply never appears in a
/// `.config`, so a derived lookup for a knob with no Kconfig side is a miss and
/// not an error.
#[must_use]
pub fn kconfig_key_for(env_name: &str) -> String {
    match KCONFIG_PAIRS.iter().find(|(env, _)| *env == env_name) {
        Some((_, kconfig)) => (*kconfig).to_string(),
        None => format!("CONFIG_{env_name}"),
    }
}

/// ONE build-time knob, resolved once — the tree's only knob ladder.
///
/// phase-468 W4. Eight build scripts across eight crates used to re-assemble
/// this by hand out of `knob_usize` / `dotconfig_usize` / a bare `env::var`,
/// and they disagreed in ways nothing reported: which Kconfig key a knob got,
/// whether a platform/board rung was consulted at all, and whether an
/// unreadable `$DOTCONFIG` stopped the build (issue 1134) or silently did not.
/// Here the SOURCES are inputs — the env name, the pairing table, an optional
/// rung — rather than three call paths a reader chooses between.
///
/// # The ladder (RFC-0049), highest rung first
///
/// 1. the environment variable — a person, right now;
/// 2. `<kconfig key>` in the `.config` named by `$DOTCONFIG` — a person, in
///    the tree;
/// 3. the platform/board rung, when the caller [`Knob::rung`]s one in;
/// 4. the crate's own builtin, which only [`Knob::resolve`] supplies.
///
/// Rung 3 is an INPUT and never a source this crate reads. The rungs live in
/// `nros-platform-config`, which depends on `serde`; this crate is
/// deliberately dependency-free (see the module docs), and a caller that has
/// no rung writes none — so a call site says how far the ladder reaches for
/// that knob rather than passing a `None` that claims otherwise.
///
/// # Why rung 2 exists at all (issue 0460)
///
/// `zephyr/cmake/nros_cargo_build.cmake` exports every resolved knob with
/// `set(ENV{...})`, which only touches the CONFIGURE-time cmake process. The C
/// lane survives that because `nros_cargo_build()` re-bakes the vars into its
/// build command (`cmake -E env …`). The RUST lane's command is built by
/// zephyr-lang-rust's `rust_cargo_application`, which passes its own fixed
/// variable list and inherits nothing — so **every Zephyr Rust image compiled
/// its crates' DEFAULTS whatever Kconfig said**. Measured on an image whose
/// `.config` said `CONFIG_NROS_EXECUTOR_MAX_CBS=16`: zero occurrences in
/// `build.ninja`, and the crate compiled 4. `DOTCONFIG` *is* in that command's
/// environment, so the value is read from the file rather than by teaching the
/// vendored module a new variable.
pub struct Knob<'a> {
    env_name: &'a str,
    rung: Option<usize>,
    strict_env: bool,
}

/// THE resolution function: a knob named by its ENV front-end.
///
/// The Kconfig key follows from [`kconfig_key_for`] — no caller names one —
/// and the remaining sources are added with [`Knob::rung`] / [`Knob::strict_env`]
/// before [`Knob::resolve`] or [`Knob::stated`] reads them.
///
/// A free function rather than `Knob::new`, and not only for reading: the knob
/// census (`scripts/check/config-knob-census.py`) classifies a call by its
/// LAST path segment, so a `::new` constructor would make `new` a read idiom
/// and every `X::new("NROS_…")` in the tree count as a knob read. `knob` is
/// already that census's word for this.
#[must_use]
pub fn knob(env_name: &str) -> Knob<'_> {
    Knob {
        env_name,
        rung: None,
        strict_env: false,
    }
}

impl<'a> Knob<'a> {
    /// Splice the platform/board rung in below `$DOTCONFIG` and above the
    /// builtin (RFC-0049). Write this only where a rung really exists.
    #[must_use]
    pub fn rung(mut self, rung: Option<usize>) -> Self {
        self.rung = rung;
        self
    }

    /// An environment value that is not a number is a BUILD ERROR rather than
    /// "nothing stated".
    ///
    /// The two policies are both deliberate and this is the input that picks
    /// between them. `nros-rmw-xrce-cffi` refuses, on the reasoning that
    /// someone typed the value in this shell and falling through to a default
    /// answers a question they did not ask. Every other reader treats it as
    /// absent, which is what `knob_usize` did and what the RANGE checks beside
    /// those call sites are written against. Unifying them would change a
    /// value, and this box is about who reads.
    #[must_use]
    pub fn strict_env(mut self) -> Self {
        self.strict_env = true;
        self
    }

    /// The Kconfig symbol this knob resolves from.
    #[must_use]
    pub fn kconfig_key(&self) -> String {
        kconfig_key_for(self.env_name)
    }

    /// Rungs 1–3: what a person, a `.config` or a descriptor STATED, or `None`.
    ///
    /// For a knob whose builtin is not a number this script can name — a const
    /// only the TARGET compiler evaluates, or one where `0` is a documented
    /// opt-out that must stay distinguishable from absence.
    #[must_use]
    pub fn stated(&self) -> Option<usize> {
        println!("cargo:rerun-if-env-changed={}", self.env_name);
        match env::var(self.env_name) {
            Ok(raw) if !raw.trim().is_empty() => match raw.trim().parse::<usize>() {
                Ok(v) => return Some(v),
                Err(_) if self.strict_env => panic!(
                    "nros-zephyr-build: {}='{raw}' is not a number",
                    self.env_name
                ),
                Err(_) => {}
            },
            _ => {}
        }
        self.kconfig_rung().or(self.rung)
    }

    /// Rung 2 alone: the `.config` named by `$DOTCONFIG`.
    fn kconfig_rung(&self) -> Option<usize> {
        let key = self.kconfig_key();
        match dotconfig(&key) {
            KnobSource::Value(v) => Some(v),
            // The key is genuinely absent from a `.config` we READ: a Kconfig
            // int left at its default is not written to the file, so this is
            // the normal case and the next rung down is the right answer.
            KnobSource::AbsentFromConfig => None,
            // Issue 1134 — `DOTCONFIG` names a file we cannot read. This used
            // to be `.unwrap_or(default)` in `knob_usize` and a plain `None` in
            // `dotconfig_usize`, indistinguishable from the arm above, and that
            // conflation is the bug: every knob silently takes its crate
            // default and the image is built to sizes its configuration never
            // asked for. A 64 KiB platform heap under a 448 KiB executor arena
            // reads as a runtime fault, not a build one, which is why it cost a
            // downstream consumer a workaround instead of a bug report.
            //
            // phase-468 W4 — the refusal now reaches EVERY reader. Three of
            // them (`nros-node`, `nros-params`, `nros-rmw-xrce-cffi`) went
            // through `dotconfig_usize`, whose doc comment said in as many
            // words that its callers could not tell the two apart.
            KnobSource::ConfigUnreadable(path, why) => panic!(
                "nros-zephyr-build: DOTCONFIG={path} could not be read ({why}), so \
                 `{key}` cannot be resolved.\n  \
                 Refusing to fall back to a default: a Zephyr image built from \
                 crate defaults compiles, links, and then behaves as though its \
                 Kconfig said nothing (issue 1134).\n  \
                 This usually means a RECONFIGURE ran without the environment the \
                 original configure had — `west build -t run` re-enters the build \
                 graph, and the run inherits whatever the shell provides."
            ),
        }
    }

    /// The whole ladder: [`Self::stated`], else the crate's builtin.
    #[must_use]
    pub fn resolve(&self, default: usize) -> usize {
        self.stated().unwrap_or(default)
    }

    /// Rungs 1–2 as a STRING, for a knob a later ladder consumes as text.
    ///
    /// The env arm takes the value VERBATIM, because this accessor doubles as
    /// the env front-end of `nros-platform-config`'s `resolve_tx` — which is
    /// handed `NROS_BOARD` and `NROS_BOARD_TOML` as well as the tx trio. The
    /// Kconfig arm is still read as a NUMBER and stringified: a bool option
    /// reads `y` in the `.config` and the parser on the other side expects
    /// `1`, so a raw read would hand it the wrong token. No knob that reaches
    /// here has a Kconfig STRING symbol.
    ///
    /// It deliberately emits NO `rerun-if-env-changed`. One of the names it is
    /// called with holds a PATH, and cargo compares an env value as text while
    /// one directory has three spellings here (issue 0491) — a path is watched
    /// by CONTENT at the call site that loads it. The 1134 refusal DOES reach
    /// here, through the shared rung-2 read: an unreadable `$DOTCONFIG` is a
    /// broken build whatever key was being asked for.
    #[must_use]
    pub fn stated_str(&self) -> Option<String> {
        if let Some(v) = env::var(self.env_name).ok().filter(|v| !v.is_empty()) {
            return Some(v);
        }
        self.kconfig_rung().map(|v| v.to_string())
    }
}

/// Where a knob's value came from — or why it did not.
///
/// Issue 1134. These three outcomes were one `Option`, and two of them meant
/// opposite things: "the config says nothing, so use the default" is correct,
/// while "there is a config and we could not read it" is a build that must
/// stop. Collapsing them is what let a reconfigure silently rebuild an image
/// to crate defaults.
///
/// PRIVATE since phase-468 W4. It is rung 2 of [`Knob`]'s ladder and nothing
/// else; the moment it is reachable on its own, a reader can assemble a second
/// ladder out of it, which is the shape this wave removed from eight crates.
#[derive(Debug)]
enum KnobSource {
    /// Read from the `.config`.
    Value(usize),
    /// No `DOTCONFIG` (not a Zephyr build at all), or the key is absent from a
    /// `.config` that WAS read.
    AbsentFromConfig,
    /// `DOTCONFIG` names a file, and reading it failed.
    ConfigUnreadable(String, String),
}

/// Read `<kconfig_key>` out of the `.config` named by `$DOTCONFIG`, keeping WHY
/// it produced no value.
fn dotconfig(kconfig_key: &str) -> KnobSource {
    println!("cargo:rerun-if-env-changed=DOTCONFIG");
    let Ok(path) = env::var("DOTCONFIG") else {
        // Not a Zephyr build. Deliberately not an error: these same build
        // scripts compile for the host and for every other platform.
        return KnobSource::AbsentFromConfig;
    };
    println!("cargo:rerun-if-changed={path}");
    match fs::read_to_string(&path) {
        Ok(body) => match kconfig_usize(&body, kconfig_key) {
            Some(v) => KnobSource::Value(v),
            None => KnobSource::AbsentFromConfig,
        },
        Err(e) => KnobSource::ConfigUnreadable(path, e.to_string()),
    }
}

/// Pure core of [`dotconfig`]: `.config` body → the key's integer value.
///
/// A bool option reads `y` (an unset bool is absent from the file, never `n`),
/// and the cmake side resolves those to `1`/`0` for the same knob — the C shim
/// takes `-DZPICO_TX_BATCH=1`. Map it the same way here so a tri-state knob
/// does not silently fall through to the crate default on the Rust lane.
fn kconfig_usize(body: &str, key: &str) -> Option<usize> {
    match kconfig_raw(body, key)?.as_str() {
        "y" => Some(1),
        "n" => Some(0),
        v => v.parse().ok(),
    }
}

/// `CONFIG_X="value"` → `Some("value")`; unset/empty → `None`.
fn kconfig_str(body: &str, key: &str) -> Option<String> {
    let raw = kconfig_raw(body, key)?;
    let val = raw.trim_matches('"');
    (!val.is_empty()).then(|| val.to_string())
}

/// `CONFIG_X=rhs` → `Some(rhs)` (verbatim, trimmed); unset/empty → `None`.
fn kconfig_raw(body: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    body.lines()
        .find_map(|l| l.strip_prefix(&prefix))
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    /// Issue 1134 — the three outcomes must stay three.
    ///
    /// These exercise [`dotconfig`] through the real `DOTCONFIG` environment
    /// variable, which is process-global, so they live in ONE test rather than
    /// three: `cargo test` runs them on threads and a second test mutating the
    /// same variable would make both flaky. The repo has been bitten by exactly
    /// that shape before (`nros_tests::unique_ros_domain_id`).
    #[test]
    fn a_knob_distinguishes_absent_from_unreadable() {
        use std::io::Write;

        // SAFETY: `set_var`/`remove_var` are unsound only with concurrent
        // readers of the environment; this test owns the variable for its
        // duration and no other test in this crate touches it.
        let restore = env::var("DOTCONFIG").ok();

        // 1. No DOTCONFIG at all — not a Zephyr build. NOT an error: the same
        //    build scripts compile for the host.
        unsafe { env::remove_var("DOTCONFIG") };
        assert!(
            matches!(dotconfig("CONFIG_ANY"), KnobSource::AbsentFromConfig),
            "no DOTCONFIG must be AbsentFromConfig, not an error"
        );

        // 2. A readable .config that does not mention the key. Correct to take
        //    the crate default: Kconfig does not write an int left at default.
        let dir = std::env::temp_dir().join(format!("nros-1134-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = dir.join(".config");
        let mut f = std::fs::File::create(&cfg).unwrap();
        writeln!(f, "CONFIG_SOMETHING_ELSE=7").unwrap();
        drop(f);
        unsafe { env::set_var("DOTCONFIG", &cfg) };
        assert!(
            matches!(dotconfig("CONFIG_ABSENT_KEY"), KnobSource::AbsentFromConfig),
            "a key absent from a READ config must be AbsentFromConfig"
        );
        assert!(
            matches!(dotconfig("CONFIG_SOMETHING_ELSE"), KnobSource::Value(7)),
            "a key present in a read config must yield its value"
        );

        // 3. DOTCONFIG names a file that is not there. THIS is the one that
        //    used to be indistinguishable from case 2, and taking the crate
        //    default here is how an image gets built to sizes its Kconfig never
        //    asked for.
        unsafe { env::set_var("DOTCONFIG", dir.join("does-not-exist")) };
        let got = dotconfig("CONFIG_ANY");
        assert!(
            matches!(got, KnobSource::ConfigUnreadable(..)),
            "an unreadable DOTCONFIG must be ConfigUnreadable, got {got:?}"
        );

        // 4. phase-468 W4 — the LADDER, on the same owned `DOTCONFIG`. It
        //    shares this test for the reason above: `DOTCONFIG` is
        //    process-global, so a second test mutating it would make both
        //    flaky.
        //
        //    The probe values are all NON-DEFAULT on purpose. Issue 1490's
        //    lesson is that a baseline cannot show a delivery failure — unset,
        //    a Kconfig default and a crate default are the same number, so
        //    "delivered" and "fell back to the same number" are ONE
        //    observation.
        let laddered = dir.join(".config-ladder");
        let mut f = std::fs::File::create(&laddered).unwrap();
        // The TABULATING shape: the two names are different words.
        writeln!(f, "CONFIG_NROS_SUBSCRIBER_RING_DEPTH=7").unwrap();
        // The DERIVED shape: `CONFIG_` + the env name.
        writeln!(f, "CONFIG_NROS_EXECUTOR_MAX_CBS=11").unwrap();
        drop(f);
        // The knob names below are real ones — the census refuses a build
        // source that reads a name `KNOB_CLASS` does not classify, and a
        // made-up name in a test is still a read. So clear whatever the
        // ambient shell may have set for them before asserting on rungs 2-4.
        //
        // ONE `unsafe` block, not five: the census counts SITES, and a site is
        // a decision someone made (issue 1221). Every env mutation in this
        // test is the same decision — the test owns the process environment
        // for its duration, which is why it is one test and not several.
        unsafe {
            env::set_var("DOTCONFIG", &laddered);
            for k in [
                "ZPICO_SUBSCRIBER_RING_DEPTH",
                "NROS_EXECUTOR_MAX_CBS",
                "NROS_EXECUTOR_MAX_SC",
            ] {
                env::remove_var(k);
            }
        }

        assert_eq!(
            knob("ZPICO_SUBSCRIBER_RING_DEPTH").resolve(4),
            7,
            "a tabulated knob must resolve through its KCONFIG_PAIRS row"
        );
        assert_eq!(
            knob("NROS_EXECUTOR_MAX_CBS").resolve(4),
            11,
            "a knob with no row must resolve through the DERIVED key"
        );
        // The rung sits BELOW `$DOTCONFIG` and ABOVE the builtin (RFC-0049).
        assert_eq!(
            knob("NROS_EXECUTOR_MAX_CBS").rung(Some(5)).resolve(4),
            11,
            "Kconfig must outrank the platform/board rung"
        );
        assert_eq!(
            knob("NROS_EXECUTOR_MAX_SC").rung(Some(5)).resolve(4),
            5,
            "with nothing above it the rung must win over the builtin"
        );
        assert_eq!(
            knob("NROS_EXECUTOR_MAX_SC").resolve(4),
            4,
            "with no rung the builtin is the answer"
        );
        // Rung 1 keeps winning. Migrating a knob into the ladder must not take
        // an operator's override away.
        unsafe { env::set_var("ZPICO_SUBSCRIBER_RING_DEPTH", "13") };
        assert_eq!(
            knob("ZPICO_SUBSCRIBER_RING_DEPTH").rung(Some(5)).resolve(4),
            13,
            "an explicit env value must outrank every other rung"
        );
        // A non-numeric env value is ABSENT by default and a build error under
        // `strict_env` — the one policy difference between the readers, and an
        // INPUT rather than a second function.
        unsafe { env::set_var("ZPICO_SUBSCRIBER_RING_DEPTH", "not-a-number") };
        let junk_default = knob("ZPICO_SUBSCRIBER_RING_DEPTH").resolve(4);
        let junk_strict = std::panic::catch_unwind(|| {
            knob("ZPICO_SUBSCRIBER_RING_DEPTH").strict_env().resolve(4)
        });
        unsafe { env::remove_var("ZPICO_SUBSCRIBER_RING_DEPTH") };
        assert_eq!(
            junk_default, 7,
            "a junk env value falls through to Kconfig by default"
        );
        assert!(
            junk_strict.is_err(),
            "strict_env must refuse a junk env value rather than default"
        );
        // The string flavour reads the same two rungs, and maps a Kconfig bool
        // to the `1`/`0` the C shim's `-D` takes.
        assert_eq!(
            knob("ZPICO_SUBSCRIBER_RING_DEPTH").stated_str().as_deref(),
            Some("7")
        );

        match restore {
            Some(v) => unsafe { env::set_var("DOTCONFIG", v) },
            None => unsafe { env::remove_var("DOTCONFIG") },
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;

    /// A pairing row must state something the derivation does NOT.
    ///
    /// phase-468 W4. A row for `NROS_X` ↔ `CONFIG_NROS_X` is a no-op, and its
    /// absence is then indistinguishable from the issue-1490 defect the table
    /// exists to prevent — "this reader tabulates, so a knob with no row is
    /// read env-only". Nine of the twenty-five rows the two old per-crate
    /// tables carried were exactly that, and the two `NROS_PARAM_SERVICE_INBOX_*`
    /// knobs 1490 measured as LIVE splits were among the names that shape
    /// affects.
    #[test]
    fn every_pairing_row_earns_its_place() {
        for (env_name, kconfig) in KCONFIG_PAIRS {
            assert_ne!(
                *kconfig,
                format!("CONFIG_{env_name}"),
                "{env_name}: this row is what `kconfig_key_for` already derives — delete it"
            );
            assert!(
                kconfig.starts_with("CONFIG_"),
                "{env_name}: `{kconfig}` is not a Kconfig symbol"
            );
        }
        for (i, (env_name, _)) in KCONFIG_PAIRS.iter().enumerate() {
            assert!(
                !KCONFIG_PAIRS[..i].iter().any(|(e, _)| e == env_name),
                "{env_name}: two rows, so which Kconfig symbol a reader gets \
                 depends on table order"
            );
        }
    }

    #[test]
    fn a_key_is_paired_or_derived() {
        assert_eq!(
            kconfig_key_for("ZPICO_MAX_QUERYABLES"),
            "CONFIG_NROS_MAX_QUERYABLES"
        );
        assert_eq!(
            kconfig_key_for("NROS_EXECUTOR_MAX_CBS"),
            "CONFIG_NROS_EXECUTOR_MAX_CBS"
        );
    }

    /// The tree's `-1` DERIVE sentinel reads as NO VALUE here.
    ///
    /// Every `-1 = derive` Kconfig knob (`NROS_EXECUTOR_MAX_CBS`,
    /// `NROS_SUBSCRIPTION_BUFFER_SIZE`, `NROS_EXECUTOR_BACKING_U64S`, …) reaches
    /// its reading build script through this function, and each depends on the
    /// sentinel falling through to the crate's own default rather than being
    /// taken as a size. Nothing said so, because the behaviour comes from
    /// `"-1".parse::<usize>()` failing rather than from a branch anyone wrote —
    /// so a future reader "fixing" this to parse an `i64` would silently hand
    /// every one of those knobs a value of `-1 as usize`. Issues 0940 / 1171.
    #[test]
    fn the_derive_sentinel_is_not_a_size() {
        let cfg = "CONFIG_NROS_EXECUTOR_BACKING_U64S=-1\n\
                   CONFIG_NROS_EXECUTOR_MAX_CBS=-1\n\
                   CONFIG_NROS_EXECUTOR_MAX_SC=8\n";
        assert_eq!(
            kconfig_usize(cfg, "CONFIG_NROS_EXECUTOR_BACKING_U64S"),
            None
        );
        assert_eq!(kconfig_usize(cfg, "CONFIG_NROS_EXECUTOR_MAX_CBS"), None);
        // A stated size still arrives — the sentinel must not swallow the
        // legitimate values beside it.
        assert_eq!(kconfig_usize(cfg, "CONFIG_NROS_EXECUTOR_MAX_SC"), Some(8));
        // `0` is a DIFFERENT answer from the sentinel and must survive: it is
        // "decline the static" for the backing knob (phase-392 W6) and "this
        // image's types all fit the small class" for the payload trio.
        assert_eq!(
            kconfig_usize(
                "CONFIG_NROS_EXECUTOR_BACKING_U64S=0\n",
                "CONFIG_NROS_EXECUTOR_BACKING_U64S"
            ),
            Some(0)
        );
    }

    #[test]
    fn zenoh_locator_and_domain_bake() {
        let cfg = "CONFIG_NROS_RMW_ZENOH=y\n\
                   CONFIG_NROS_ZENOH_LOCATOR=\"tcp/127.0.0.1:7456\"\n\
                   CONFIG_NROS_DOMAIN_ID=42\n";
        assert_eq!(
            bake_directives(cfg),
            vec![
                "cargo:rustc-env=NROS_LOCATOR=tcp/127.0.0.1:7456".to_string(),
                "cargo:rustc-env=NROS_DOMAIN_ID=42".to_string(),
            ]
        );
    }

    #[test]
    fn unset_and_empty_are_no_ops() {
        assert!(bake_directives("").is_empty());
        assert!(bake_directives("CONFIG_NROS_ZENOH_LOCATOR=\"\"\n").is_empty());
        // A different key sharing the prefix must not match.
        assert!(bake_directives("CONFIG_NROS_ZENOH_LOCATOR_EXTRA=\"x\"\n").is_empty());
    }

    #[test]
    fn xrce_synthesis_with_explicit_endpoint() {
        let cfg = "CONFIG_NROS_RMW_XRCE=y\n\
                   CONFIG_NROS_XRCE_AGENT_ADDR=\"192.0.2.7\"\n\
                   CONFIG_NROS_XRCE_AGENT_PORT=8888\n";
        assert_eq!(
            bake_directives(cfg),
            vec!["cargo:rustc-env=NROS_LOCATOR=192.0.2.7:8888".to_string()]
        );
    }

    #[test]
    fn xrce_synthesis_defaults() {
        assert_eq!(
            bake_directives("CONFIG_NROS_RMW_XRCE=y\n"),
            vec!["cargo:rustc-env=NROS_LOCATOR=127.0.0.1:2018".to_string()]
        );
    }

    #[test]
    fn xrce_absent_emits_nothing() {
        // `# CONFIG_NROS_RMW_XRCE is not set` — the Kconfig-disabled shape.
        let cfg = "# CONFIG_NROS_RMW_XRCE is not set\n\
                   CONFIG_NROS_XRCE_AGENT_ADDR=\"10.0.0.1\"\n";
        assert!(bake_directives(cfg).is_empty());
    }
}
