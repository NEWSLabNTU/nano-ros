//! Issue 0363 B — the in-tree CLI refuses to run stale.
//!
//! A staleness guard already existed and was good: `scripts/build/cargo.sh`
//! walks `git ls-files packages/cli` for any source newer than the binary and
//! refuses. But it lives in the shell function `nros_cli_bin()`, so it only
//! covers callers that go through `just` — while `activate.sh` puts the raw
//! binary on `PATH`, so a bare `nros sync` never reaches it.
//!
//! That is the whole defect. `nros sync` is the command CLAUDE.md and
//! `nros-patch.toml`'s own header tell you to run to recover, and it was
//! precisely the invocation the protection did not cover. Same shape as issue
//! 0354 (a validator whose callers exclude the case it exists for), with a
//! worse payload: phase-321 moved packages, the stale binary's hardcoded
//! crate→path table still named the old locations, and the emitted
//! `[patch.crates-io]` table DROPPED `nros-zephyr-build` without a word. A
//! dropped patch entry does not fail — the dependency quietly resolves from
//! crates.io instead of the checkout.
//!
//! So the check moves to where it cannot be bypassed by invocation style: the
//! binary checks itself.

use std::path::{Path, PathBuf};

use crate::source_stamp;

/// Stamp of the sources this binary was compiled from, embedded by `build.rs`.
///
/// `"unknown"` when the build happened outside a git checkout (tarball,
/// vendored copy). That is a skip, not a failure: without the tree there is
/// nothing to be stale RELATIVE TO, and guessing would break every packaged
/// install.
const BUILT_STAMP: &str = env!("NROS_CLI_SOURCE_STAMP");

/// The same stamp, PER INPUT — `label=hash,label=hash,…`, also from `build.rs`.
///
/// Issue 1018. Empty when the build could not stamp at all, and possibly
/// missing labels when an older binary meets a newer input list; both are
/// "cannot attribute", never "nothing moved".
const BUILT_COMPONENTS: &str = env!("NROS_CLI_SOURCE_STAMP_COMPONENTS");

/// The hash `built` (a `label=hash,…` string) carries for one stamp input.
fn built_component<'a>(built: &'a str, label: &str) -> Option<&'a str> {
    built
        .split(',')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| *k == label)
        .map(|(_, v)| v)
}

/// Which stamp inputs differ between a binary's baked components and `root` —
/// issue 1018.
///
/// `built` is passed in rather than read from `BUILT_COMPONENTS` inside, for
/// the reason [`refuse_if_foreign_to_workspace`]'s workspace argument is: it
/// makes the decision a pure function of its inputs, so its tests can build a
/// checkout and a baked-component string instead of needing a binary that was
/// compiled from that checkout — which no test can produce.
///
/// `None` means the question cannot be answered here: a binary with no baked
/// components (built before they existed), or a tree that cannot be stamped.
/// The caller then says less rather than something wrong.
///
/// An input this binary has no baked hash for is NOT reported as moved. A newer
/// tree adding a stamp input is a real staleness, but naming it as "moved"
/// would assert a comparison that was never made; [`attribution`]'s empty arm
/// covers it and says exactly that.
fn moved_inputs(built: &str, root: &Path) -> Option<Vec<&'static str>> {
    if built.is_empty() {
        return None;
    }
    let now = source_stamp::source_stamp_components(root)?;
    let mut compared = 0usize;
    let mut moved = Vec::new();
    for (label, hash) in now {
        let Some(built_hash) = built_component(built, label) else {
            continue;
        };
        compared += 1;
        if built_hash != hash {
            moved.push(label);
        }
    }
    (compared > 0).then_some(moved)
}

/// A 40-hex sha abbreviated for reading; anything else (`unknown`,
/// `uninitialised`) passed through — those words are the answer, not an id.
fn short(sha: &str) -> String {
    if sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        sha[..12].to_string()
    } else {
        sha.to_string()
    }
}

/// `<head>` plus up to three indented paths, then a count. Three because the
/// refusal is read inside a cmake error block, where it is already re-wrapped
/// once per line and a full file list would bury the remedy.
fn listed(head: &str, files: &[String]) -> String {
    let mut s = String::from(head);
    for f in files.iter().take(3) {
        s.push_str(&format!("\n\x20       {f}"));
    }
    if files.len() > 3 {
        s.push_str(&format!("\n\x20       … and {} more", files.len() - 3));
    }
    s
}

