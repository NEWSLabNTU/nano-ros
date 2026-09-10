//! What the launcher does, as a value — RFC-0097 D4 + D11.
//!
//! [`dispatch::decide`](crate::dispatch::decide) answers a different question,
//! and the difference is why this is not a fourth arm bolted onto it. That one
//! is asked by a TOOLCHAIN that may have been fronted: every arm it can take
//! ends in "carry on in this process", because there is always a full CLI here
//! to carry on as. A launcher has no CLI at all, so three states that are
//! `Proceed` there are terminal here and each needs its own message:
//!
//! | state | toolchain (`dispatch::decide`) | launcher (this module) |
//! | --- | --- | --- |
//! | no pin | `Proceed` — run as myself | pick the newest installed, or refuse |
//! | store has no toolchain | `Proceed` — I am one | [`Plan::NothingInstalled`] |
//! | cwd in a checkout | `Proceed` — contributor | [`Plan::InCheckout`], refuse |
//!
//! ## The unpinned case, and why the launcher WARNS where `nros build` REFUSES
//!
//! An unpinned project on a developer's machine gets the newest installed
//! toolchain, the way `rustup` falls back to the default toolchain. In CI that
//! same rule silently selects whatever the runner image happens to carry —
//! RFC-0097 D11's defect one step EARLIER than the pin write, because not even
//! a file records what was chosen.
//!
//! The launcher's answer is a LOUD LINE naming the version it took and the pin
//! that would fix it ([`Source::DefaultInCi`]), not a refusal. The two halves
//! of D11 are enforced at different places on purpose:
//!
//! * **the launcher cannot know the verb.** It parses no `argv` (D8), so
//!   refusing here would refuse `nros --version`, `nros setup --list` and
//!   `nros doctor` on every runner — commands that write nothing, produce no
//!   artifact and have no reproducibility to protect. A launcher that makes CI
//!   unusable for the verbs that are fine is not enforcing D11, it is guessing;
//! * **`nros build` knows exactly what it is about to do**, which is write a
//!   file into the user's source tree, so it refuses
//!   ([`crate::pin::PinOutcome::RefusedInCi`]).
//!
//! The net effect for the operation D11 is about is unchanged — an unpinned
//! `nros build` in CI fails, having said why twice — while everything else
//! keeps working and says which toolchain answered.

use std::path::{Path, PathBuf};

use crate::{
    checkout, dispatch,
    pin::{self, FILE_NAME},
    session::{ALLOW_ENV, Session},
};

/// Who chose the toolchain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// A `nros-toolchain.toml` named it. Reproducible.
    Pin(PathBuf),
    /// Nothing named it; the newest installed was taken. The `rustup` default-
    /// toolchain rule, and fine on a developer's machine.
    Default,
    /// [`Source::Default`] in an automated session — RFC-0097 D11. Same choice,
    /// but it carries a warning, because in CI "the newest installed" is
    /// whatever the runner image happens to have rather than anything a person
    /// decided.
    DefaultInCi { var: &'static str },
}

impl Source {
    /// How the choice is described on the one line the launcher prints.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Source::Pin(p) => format!("pinned by {}", p.display()),
            Source::Default => "no pin — newest installed".to_string(),
            Source::DefaultInCi { var } => {
                format!("no pin — newest installed, and ${var} is set")
            }
        }
    }
}

/// Everything [`resolve`] needs, gathered once so the decision touches no
/// process state and its tests need no environment.
#[derive(Clone, Debug)]
pub struct Context {
    /// The launcher binary itself. Only used to find the installer it shipped
    /// with ([`dispatch::bundled_installer`]).
    pub exe: PathBuf,
    /// Where the project is — the cwd, in production.
    pub cwd: PathBuf,
    /// The store root ([`crate::store_root::root`] in production).
    pub store: PathBuf,
    /// `$NROS_TOOLCHAIN_DISPATCH`, if set. A launcher that finds it set was
    /// exec'd BY a toolchain, and must not exec again.
    pub dispatched: Option<String>,
    /// Interactive or automated (RFC-0097 D11).
    pub session: Session,
}

