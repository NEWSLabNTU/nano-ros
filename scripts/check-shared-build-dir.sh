#!/usr/bin/env bash
# check-shared-build-dir — phase-559 W10's negative controls, on REAL cargo.
#
# The metadata probe builds every C/C++ workspace's generated image root with
# its own Corrosion target dir and ONE cargo `build.build-dir` per shared-cargo
# key, and after every build runs `nros dev shared-build-dir-check`
# (orchestration/shared_build_dir.rs). That check exists because two failures
# were measured on cargo 1.99 before any of this was written. This gate
# reproduces BOTH on scratch crates and requires the check to refuse each, and
# then builds the shape the generator emits and requires it to pass. A check
# that cannot fail on the cases it was written for proves nothing.
#
#   collision  Two roots with one name and version and different code, one
#              build-dir. Cargo spells a root relative to itself, so both get
#              ONE unit hash; the second build found it fresh and uplifted the
#              FIRST root's archive. Must be refused (marker check).
#   spellings  One crate reached as a workspace MEMBER by one root and as an
#              external path dep by another, one build-dir: two units that
#              differ only in path spelling, issue 0616's duplicate-symbol
#              shape. Must be refused (census).
#   generated  Distinct `+r<id>` version suffix and marker per root, every
#              other crate an external path dep: what NanoRosImageCrate.cmake
#              emits. Must pass, AND each archive must hold its own code.
#   nested     A build script that runs cargo, under the shared build-dir.
#              Cargo holds the build-dir lock while it waits on a build
#              script, so a nested `cargo build` that inherits
#              CARGO_BUILD_BUILD_DIR deadlocks silently (the probe hung 3 h).
#              Through `nros_sizes_build::nested_cargo()` it must finish.
#              With a raw `$CARGO` it must TIME OUT (the negative control),
#              and a timeout must leave the lock free again.
#
# `--offline`. The first three scenarios are a few small rustc runs. `nested`
# also builds nros-sizes-build and its two registry deps, with the lock seeded
# from the workspace's own.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/build/cargo.sh
source "$repo/scripts/build/cargo.sh"   # nros_cli_bin

fail() { echo "check-shared-build-dir: FAIL -- $*" >&2; exit 1; }

# No CLI, no gate -- RECORDED through the shared ledger, never silent, so the
# fast tier keeps running green on a pristine worktree (the entity-census
# contract). A CLI built before the check existed is the same case.
nros=""
if ! nros="$(nros_cli_bin --checkout-only)" \
    || ! "$nros" dev shared-build-dir-check --help >/dev/null 2>&1; then
    # shellcheck source=scripts/build/check-skip.sh
    source "$repo/scripts/build/check-skip.sh"
    nros_check_skip shared-build-dir \
        "no in-tree nros CLI with \`dev shared-build-dir-check\` (just setup-cli)"
    exit 0
fi

wrapper="${RUSTC_WRAPPER:-}"   # restored for `nested` only; it redirects nothing
mkdir -p "$repo/tmp"
work="$(mktemp -d "$repo/tmp/check-shared-build-dir.XXXXXX")"
trap '[ -n "${KEEP:-}" ] || rm -rf "$work"' EXIT
# Nothing inherited may redirect the scratch builds.
unset CARGO_TARGET_DIR CARGO_BUILD_TARGET_DIR CARGO_BUILD_BUILD_DIR CARGO_BUILD_TARGET RUSTC_WRAPPER

# crate <dir> <name> <version> <lib.rs body> <dep line> [members]
crate() {
    local dir="$1" name="$2" ver="$3" body="$4" dep="$5" members="${6:-}"
    mkdir -p "$dir/src"
    printf '[package]\nname = "%s"\nversion = "%s"\nedition = "2021"\n\n[lib]\ncrate-type = ["staticlib"]\n\n[dependencies]\n%s\n\n[workspace]\n%s\n\n[profile.release]\npanic = "abort"\n' \
        "$name" "$ver" "$dep" "$members" > "$dir/Cargo.toml"
    printf '%s\n' "$body" > "$dir/src/lib.rs"
}
# depcrate <dir> -- the plain rlib every root depends on.
depcrate() {
    mkdir -p "$1/src"
    printf '[package]\nname = "dep"\nversion = "0.1.0"\nedition = "2021"\n' > "$1/Cargo.toml"
    printf 'pub fn f() -> u32 { 1 }\n' > "$1/src/lib.rs"
}
# root_body <id> <fn>
root_body() {
    printf '#[used]\nstatic _NROS_IMAGE_ROOT: [u8; 32] = *b"nros-image-root:%s";\n#[unsafe(no_mangle)]\npub extern "C" fn %s() -> u32 { dep::f() }\n' "$1" "$2"
}