/// One sentence per stamp input that moved, naming the input and what to do.
///
/// Every arm is tagged `ATTRIBUTES:` because that tag is what
/// `check-stale-cli-attribution` joins against `source_stamp::STAMP_INPUTS`, in
/// both directions: a stamp input with no arm is a refusal that has to guess,
/// and an arm naming no input is a rule about something that stopped existing.
fn attribution(built_pin: &str, root: &Path, moved: &[&str]) -> String {
    let mut lines: Vec<String> = Vec::new();
    for label in moved {
        match *label {
            // ATTRIBUTES: cli_sources
            //
            // The CLI's source content. One stamp input, but three shapes a
            // reader needs told apart — and the second and third are the ones
            // the pre-1018 message could not see:
            //
            //   * uncommitted edits — the common case, already named before;
            //   * a new source written but not `git add`ed, which compiles into
            //     the binary and so stales it while showing up in no diff;
            //   * committed content that differs, which IS a checkout move.
            //
            // The old sentence asserted the third for all three.
            "cli_sources" => {
                let dirty = source_stamp::modified_cli_files(root);
                let new = source_stamp::untracked_cli_files(root);
                if !dirty.is_empty() {
                    lines.push(listed("CLI sources: uncommitted edits", &dirty));
                }
                if !new.is_empty() {
                    lines.push(listed("CLI sources: new, untracked", &new));
                }
                if dirty.is_empty() && new.is_empty() {
                    lines.push(
                        "CLI sources: committed content differs from the build's,\n\
                         \x20     with nothing uncommitted here — a branch switch, rebase or pull"
                            .to_string(),
                    );
                }
            }
            // ATTRIBUTES: play_launch_pin
            //
            // Issue 1018's stop 2, the one it calls interesting: nothing in the
            // consumer's tree changed, and the CLI is correctly stale anyway.
            // `build.rs` bakes this pin as `NROS_PLAY_LAUNCH_SHA` and the
            // issue-0409 guard compares it, so a pin move IS a source change to
            // the CLI — it is just not one in any file. Phase-429 measured the
            // stop to be right; what was wrong was calling it a checkout move.
            "play_launch_pin" => {
                // The two SHAs, not the two component hashes: the pin is the
                // one stamp input a person can act on directly, and `<sha> ->
                // <sha>` is what `git -C packages/cli/third-party/play_launch
                // log` will confirm. `NROS_PLAY_LAUNCH_SHA` is the same value
                // `build.rs` baked, so this cannot drift from what was built.
                let now = source_stamp::play_launch_pin(root);
                lines.push(format!(
                    "the pinned play_launch submodule moved: {} -> {}\n\
                     \x20     (a pin is a CLI build input — `build.rs` bakes it as\n\
                     \x20      NROS_PLAY_LAUNCH_SHA and the issue-0409 guard compares it.\n\
                     \x20      Moving the pin, or `git submodule update --init`, stales the\n\
                     \x20      CLI although no file in your tree changed: expected, and not\n\
                     \x20      a mistake on your part. Rebuild and carry on.)",
                    short(built_pin),
                    short(now.as_deref().unwrap_or("uninitialised")),
                ));
            }
            other => lines.push(format!("{other} (no attribution for this input)")),
        }
    }
    if lines.is_empty() {
        // Every comparable input agrees, yet the fold differs: this binary's
        // input SET is not this tree's. Say that, rather than blaming content.
        return "  what moved: the stamp's INPUT SET — this binary predates one of the\n\
                \x20 inputs this tree stamps, so there is nothing to compare it against"
            .to_string();
    }
    let mut out = String::from("  what moved:\n");
    for line in lines {
        out.push_str(&format!("\x20   - {line}\n"));
    }
    out.trim_end().to_string()
}

/// Commands that consume the crate→path table or emit generated artifacts.
///
/// Deliberately NOT every command. `nros --version` / `completions` / `doctor`
/// must keep working on a stale binary — `doctor` especially, since diagnosing
/// a broken checkout is exactly when you have one.
fn command_is_guarded(name: &str) -> bool {
    matches!(
        name,
        "sync"
            | "plan"
            | "ws"
            | "ws-build"
            | "codegen"
            | "codegen-system"
            | "generate-rust"
            | "setup"
    )
}