/// What the launcher will do. Every non-`Exec` arm is terminal and carries what
/// its message needs, because a launcher has nothing to fall back to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plan {
    /// Become this binary.
    Exec {
        version: String,
        bin: PathBuf,
        source: Source,
    },
    /// A pin names a version the store does not have. Fetchable, if this
    /// launcher shipped with an installer.
    Missing {
        version: String,
        pin_path: PathBuf,
        looked_in: Vec<PathBuf>,
    },
    /// The store holds no toolchain at all, and no pin named one.
    ///
    /// The state a freshly installed launcher is in, and the reason the
    /// launcher must not simply fail to start: "nothing installed, run X" is
    /// the single most likely thing it will ever have to say.
    NothingInstalled { store: PathBuf },
    /// The cwd is inside a nano-ros checkout. RFC-0097: *"inside a checkout the
    /// launcher is not involved"* — the ownership guard (`stale_guard`,
    /// phase-431 W1) requires that clone's own build, so dispatching a store
    /// toolchain here would only earn its refusal one frame later.
    InCheckout { root: PathBuf },
    /// `$NROS_TOOLCHAIN_DISPATCH` is set, so a toolchain exec'd US.
    ///
    /// The one failure mode a launcher must not have is an exec loop, and this
    /// is the shape it would take: a fronted launcher reached through a
    /// toolchain that dispatched to it.
    AlreadyDispatched { version: String },
}

/// The launcher decision — a pure function of [`Context`] and the filesystem.
///
/// Reads two things off disk: whether a pin exists on the walk up from `cwd`,
/// and what the store's toolchain directories hold. It never reads `argv`
/// (RFC-0095 D8) and never writes anything.
pub fn resolve(ctx: &Context) -> eyre::Result<Plan> {
    if let Some(version) = &ctx.dispatched {
        return Ok(Plan::AlreadyDispatched {
            version: version.clone(),
        });
    }
    if let Some(root) = checkout::find_monorepo_root(&ctx.cwd) {
        return Ok(Plan::InCheckout { root });
    }
    if let Some(p) = pin::find_and_load(&ctx.cwd)? {
        return Ok(match pin::installed_bin(&ctx.store, &p.version) {
            Some(bin) => Plan::Exec {
                version: p.version,
                bin,
                source: Source::Pin(p.path),
            },
            None => Plan::Missing {
                looked_in: pin::candidate_bins(&ctx.store, &p.version),
                version: p.version,
                pin_path: p.path,
            },
        });
    }
    // Unpinned from here down.
    let newest = installed_versions(&ctx.store).pop();
    // "Nothing is installed" is reported ahead of D11's refusal on purpose:
    // both are true, and only one of them is actionable here. Telling a user
    // with an empty store to commit a pin would name a version that exists
    // nowhere.
    let Some(version) = newest else {
        return Ok(Plan::NothingInstalled {
            store: ctx.store.clone(),
        });
    };
    let source = match ctx.session.ci_var() {
        Some(var) => Source::DefaultInCi { var },
        None => Source::Default,
    };
    match pin::installed_bin(&ctx.store, &version) {
        Some(bin) => Ok(Plan::Exec {
            version,
            bin,
            source,
        }),
        // `installed_versions` only reports a version whose `bin/nros` is
        // there, so this is unreachable in practice — and it is an arm rather
        // than an `unwrap` because "the store changed under us" is a real thing
        // and a panic is a bad way to say it.
        None => Ok(Plan::NothingInstalled {
            store: ctx.store.clone(),
        }),
    }
}

