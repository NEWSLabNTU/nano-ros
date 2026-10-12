# shellcheck shell=sh
# The variables activation deliberately does NOT export into a user's shell —
# ONE derivation, sourced by `scripts/sdk-env.sh` (activation) and
# `scripts/build/cmake-incremental.sh` (a build run outside `just`).
#
#   - the index's `[source.*] env` names (phase-484 W2c, RFC-0103 D4): a source
#     tree is LOCATED by its resolver, never carried in the shell;
#   - the `# just-only:` line in `just/sdk-env.just` (RFC-0103 D6, amended
#     2026-10-12): the checkout root, `NROS_REPO_DIR` and `nano_ros_ROOT`.
#
# Issue 1840 is why a second caller needs the list: a name activation SKIPS is
# also a name it never RE-ROOTS (issue 1280's rule, `checkout-paths.sh`), so an
# inherited value naming ANOTHER checkout survived activation untouched, and
# `cmake-incremental.sh` — which trusted any set `nano_ros_ROOT` — configured a
# worktree's fixtures against the main checkout's runtime. Both callers now
# classify these names by the one rule; neither restates which names they are.
#
# Sourced from bash, zsh and plain POSIX sh: no arrays, no `[[ ]]`.

# nros_unexported_env_names <checkout-root>
#
# One name per line, derived from that checkout's index and `sdk-env.just`.
nros_unexported_env_names() {
    sed -n 's/^env = "\([A-Za-z_][A-Za-z0-9_]*\)"$/\1/p' \
        "${1:?nros_unexported_env_names: no checkout root}/nros-sdk-index.toml"
    sed -n 's/^# just-only:[[:space:]]*//p' "$1/just/sdk-env.just" |
        tr ' ' '\n' | sed '/^$/d'
}