/// Does the WORKSPACE check apply to this verb? — issue filed by phase-413 W2.
///
/// The staleness check (case 1) asks about the BINARY and applies to every
/// guarded verb. This one asks "which checkout is my cwd in", and that question
/// is meaningless for the build-tool verbs: cmake invokes `nros codegen` with
/// the tool path it was CONFIGURED with (`-D_NANO_ROS_CODEGEN_TOOL`), so the
/// caller has already answered "which binary", and the cwd is wherever the
/// build system put its output.
///
/// It fired on exactly that. The tier-2 `zephyr` lane builds into a west
/// workspace which, on the CI runner, sits under a DIFFERENT nano-ros tree from
/// the one being built — the runner's checkout and the workspace root are two
/// separate directories, and `NROS_ZEPHYR_WORKSPACE` may point anywhere.
///
/// So the refusal read: running `<runner checkout>/packages/cli/target/release/
/// nros`, checkout `<the west workspace's parent>`. The binary was the runner
/// checkout's own — correct, freshly built — and the guard refused it because
/// the cwd walked up into another checkout. A build OUTPUT directory is not a
/// consumer workspace, and nothing tells the two apart by path alone.
///
/// The verbs a USER types in their own workspace keep the check, which is what
/// phase-431 W1 was for: `sync`, `plan`, `ws`, `setup`.
fn workspace_check_applies(name: &str) -> bool {
    // `ws-build` is the five `ws` subcommands cmake invokes — see `ws_cmd_name`
    // in `lib.rs`. Same argument as the three beside it, and the same lane:
    // two of the five swallow a refusal into `message(STATUS …)` and build on
    // with empty board/entity facts.
    !matches!(
        name,
        "codegen" | "codegen-system" | "generate-rust" | "ws-build"
    )
}

/// Refuse to run when this binary is older than the sources it was built from,
/// or when it is a FOREIGN binary being run against a checkout.
///
/// Two questions, and phase-431 W1 added the second because shipping a prebuilt
/// `nros` inverts what the first one's exemption means.
///
/// 1. **Is this binary stale relative to the checkout it lives in?** Keyed on
///    the binary's own path. Unchanged.
/// 2. **Is a binary from somewhere else being run against a checkout?** Keyed on
///    the WORKSPACE. Until there was a release to install, the only non-checkout
///    `nros` was a deliberate experiment, so exempting it was right. Once
///    `~/.nros/bin/nros` exists on developer machines, that exemption becomes a
///    hole: a PATH accident silently disables the freshness check inside a
///    checkout, and the binary emits with whatever ITS emitters were.
///
/// RFC-0090's codegen version does not cover case 2. It catches an INCOMPATIBLE
/// emitter; a release at the same version whose emitters have merely MOVED is a
/// freshness question, and the fingerprint that answers it is consulted by the
/// fixture stamps, not here.
///
/// An installed copy run against a user's own project is still exempt, which is
/// the case the original exemption existed to protect.
pub fn refuse_if_stale(command_name: &str) -> Result<(), String> {
    if std::env::var_os("NROS_SKIP_STALE_CHECK").is_some() {
        return Ok(());
    }
    if !command_is_guarded(command_name) {
        return Ok(());
    }
    let Ok(exe) = std::env::current_exe() else {
        return Ok(());
    };
    // The workspace is the cwd. Passed in rather than read inside, so the
    // decision is a pure function of two paths and its tests need no
    // `set_current_dir` — a process-global that leaks between parallel tests
    // (issue 1101 is that hazard, one crate over).
    if workspace_check_applies(command_name) {
        if let Ok(cwd) = std::env::current_dir() {
            refuse_if_foreign_to_workspace(&exe, &cwd)?;
        }
    }
    let Some(root) = checkout_root_of(&exe) else {
        return Ok(());
    };
    if BUILT_STAMP == "unknown" {
        return Ok(());
    }
    // No stamp computable now (git absent / not a checkout) — skip rather than
    // guess. Same reasoning as `BUILT_STAMP == "unknown"`.
    let Some(current) = source_stamp::source_stamp(&root) else {
        return Ok(());
    };
    if current == BUILT_STAMP {
        return Ok(());
    }
    // Name the INPUT that moved, then the files under it. The mtime predicate
    // could only report whichever tracked file sorted first; naming the dirty
    // files fixed that for the one cause it covers, and issue 1018 is the rest:
    // a cause the message could not see was reported as the cause it could.
    let detail = match moved_inputs(BUILT_COMPONENTS, &root) {
        Some(moved) => attribution(env!("NROS_PLAY_LAUNCH_SHA"), &root, &moved),
        // No baked components — a binary built before issue 1018. Attribution
        // is unavailable, so fall back to what CAN be said without it, which is
        // the pre-1018 sentence. Saying nothing here would be a regression for
        // the case that message did cover.
        None => {
            let dirty = source_stamp::modified_cli_files(&root);
            if dirty.is_empty() {
                "  (this binary bakes no per-input stamp, so what moved cannot be named)"
                    .to_string()
            } else {
                let mut s = String::from("  uncommitted CLI edits:\n");
                for f in dirty.iter().take(3) {
                    s.push_str(&format!("    {f}\n"));
                }
                if dirty.len() > 3 {
                    s.push_str(&format!("    … and {} more\n", dirty.len() - 3));
                }
                s.trim_end().to_string()
            }
        }
    };
    Err(format!(
        "in-tree nros CLI is STALE — its sources changed since it was built\n\
         (source stamp {BUILT_STAMP} != {current}) for '{}',\n\
         whose checkout is '{}'.\n\
         {detail}\n\
         A stale CLI silently breaks workspace planning + codegen: its hardcoded\n\
         crate→path table can name locations that no longer exist, and a dropped\n\
         [patch.crates-io] entry resolves from crates.io instead of this checkout\n\
         WITHOUT failing (issues 0363, 0197).\n\
         Rebuild it (not auto-done — compiling at build/test time is forbidden):\n\
         \x20   ./scripts/bootstrap.sh      (contributors: just setup-cli)\n\
         Override for a deliberate experiment: NROS_SKIP_STALE_CHECK=1",
        exe.display(),
        // Issue 1133 — name the CHECKOUT, not only the binary. With several
        // worktrees in play the first question is "is this even my tree?", and
        // the exe path answers it only if you already know where each tree's
        // target dir lives. A `PATH`-resolved binary from ANOTHER checkout is
        // the common cause, and no number of rebuilds here can refresh it.
        root.display()
    ))
}