/// Every toolchain version installed under `store`, oldest first.
///
/// Reads BOTH layouts for the same reason [`pin::candidate_bins`] constructs
/// both: RFC-0095 D2's `toolchains/<version>` is a rename of the
/// `sdk/nros/<version>` `scripts/install.sh` writes today, and a launcher that
/// knew only the new name would see an empty store on every host that exists.
///
/// A directory only counts when it actually holds a `bin/nros`. A half-removed
/// toolchain is a directory too, and `exec`ing into one is a worse error than
/// not seeing it.
#[must_use]
pub fn installed_versions(store: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for dir in [store.join("toolchains"), store.join("sdk").join("nros")] {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let Ok(name) = e.file_name().into_string() else {
                continue;
            };
            if !e.path().join("bin").join("nros").is_file() {
                continue;
            }
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }
    // `sort_by_cached_key`, not `sort_by`: `natural_key` allocates, and a plain
    // comparator recomputes both sides on every comparison. Cached is also what
    // clippy's `unnecessary_sort_by` asks for, and the borrow checker rules out
    // the `sort_by_key` it suggests (the key owns its `String`s).
    out.sort_by_cached_key(|v| natural_key(v));
    out
}

/// One chunk of a version string for natural ordering.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Chunk {
    // Numbers sort BELOW text at the same position so `0.5.0` < `0.5.0-nros1`:
    // the shorter one runs out of chunks first, and `Vec`'s ordering makes the
    // prefix smaller. The variant order here only decides the mixed case.
    Num(u64),
    Text(String),
}

/// Natural (`sort -V`-ish) ordering key.
///
/// Lexicographic ordering is wrong for exactly the strings this compares:
/// `0.10.0-nros1` sorts BELOW `0.9.0-nros1` as text, so a store holding both
/// would default to the older one — silently, and only after ten releases. The
/// SDK store has the same rule for the same reason (issue 0500's
/// `COMPARE NATURAL ORDER DESCENDING`).
fn natural_key(s: &str) -> Vec<Chunk> {
    let mut out = Vec::new();
    let mut rest = s;
    while !rest.is_empty() {
        let digits = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        if digits > 0 {
            // A run longer than a u64 is not a version; keep it as text rather
            // than saturating two different numbers to the same value.
            match rest[..digits].parse::<u64>() {
                Ok(n) => out.push(Chunk::Num(n)),
                Err(_) => out.push(Chunk::Text(rest[..digits].to_string())),
            }
            rest = &rest[digits..];
            continue;
        }
        let text = rest
            .find(|c: char| c.is_ascii_digit())
            .unwrap_or(rest.len())
            .max(1);
        out.push(Chunk::Text(rest[..text].to_string()));
        rest = &rest[text..];
    }
    out
}

/// The message for a terminal [`Plan`] — the whole reason each arm carries its
/// own data.
///
/// One place, so the text and the decision cannot drift, and so a test can
/// assert what a user is told without running a process.
#[must_use]
pub fn explain(plan: &Plan) -> String {
    match plan {
        Plan::Exec {
            version, source, ..
        } => format!("nros: toolchain {version} ({})", source.label()),
        Plan::Missing {
            version, pin_path, ..
        } => format!(
            "this project pins nano-ros {version} and the store does not have it.\n\
             \x20   pinned by: {}",
            pin_path.display()
        ),
        Plan::NothingInstalled { store } => format!(
            "no nano-ros toolchain is installed.\n\
             \x20   store: {}\n\
             This is the launcher; the toolchain it launches is a separate \
             artifact (RFC-0097 D4),\n\
             and this store has none of them yet. Install one:\n\
             \x20   curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh | sh\n\
             Then `nros build` in your project.",
            store.display()
        ),
        Plan::InCheckout { root } => format!(
            "the current directory is inside a nano-ros CHECKOUT, so the launcher \
             stands aside.\n\
             \x20   checkout: {}\n\
             A checkout builds with its OWN `nros` — a store toolchain run here \
             is refused one\n\
             frame later by the ownership guard anyway (phase-431 W1), and it \
             would emit with\n\
             emitters nobody in that tree can see. Build and use the checkout's \
             own CLI:\n\
             \x20   ./scripts/bootstrap.sh      (contributors: just setup-cli)\n\
             \x20   source ./activate.sh",
            root.display()
        ),
        Plan::AlreadyDispatched { version } => format!(
            "this launcher was reached from a toolchain that had already \
             dispatched ({version}).\n\
             That is an exec loop, and a launcher must not have one. The \
             toolchain at\n\
             ${} = {version} either does not exist or does not agree about its \
             own version.\n\
             Override for a deliberate experiment: {}=1",
            dispatch::DISPATCHED_ENV,
            dispatch::SKIP_ENV,
        ),
    }
}