# One probe-shaped build dir: <b>/cargo -> <b>/key (the shared redirect), the
# build-dir under it, a target dir per root, a record per root.
# build_root <b> <root-dir> <id>
build_root() {
    local b="$1" root="$2" id="$3" tdir
    mkdir -p "$b/key" "$b/nros-image-roots"
    [ -L "$b/cargo" ] || ln -s key "$b/cargo"
    tdir="$b/cargo/$(basename "$root")"
    ( cd "$root" && CARGO_BUILD_BUILD_DIR="$b/cargo/.build-dir" \
        cargo build -q --release --offline --target-dir "$tdir" ) \
        || fail "cargo build of $(basename "$root") failed"
    printf '%s\n' "$tdir/release/libimg.a" > "$b/nros-image-roots/$id.txt"
}

dep_line_ext() { printf 'dep = { path = "%s" }' "$1"; }

# The negative controls: the two shapes the check exists for, each of which it
# must REFUSE. Run on every invocation, never behind a flag.
selftest() {
# --- collision: same name+version, different code ---------------------------
s="$work/collision"
depcrate "$s/dep"
crate "$s/a" img 0.1.0 "$(root_body aaaaaaaaaaaaaaaa g_a)" "$(dep_line_ext "$s/dep")"
crate "$s/b" img 0.1.0 "$(root_body bbbbbbbbbbbbbbbb g_b)" "$(dep_line_ext "$s/dep")"
build_root "$s/build" "$s/b" bbbbbbbbbbbbbbbb
build_root "$s/build" "$s/a" aaaaaaaaaaaaaaaa
if "$nros" dev shared-build-dir-check "$s/build" > "$work/collision.out" 2>&1; then
    cat "$work/collision.out" >&2
    fail "collision: two same-named roots shared one unit and the check PASSED"
fi
grep -q "another build's root unit" "$work/collision.out" \
    || { cat "$work/collision.out" >&2; fail "collision: refused for the wrong reason"; }

# --- spellings: one crate, member of one root, external dep of another -------
s="$work/spellings"
depcrate "$s/c/dep"
# `dep` inside c/ as a member: the issue-0616 relative spelling.
crate "$s/c" img 0.1.0+rcccccccccccccccc "$(root_body cccccccccccccccc g_c)" 'dep = { path = "dep" }' 'members = ["dep"]'
crate "$s/a" img 0.1.0+raaaaaaaaaaaaaaaa "$(root_body aaaaaaaaaaaaaaaa g_a)" "$(dep_line_ext "$s/c/dep")"
build_root "$s/build" "$s/c" cccccccccccccccc
build_root "$s/build" "$s/a" aaaaaaaaaaaaaaaa
if "$nros" dev shared-build-dir-check "$s/build" > "$work/spellings.out" 2>&1; then
    cat "$work/spellings.out" >&2
    fail "spellings: one crate reached by two spellings and the check PASSED"
fi
grep -q "crate \`dep\`" "$work/spellings.out" \
    || { cat "$work/spellings.out" >&2; fail "spellings: refused for the wrong reason"; }

}
selftest

# --- generated: the shape the generator emits --------------------------------
s="$work/generated"
depcrate "$s/dep"
crate "$s/a" img 0.0.0+raaaaaaaaaaaaaaaa "$(root_body aaaaaaaaaaaaaaaa g_a)" "$(dep_line_ext "$s/dep")"
crate "$s/b" img 0.0.0+rbbbbbbbbbbbbbbbb "$(root_body bbbbbbbbbbbbbbbb g_b)" "$(dep_line_ext "$s/dep")"
build_root "$s/build" "$s/b" bbbbbbbbbbbbbbbb
build_root "$s/build" "$s/a" aaaaaaaaaaaaaaaa
"$nros" dev shared-build-dir-check "$s/build" > "$work/generated.out" 2>&1 \
    || { cat "$work/generated.out" >&2; fail "generated: the emitted shape was refused"; }
for pair in a:g_a b:g_b; do
    r="${pair%%:*}" sym="${pair#*:}"
    # Captured first: `grep -q` closing the pipe early SIGPIPEs nm, which
    # `pipefail` would read as a missing symbol.
    syms="$(nm "$s/build/cargo/$r/release/libimg.a" 2>/dev/null || true)"
    grep -q " T $sym\$" <<< "$syms" \
        || fail "generated: root $r's archive does not define its own $sym"