/// phase-431 W1 — refuse a binary that does not belong to the checkout it is
/// being run against.
///
/// The workspace decides, not the binary: if the current directory sits inside a
/// nano-ros checkout AND that checkout can build a CLI, then the running `nros`
/// must be that checkout's own build. Anything else is a shadow — a released
/// binary on `PATH`, another checkout's build, a stray copy — and it emits with
/// emitters nobody in this tree can see.
///
/// Deliberately silent in three cases, each of which would otherwise break a
/// legitimate flow:
///
/// * the cwd is not in a checkout — a user's own project, which is exactly what
///   a released binary is FOR;
/// * the checkout carries no `packages/cli` sources — nothing to be foreign to;
/// * `current_exe` or the cwd cannot be resolved — skip rather than guess, the
///   same rule the stamp comparison already follows.
fn refuse_if_foreign_to_workspace(exe: &Path, workspace: &Path) -> Result<(), String> {
    let Some(ws_root) = crate::abi_guard::find_monorepo_root(workspace) else {
        return Ok(());
    };
    // A checkout with no CLI sources cannot expect one to be built from it.
    if !ws_root.join("packages/cli/Cargo.toml").is_file() {
        return Ok(());
    }
    let exe_real = exe.canonicalize().unwrap_or_else(|_| exe.to_path_buf());
    let expected_dir = ws_root.join("packages/cli/target");
    let expected_real = expected_dir
        .canonicalize()
        .unwrap_or_else(|_| expected_dir.clone());
    if exe_real.starts_with(&expected_real) {
        return Ok(());
    }
    Err(format!(
        "this `nros` does not belong to the checkout it is being run against.\n\
         \x20   running: {}\n\
         \x20   checkout: {}\n\
         A binary from outside this tree emits with ITS OWN codegen, which may\n\
         differ from this checkout's while carrying the same codegen version —\n\
         the version catches an incompatible emitter, not one that merely moved\n\
         (RFC-0090, phase-431 W1). Build and use this checkout's own CLI:\n\
         \x20   ./scripts/bootstrap.sh      (contributors: just setup-cli)\n\
         \x20   source ./activate.sh\n\
         Override for a deliberate experiment: NROS_SKIP_STALE_CHECK=1",
        exe_real.display(),
        ws_root.display(),
    ))
}