/// The line an automated session gets when nothing named a toolchain —
/// RFC-0097 D11, the launcher's half.
///
/// `Some` only for [`Source::DefaultInCi`]. Printed BEFORE the `exec`, because
/// after it this process is gone; that is also why it is a separate function
/// from [`explain`], which describes states that never reach an `exec` at all.
///
/// It names the version that was taken, so the warning is a record of what
/// happened rather than a note that something might have. Without the version
/// the reader has to reconstruct the store's contents at that moment, which on
/// a runner is exactly the thing that no longer exists.
#[must_use]
pub fn warning(plan: &Plan) -> Option<String> {
    let Plan::Exec {
        version,
        source: Source::DefaultInCi { var },
        ..
    } = plan
    else {
        return None;
    };
    Some(format!(
        "warning: this project has no {FILE_NAME} and ${var} is set, so it ran \
         {version} —\n\
         \x20   whatever this runner happens to have installed, chosen by \
         nobody (RFC-0097 D11).\n\
         \x20   An unpinned `nros build` here will REFUSE rather than pin, \
         because a pin is a\n\
         \x20   source edit. Write {FILE_NAME} beside your project and commit \
         it:\n\n\
         {}\n\
         \x20   To silence this and let an automated session choose: \
         {ALLOW_ENV}=1",
        pin::render(version)
            .lines()
            .map(|l| format!("\x20       {l}\n"))
            .collect::<String>(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with(root: &Path, rels: &[&str]) {
        for rel in rels {
            let bin = root.join(rel).join("bin");
            std::fs::create_dir_all(&bin).unwrap();
            std::fs::write(bin.join("nros"), b"x").unwrap();
        }
    }

    /// `0.10.0` is NEWER than `0.9.0`, and text ordering says otherwise.
    ///
    /// The bug this forecloses is invisible for the project's first nine
    /// releases and then silently defaults every unpinned project to an older
    /// toolchain, so it is worth a test rather than a comment.
    #[test]
    fn versions_order_naturally_not_lexicographically() {
        let tmp = tempfile::tempdir().unwrap();
        store_with(
            tmp.path(),
            &[
                "toolchains/0.9.0-nros1",
                "toolchains/0.10.0-nros1",
                "toolchains/0.10.0-nros2",
            ],
        );
        let got = installed_versions(tmp.path());
        assert_eq!(
            got,
            vec!["0.9.0-nros1", "0.10.0-nros1", "0.10.0-nros2"],
            "natural order, oldest first"
        );
    }

    /// Both store layouts are seen, and a directory with no `bin/nros` is not a
    /// toolchain — a half-removed one must not be exec'd into.
    #[test]
    fn both_layouts_count_and_an_empty_directory_does_not() {
        let tmp = tempfile::tempdir().unwrap();
        store_with(
            tmp.path(),
            &["toolchains/0.6.0-nros1", "sdk/nros/0.5.0-nros1"],
        );
        std::fs::create_dir_all(tmp.path().join("toolchains/0.7.0-nros1")).unwrap();
        assert_eq!(
            installed_versions(tmp.path()),
            vec!["0.5.0-nros1", "0.6.0-nros1"]
        );
    }
}
