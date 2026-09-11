# shellcheck shell=bash
# PROBE-OWNED check for the `installed` track (phase-447 A3, RFC-0099 D1),
# spliced in right after the book's install step (`--after-step`), in the same
# shell.
#
# WHY HERE AND NOT AT THE END
#
# Two things must be true of an installed release before anything is
# provisioned: that the machine has no nano-ros checkout (the whole claim of
# this track), and that the toolchain answers "where is the SDK root?" out of
# its own asset (phase-447 A1+A2). Both are checkable the moment `nros` is on
# PATH — `nros sdk-root` reads no index and opens no socket.
#
# Checking them at the END made them hostage to every step in between. With
# the release's SDK-root staging reverted, the probe died at `nros setup`
# (issue 1304: submodule sources cannot be provisioned without a checkout) —
# the SAME step and the same message as with it intact — so a regression of
# A1+A2 would have been indistinguishable from an already-open red. Here, a
# reverted A1+A2 fails on its own line: `nros sdk-root` has no answer.
#
# (a) matters more than it looks. The dead-end this probe gates survived
# because every person who could have noticed had a checkout the resolution
# ladder reached first; a probe that passed through a checkout would repeat
# that exactly, so it is asserted rather than assumed from the docker call.

echo '=== probe check: an installed release, and no checkout ==='
probe_fail() {
    echo "PROBE FAIL: $*" >&2
    exit 1
}

# --- no checkout, by every route the ladder has -------------------------------
[ -z "${NROS_REPO_DIR:-}" ] \
    || probe_fail "NROS_REPO_DIR is set ($NROS_REPO_DIR) — the checkout rung could answer"
[ ! -e /nano-ros-src ] \
    || probe_fail "/nano-ros-src exists — the installed track must not mount the repo"
# The marker `nros_launcher::checkout::MONOREPO_MARKER` names. The one copy
# allowed is the release's own SDK root under the store.
nros_store="$HOME/.nros"
marker_hits="$(find / -xdev \( -path /proc -o -path "$nros_store" \) -prune -o \
    -path '*/packages/core/nros-core/Cargo.toml' -print 2>/dev/null || true)"
[ -z "$marker_hits" ] \
    || probe_fail "a nano-ros root exists outside the store: $marker_hits"

command -v nros >/dev/null || probe_fail "nros is not on PATH after the install step"
# The release prefix is CONSTRUCTED from the binary on PATH, never found by
# globbing the store for a version (`check-sdk-store-not-enumerated`, issue
# 0625): `<store>/sdk/nros/<version>/bin/nros`, so stripping `/bin/nros` gives
# the prefix and its parent must be the store's `nros` directory.
nros_bin="$(readlink -f "$(command -v nros)")"
nros_prefix="${nros_bin%/bin/nros}"
[ "$nros_prefix" != "$nros_bin" ] && [ "${nros_prefix%/*}" = "$nros_store/sdk/nros" ] \
    || probe_fail "nros on PATH is $nros_bin, not a release installed under $nros_store/sdk/nros"

# --- the SDK root comes out of the release itself ------------------------------
# From $HOME, which is not a workspace: the walk-up rung finds nothing, so only
# the shipped rung can answer. `--explain` names the rung on stderr.
rc=0
sdk_root="$(cd "$HOME" && nros sdk-root --explain 2>/tmp/sdk-root.explain)" || rc=$?
cat /tmp/sdk-root.explain
[ "$rc" -eq 0 ] \
    || probe_fail "\`nros sdk-root\` exited $rc — this release carries no SDK root, so no scaffolded project can configure (RFC-0099 D1; staged by scripts/stage-sdk-root.sh)"
# `nros_grep_q` (scripts/lib/grep-q.sh, prepended to this file by
# run-bootstrap-probe.sh): a grep that fails to RUN exits 2 instead of reading
# as "the rung was not named" (issue 0726).
nros_grep_q "via this toolchain's own share/nano-ros" /tmp/sdk-root.explain \
    || probe_fail "the SDK root was answered by a rung other than the shipped one: $sdk_root"
# Not "some release's share/nano-ros" — THIS binary's own prefix.
[ "$sdk_root" = "$nros_prefix/share/nano-ros" ] \
    || probe_fail "the SDK root is $sdk_root, not the running release's own $nros_prefix/share/nano-ros"
# The launch resolver every workspace configure runs, at the rung
# `model_location::launch_resolver_bin` reads — and RUN, because it links
# libpython and "the file is there" is not "it starts".
"$nros_store/bin/nros-launch-resolve" --version >/dev/null \
    || probe_fail "\$NROS_HOME/bin/nros-launch-resolve is missing or does not start — every workspace configure needs it"
echo "probe: the SDK root is the release's own ($sdk_root); no checkout on this machine"