/// `<root>` when `exe` is `<root>/packages/cli/target/**/nros`, else `None`.
fn checkout_root_of(exe: &Path) -> Option<PathBuf> {
    let mut dir = exe.parent()?;
    // walk up looking for the `packages/cli/target` shape
    while let Some(parent) = dir.parent() {
        if dir.file_name().is_some_and(|n| n == "target")
            && parent.file_name().is_some_and(|n| n == "cli")
            && parent
                .parent()?
                .file_name()
                .is_some_and(|n| n == "packages")
        {
            return parent.parent()?.parent().map(Path::to_path_buf);
        }
        dir = parent;
    }
    None
}

/// Report freshness without refusing anything — backs `nros source-stamp`.
///
/// Returns `(built, current)`. Equal means fresh; `None` means the question
/// does not apply here (no stamp, or not a per-checkout binary).
pub fn stamp_pair() -> Option<(String, String)> {
    if BUILT_STAMP == "unknown" {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    let root = checkout_root_of(&exe)?;
    let current = source_stamp::source_stamp(&root)?;
    Some((BUILT_STAMP.to_string(), current))
}

/// Issue 1018 — every stamp input must be nameable by the refusal, measured.
///
/// The gate `check-stale-cli-attribution` asks the same question of the TEXT,
/// on the fast line and without a compiler. This asks it of the BEHAVIOUR:
/// perturb one input in a real checkout and read what the refusal says. Both
/// are needed for the reason issue 1167 records one lane over — a guard that
/// exists is not a guard that fires.
#[cfg(test)]
mod attribution_tests {
    use super::*;
    use crate::source_stamp::STAMP_INPUTS;
    use std::fs;

    fn sh(dir: &Path, cmd: &str) {
        // Issues 0986/0988 — a test that runs `git init` in a temp dir must not
        // be steerable by an inherited git environment: under a `GIT_DIR` (which
        // every linked worktree here sets) `git init <tmp>` builds nothing and
        // writes into the CALLER's repository instead. The list is ASKED of git
        // so it cannot drift, exactly as `source_stamp`'s own tests do.
        let vars: Vec<String> = std::process::Command::new("git")
            .args(["rev-parse", "--local-env-vars"])
            .output()
            .ok()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .split_whitespace()
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let mut command = std::process::Command::new("sh");
        for var in vars {
            command.env_remove(var);
        }
        let ok = command
            .args(["-c", cmd])
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok, "command failed: {cmd}");
    }

    /// A checkout the stamp will answer for: the generated closure list (issue
    /// 0604 — without it `source_stamp` refuses), one CLI source, and an
    /// initialised play_launch submodule so the pin has somewhere to move FROM.
    fn checkout(root: &Path) {
        fs::create_dir_all(root.join("packages/cli/x/src")).unwrap();
        fs::write(root.join("packages/cli/cli-source-dirs.txt"), "# test\n").unwrap();
        fs::write(root.join("packages/cli/x/src/lib.rs"), "fn a() {}\n").unwrap();
        sh(root, "git init -q -b main .");
        // Named paths, never `git add -A`: once the nested play_launch repo
        // exists a blanket add stages it as an embedded gitlink, which is the
        // hazard CLAUDE.md records for the real tree and is just as wrong in a
        // fixture — it would put the submodule's own commit into the stamp's
        // tracked side and stop the pin from being an independent input.
        sh(root, "git add packages/cli && git commit -qm init");
        let sub = root.join("packages/cli/third-party/play_launch");
        fs::create_dir_all(&sub).unwrap();
        sh(
            &sub,
            "git init -q -b main . && git commit -q --allow-empty -m pin1",
        );
    }

    fn baked(root: &Path) -> String {
        crate::source_stamp::source_stamp_components(root)
            .expect("a checkout must stamp")
            .into_iter()
            .map(|(l, h)| format!("{l}={h}"))
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Move exactly ONE stamp input, and say what the refusal must then name.
    ///
    /// No catch-all arm, deliberately: adding a label to `STAMP_INPUTS` without
    /// deciding how to perturb it is a COMPILE error here, which is the one
    /// corner a text gate cannot hold. `check-stale-cli-attribution` holds the
    /// other three.
    fn perturb(label: &str, root: &Path) -> &'static str {
        match label {
            "cli_sources" => {
                fs::write(
                    root.join("packages/cli/x/src/lib.rs"),
                    "fn a() {}\nfn b() {}\n",
                )
                .unwrap();
                "uncommitted edits"
            }
            "play_launch_pin" => {
                sh(
                    &root.join("packages/cli/third-party/play_launch"),
                    "git commit -q --allow-empty -m pin2",
                );
                "play_launch submodule moved"
            }
            other => panic!("STAMP_INPUTS gained `{other}` with no perturbation case"),
        }
    }

    /// The whole rule, per input: move it alone, and the refusal names IT.
    #[test]
    fn every_stamp_input_is_named_when_it_moves() {
        for label in STAMP_INPUTS {
            let tmp = tempfile::tempdir().unwrap();
            let root = tmp.path();
            checkout(root);
            let built = baked(root);
            let built_pin = crate::source_stamp::play_launch_pin(root).expect("pin after init");

            let want = perturb(label, root);
            let moved = moved_inputs(&built, root).expect("components are comparable");
            assert_eq!(
                moved,
                vec![label],
                "moving `{label}` alone must report `{label}` alone — reporting more \
                 is the guess this issue was about, reporting fewer is silence"
            );
            let text = attribution(&built_pin, root, &moved);
            assert!(
                text.contains(want),
                "the refusal for `{label}` must contain {want:?}, got:\n{text}"
            );
        }
    }

    /// Issue 1018's stop 2, the one it calls interesting — and the negative
    /// control for the sentence it USED to print.
    ///
    /// A contributor moves a submodule pin forward and touches nothing else.
    /// The refusal is correct (phase-429 measured that: the pin IS a CLI build
    /// input), and it used to explain itself with "no uncommitted CLI edits —
    /// the checkout moved, e.g. a branch switch". The checkout had not moved.
    #[test]
    fn a_pin_move_is_not_reported_as_a_checkout_move() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        checkout(root);
        let built = baked(root);
        let built_pin = crate::source_stamp::play_launch_pin(root).unwrap();

        sh(
            &root.join("packages/cli/third-party/play_launch"),
            "git commit -q --allow-empty -m forward",
        );
        let now_pin = crate::source_stamp::play_launch_pin(root).unwrap();
        assert_ne!(built_pin, now_pin, "the fixture must actually move the pin");

        let moved = moved_inputs(&built, root).unwrap();
        let text = attribution(&built_pin, root, &moved);
        assert!(
            text.contains("play_launch submodule moved"),
            "a pin move must be named as a pin move: {text}"
        );
        assert!(
            text.contains(&built_pin[..12]) && text.contains(&now_pin[..12]),
            "and it must name BOTH shas, so `git log` in the submodule confirms it: {text}"
        );
        assert!(
            !text.contains("branch switch") && !text.contains("committed content"),
            "it must NOT be reported as a checkout move — that was the defect: {text}"
        );
        assert!(
            text.contains("a mistake on your part"),
            "a correct refusal for something the user did right must say so: {text}"
        );
    }

    /// `cli_sources` is one input with three shapes, and the two the pre-1018
    /// message could not see are asserted here: a new untracked source, and
    /// both shapes at once.
    ///
    /// An untracked source is the quieter of the two — it compiles into the
    /// binary and appears in no diff, so the old message called it a branch
    /// switch and a contributor went looking at their git log.
    #[test]
    fn an_untracked_source_is_named_as_one() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        checkout(root);
        let built = baked(root);
        let built_pin = crate::source_stamp::play_launch_pin(root).unwrap();

        fs::write(root.join("packages/cli/x/src/new.rs"), "fn c() {}\n").unwrap();
        let moved = moved_inputs(&built, root).unwrap();
        assert_eq!(moved, vec!["cli_sources"]);
        let text = attribution(&built_pin, root, &moved);
        assert!(text.contains("new, untracked"), "{text}");
        assert!(
            text.contains("x/src/new.rs"),
            "it must name the file: {text}"
        );
        assert!(
            !text.contains("branch switch"),
            "a file you just wrote is not a checkout move: {text}"
        );

        // Both shapes at once — the residual arm must stay out of the way.
        fs::write(
            root.join("packages/cli/x/src/lib.rs"),
            "fn a() {}\nfn b() {}\n",
        )
        .unwrap();
        let text = attribution(&built_pin, root, &moved_inputs(&built, root).unwrap());
        assert!(text.contains("uncommitted edits"), "{text}");
        assert!(text.contains("new, untracked"), "{text}");
        assert!(!text.contains("committed content differs"), "{text}");
    }

    /// Two inputs moving at once must both be named. This is what elimination
    /// could never do: with one number and a dirty-file probe, a pin move
    /// alongside an edit is invisible behind the edit.
    #[test]
    fn two_inputs_moving_are_both_named() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        checkout(root);
        let built = baked(root);
        let built_pin = crate::source_stamp::play_launch_pin(root).unwrap();

        perturb("cli_sources", root);
        perturb("play_launch_pin", root);
        let moved = moved_inputs(&built, root).unwrap();
        assert_eq!(moved.len(), 2, "both inputs moved: {moved:?}");
        let text = attribution(&built_pin, root, &moved);
        assert!(text.contains("uncommitted edits"), "{text}");
        assert!(text.contains("play_launch submodule moved"), "{text}");
    }

    /// The residual arm: committed content differs and nothing is dirty. That
    /// IS a checkout move, and the old sentence was right about this one case —
    /// which is why the fix is attribution rather than a reworded message.
    #[test]
    fn a_commit_with_nothing_dirty_reads_as_a_checkout_move() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        checkout(root);
        let built = baked(root);
        let built_pin = crate::source_stamp::play_launch_pin(root).unwrap();

        fs::write(
            root.join("packages/cli/x/src/lib.rs"),
            "fn a() {}\nfn b() {}\n",
        )
        .unwrap();
        sh(
            root,
            "git add packages/cli/x/src/lib.rs && git commit -qm second",
        );

        let moved = moved_inputs(&built, root).unwrap();
        assert_eq!(moved, vec!["cli_sources"]);
        let text = attribution(&built_pin, root, &moved);
        assert!(text.contains("committed content differs"), "{text}");
        assert!(text.contains("rebase or pull"), "{text}");
    }

    /// A binary with no baked components cannot attribute, and must say so
    /// rather than report that nothing moved — "assume fresh" is the one answer
    /// a freshness probe must never give (`source_stamp`'s own rule).
    #[test]
    fn a_binary_with_no_components_cannot_attribute() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        checkout(root);
        assert!(
            moved_inputs("", root).is_none(),
            "no baked components means the question is unanswerable, not answered `no`"
        );
    }

    /// A binary that predates ONE input still compares the others, and when
    /// they all agree it names the input SET as what moved — never content.
    #[test]
    fn an_input_this_binary_never_baked_is_not_called_moved() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        checkout(root);
        let full = baked(root);
        let partial: String = full
            .split(',')
            .filter(|kv| !kv.starts_with("play_launch_pin="))
            .collect::<Vec<_>>()
            .join(",");
        perturb("play_launch_pin", root);

        let moved = moved_inputs(&partial, root).expect("the other inputs are comparable");
        assert!(
            moved.is_empty(),
            "an input with no baked hash was never compared, so it cannot be \
             REPORTED as moved: {moved:?}"
        );
        let text = attribution("unknown", root, &moved);
        assert!(text.contains("INPUT SET"), "{text}");
    }
}