done
n=$(find "$s/build/key/.build-dir" -path '*/.fingerprint/dep-*' -maxdepth 3 -type d | wc -l)
[ "$n" -eq 1 ] || fail "generated: the shared dep is $n units, expected 1 (nothing shared)"

# --- nested: a cargo run from a build script, under the shared build-dir -----
# bounded <secs> <log> <cmd...> -- its own process group, so a timeout reaps
# the build script and the cargo it nested too, not only the outer cargo.
bounded() {
    local secs="$1" log="$2" pid rc=0
    shift 2
    setsid bash -c 'exec timeout -k 5 "$0" "$@"' "$secs" "$@" > "$log" 2>&1 &
    pid=$!
    wait "$pid" || rc=$?
    kill -KILL -- "-$pid" 2>/dev/null || true
    return "$rc"
}
s="$work/nested"
b="$s/build"
mkdir -p "$s/inner/src" "$s/root/src" "$b/key"
ln -s key "$b/cargo"
printf '[package]\nname = "inner"\nversion = "0.1.0"\nedition = "2021"\n\n[workspace]\n' > "$s/inner/Cargo.toml"
printf 'pub fn f() {}\n' > "$s/inner/src/lib.rs"
printf '[package]\nname = "nestroot"\nversion = "0.1.0"\nedition = "2021"\nbuild = "build.rs"\n\n[build-dependencies]\nnros-sizes-build = { path = "%s" }\n\n[workspace]\n' \
    "$repo/packages/tooling/nros-sizes-build" > "$s/root/Cargo.toml"
printf '\n' > "$s/root/src/lib.rs"
cp "$repo/Cargo.lock" "$s/root/Cargo.lock"   # a throwaway seed: resolves what the workspace does, offline
cat > "$s/root/build.rs" <<'RS'
use std::process::Command;
fn main() {
    println!("cargo:rerun-if-env-changed=NROS_NESTED_RAW");
    let inner = concat!(env!("CARGO_MANIFEST_DIR"), "/../inner/Cargo.toml");
    let out = std::env::var("OUT_DIR").unwrap();
    let mut cmd = if std::env::var_os("NROS_NESTED_RAW").is_some() {
        Command::new(std::env::var_os("CARGO").unwrap())
    } else {
        nros_sizes_build::nested_cargo()
    };
    let st = cmd
        .args(["build", "-q", "--offline", "--manifest-path", inner, "--target-dir"])
        .arg(format!("{out}/inner"))
        .status()
        .unwrap();
    assert!(st.success(), "the nested cargo failed");
}
RS
nested_build() {   # nested_build <secs> <log> [VAR=value...]
    local secs="$1" log="$2"
    shift 2
    (
        cd "$s/root"
        export CARGO_BUILD_BUILD_DIR="$b/cargo/.build-dir"
        [ -z "$wrapper" ] || export RUSTC_WRAPPER="$wrapper"
        [ "$#" -eq 0 ] || export "$@"
        bounded "$secs" "$log" cargo build -q --offline --target-dir "$b/cargo/root"
    )
}
nested_build 900 "$work/nested.out" \
    || { cat "$work/nested.out" >&2; fail "nested: a build script's cargo through nested_cargo() did not finish under the shared build-dir"; }
rc=0
nested_build 20 "$work/nested-raw.out" NROS_NESTED_RAW=1 || rc=$?
[ "$rc" -eq 124 ] || { cat "$work/nested-raw.out" >&2; \
    fail "nested: the negative control (a raw \$CARGO under the shared build-dir) exited $rc instead of deadlocking into the 20 s timeout -- this scenario can no longer tell the fix from its absence"; }
# The rebuild gets the first build's budget, not a shorter one: the control
# killed cargo mid-build, so this run recompiles what was interrupted, and on a
# loaded host that took over the 120 s it once had (2026-11-15, beside a
# concurrent `ci gate`). A lock that stayed held still ends here, as a 900 s
# timeout, and the two outcomes are reported apart.
rc=0
nested_build 900 "$work/nested-again.out" || rc=$?
if [ "$rc" -eq 124 ]; then
    cat "$work/nested-again.out" >&2
    fail "nested: after the control's timeout the rebuild did not finish in 900 s -- the build-dir lock was not freed when the control's process group was killed"
elif [ "$rc" -ne 0 ]; then
    cat "$work/nested-again.out" >&2
    fail "nested: after the control's timeout the rebuild FAILED (exit $rc), see its output above"
fi

echo "check-shared-build-dir: OK -- a same-named root collision and a two-spelling crate are both refused; the generated shape passes, each archive holds its own code, and the shared dep is one unit; a build script's nested cargo finishes through nested_cargo() and deadlocks without it (the control)"
