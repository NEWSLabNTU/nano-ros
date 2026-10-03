// The ONE way the CLI's Rust drops a git-repository-local environment before
// spawning `git` — issue 1659, the Rust twin of `scripts/lib/git-hook-env.sh`
// and `scripts/lib/git_hook_env.py` (issues 0986/0988), and sharing their
// identifier, `nros_clear_inherited_git_env`, which is what
// `check-hook-repo-side-effects` credits in every language it reads.
//
// `include!`d — by `nros-cli-core/src/source_stamp.rs` (and through it by
// `nros-cli-core/build.rs`), by `nros-launch-resolve/build.rs`, and by
// `cargo-nano-ros`. So: no inner doc comments (`//!` is only legal at the top
// of a file) and no `use` (it would collide with the includer's imports);
// types are spelled out.
//
// ## What goes wrong without it (measured, issue 1659)
//
// `GIT_DIR` and its family override BOTH a path argument and `git -C`. Under
// `git bisect run`, git exports `GIT_DIR=<repo>/.git/worktrees/<name>`, so
// `git -C packages/cli/third-party/play_launch rev-parse HEAD` answered with
// the SUPERPROJECT's HEAD. `nros` baked that as its play_launch pin, and every
// `nros sync` then refused the resolver with a message that blamed the
// resolver — 25 consecutive bisect steps SKIPped. The same environment reaches
// the CLI from a git hook, and there the store provisioner's `git init <tmp>`
// would write into the caller's repository instead of building a temp one,
// which is issue 0986 itself.
//
// ## Why the list is ASKED of git
//
// `git rev-parse --local-env-vars` is git's own answer to "which variables
// make me act on a different repository". Every hand-written copy of that
// list in this tree had drifted (four in shell, one in Python that cleared
// four names of sixteen and leaked `GIT_OBJECT_DIRECTORY`), so a copy here
// would be a fifth.
//
// The query itself runs with every `GIT_*` variable removed: it touches no
// repository, but an inherited `GIT_DIR` naming a directory that is not one
// makes `rev-parse` die before it answers, and then nothing would be cleared
// in exactly the environment that needs it.
//
// Do NOT call this from code that is SUPPOSED to act on the repository the
// environment names; nothing in the CLI is.

/// The variables an inherited environment uses to point git at a DIFFERENT
/// repository, as git reports them. Empty when no `git` can be run, in which
/// case there is no git spawn to protect either.
pub fn nros_inherited_git_env_vars() -> &'static [String] {
    static VARS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    VARS.get_or_init(|| {
        let mut probe = std::process::Command::new("git");
        probe.args(["rev-parse", "--local-env-vars"]);
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("GIT_") {
                probe.env_remove(key);
            }
        }
        probe
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .split_whitespace()
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// Remove the inherited repository-local git environment from `cmd`, so a
/// `git` it runs answers about the repository its ARGUMENTS name. Returns
/// `cmd` for chaining.
#[allow(dead_code)] // each includer uses the half it needs
pub fn nros_clear_inherited_git_env(cmd: &mut std::process::Command) -> &mut std::process::Command {
    for var in nros_inherited_git_env_vars() {
        cmd.env_remove(var);
    }
    cmd
}

/// `git` (or the resolved path the caller passes), with the inherited
/// repository-local environment already removed — the spelling every `git`
/// spawn in the CLI goes through.
#[allow(dead_code)] // each includer uses the half it needs
pub fn nros_git_command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    nros_clear_inherited_git_env(&mut cmd);
    cmd
}