#[cfg(test)]
mod foreign_binary_tests {
    /// The build-tool verbs are exempt from the WORKSPACE check — the
    /// regression phase-413 W2 found on the tier-2 lane.
    ///
    /// cmake invokes `nros codegen` with the tool it was configured with, and
    /// the cwd is wherever the build put its output. On the CI runner the
    /// zephyr west workspace lives under a different nano-ros tree, so the cwd
    /// walked up into a checkout the binary did not come from and a correct,
    /// freshly built binary was refused.
    #[test]
    fn the_build_tool_verbs_skip_the_workspace_check() {
        // `ws-build` is the five `ws` subcommands cmake invokes (`ws_cmd_name`
        // in `lib.rs`). Phase-413 W2 found them AFTER the three below: the
        // first fix landed at the one site whose symptom was visible, and
        // `ws providers` / `ws order` / `ws entity-inventory` would have gone
        // red at the first workspace entry leaf, while `ws board-facts` and
        // `ws entity-facts` swallow the refusal and build on with no facts.
        for verb in ["codegen", "codegen-system", "generate-rust", "ws-build"] {
            assert!(!workspace_check_applies(verb), "{verb} must be exempt");
            // …but they are still STALENESS-guarded: that question is about the
            // binary, and is the one that protects a build-time emitter.
            assert!(command_is_guarded(verb), "{verb} must stay stale-guarded");
        }
        // What a user types in their own workspace keeps the check.
        for verb in ["sync", "plan", "ws", "setup"] {
            assert!(workspace_check_applies(verb), "{verb} must keep the check");
        }
    }

    use super::*;
    use std::fs;

    /// A checkout is "a tree with `packages/core/nros-core/Cargo.toml`" — the
    /// marker `abi_guard::find_monorepo_root` already uses — plus CLI sources.
    fn fake_checkout(root: &Path) {
        fs::create_dir_all(root.join("packages/core/nros-core")).unwrap();
        fs::write(root.join("packages/core/nros-core/Cargo.toml"), "").unwrap();
        fs::create_dir_all(root.join("packages/cli/target/release")).unwrap();
        fs::write(root.join("packages/cli/Cargo.toml"), "").unwrap();
    }

    fn stray(tmp: &Path) -> PathBuf {
        let p = tmp.join("elsewhere/nros");
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, "").unwrap();
        p
    }

    /// The whole point of phase-431 W1: a binary from outside the tree is
    /// refused when it is run AGAINST that tree.
    #[test]
    fn a_foreign_binary_is_refused_against_a_checkout() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("checkout");
        fake_checkout(&root);
        let err = refuse_if_foreign_to_workspace(&stray(tmp.path()), &root)
            .expect_err("a binary outside the checkout must be refused");
        assert!(
            err.contains("does not belong to the checkout"),
            "the message must say WHICH problem this is: {err}"
        );
        // phase-368 / `check-emitter-just-spelling`: a user-reachable message
        // that names a `just` recipe must name the USER spelling beside it.
        // This message becomes MORE user-reachable once a binary ships — a
        // user who wanders into a checkout is exactly who sees it.
        assert!(
            err.contains("./scripts/bootstrap.sh") && err.contains("just setup-cli"),
            "the remedy must carry both spellings, user first: {err}"
        );
    }

    /// The negative control, and the case the original exemption existed to
    /// protect: a released binary building a USER's project must not be
    /// refused. Without this, shipping would break every out-of-tree consumer.
    #[test]
    fn a_foreign_binary_is_fine_outside_any_checkout() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("user-project");
        fs::create_dir_all(&project).unwrap();
        assert!(refuse_if_foreign_to_workspace(&stray(tmp.path()), &project).is_ok());
    }

    /// The checkout's own build is what the tree wants, so it passes this check
    /// and goes on to the staleness comparison.
    #[test]
    fn the_checkouts_own_binary_passes() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("checkout");
        fake_checkout(&root);
        let own = root.join("packages/cli/target/release/nros");
        fs::write(&own, "").unwrap();
        assert!(refuse_if_foreign_to_workspace(&own, &root).is_ok());
    }

    /// A tree with the runtime marker but no CLI sources cannot expect a CLI to
    /// be built from it, so there is nothing to be foreign to. Without this arm
    /// a consumer vendoring `packages/core` would be refused.
    #[test]
    fn a_tree_with_no_cli_sources_is_not_a_checkout_for_this_purpose() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("vendored");
        fs::create_dir_all(root.join("packages/core/nros-core")).unwrap();
        fs::write(root.join("packages/core/nros-core/Cargo.toml"), "").unwrap();
        assert!(refuse_if_foreign_to_workspace(&stray(tmp.path()), &root).is_ok());
    }

    /// A subdirectory of the checkout is still the checkout — the walk-up is
    /// what makes this usable from anywhere in the tree.
    #[test]
    fn a_subdirectory_of_the_checkout_still_counts() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("checkout");
        fake_checkout(&root);
        let deep = root.join("examples/native/rust/talker");
        fs::create_dir_all(&deep).unwrap();
        assert!(refuse_if_foreign_to_workspace(&stray(tmp.path()), &deep).is_err());
    }
}
