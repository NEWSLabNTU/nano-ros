#!/usr/bin/env bash
# phase-431 W4 — `scripts/install.sh` must INSTALL, and must REFUSE.
#
# This is the script a user runs from a curl pipe on a machine with nothing
# provisioned, which makes its refusals the interesting half: a corrupted or
# unsigned asset that installs anyway is worse than one that fails, because the
# binary it leaves behind emits code into someone's workspace. A check nobody
# has watched fail is a check nobody has evidence still works.
#
# Everything is served from a LOOPBACK http server over a tarball built here
# from the checkout's own `nros`, so the test needs no release to exist and
# reaches no network. That is also why `NROS_INSTALL_URL` relaxes the scheme
# restriction — see the comment beside it in the script.
#
#   1. happy path       -> installed at <store>/nros/<version>, fronted, runs
#   2. version pinning  -> the prefix comes from the asset's share/nros/VERSION,
#                          not from `nros --version` (they differ by `-nrosN`)
#   3. bad checksum     -> refuses, and installs NOTHING
#   4. absent .sha256   -> refuses (unverified is not a fallback)
#   5. no asset at all  -> refuses, naming the source build as the way forward
#   6. manifest read    -> the INSTALLED binary reads share/nros/manifest.toml
#                          back out of its prefix (RFC-0097 D7); no component
#                          version is ever parsed out of an asset or dir name
#   7. codegen delta    -> `nros pin` across two installed toolchains: equal
#                          codegen re-emits nothing and does not warn; a
#                          different one warns, names `nros sync`, and (under
#                          --dry-run) has written nothing when it does
#   8. pre-W2 asset     -> VERSION alone still installs, and the binary reports
#                          that the release declares nothing rather than guessing
set -uo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# issue 0726 — `nros_grep_q` exits 2 when grep itself fails to run, so a forked
# grep that could not start under a parallel fan-out cannot be read here as
# "the installer did not print that". Every assertion below is about the
# ABSENCE of a string, which is exactly the shape that conflation corrupts.
# shellcheck source=scripts/lib/grep-q.sh
source "$root/scripts/lib/grep-q.sh"
installer="$root/scripts/install.sh"
cli="$root/packages/cli/target/release/nros"

FAILURES=0
fail() { echo "FAIL: $*" >&2; FAILURES=$((FAILURES + 1)); }
ok() { echo "  [OK] $*"; }
# For an arm whose assertions are a straight sequence rather than an if/else:
# `ok` after a `fail` would print a pass line for an arm that failed, which is
# exactly the shape `check-no-vacuous-tests` exists to stop one level up.
arm_start() { _arm_failures="$FAILURES"; }
arm_ok() { [ "$FAILURES" -eq "$_arm_failures" ] && ok "$*"; }

[ -x "$cli" ] || { echo "SKIP: no in-tree nros at $cli (just setup-cli)" >&2; exit 0; }
for tool in python3 curl tar zstd; do
    command -v "$tool" >/dev/null || { echo "SKIP: $tool not on PATH" >&2; exit 0; }
done

tmp="$(mktemp -d "${TMPDIR:-/tmp}/nros-installer-test.XXXXXX")"
srv_pid=""
cleanup() {
    [ -n "$srv_pid" ] && kill "$srv_pid" 2>/dev/null
    rm -rf "$tmp"
}
trap cleanup EXIT INT TERM

# Build the asset in the shape a release has: prefix-rooted, with the store
# version beside the binary — and, since RFC-0097 D7, the manifest that records
# what the release is MADE OF.
#
# The manifest is stamped by the same verb `release-nros.yml` calls, so this
# exercises the real emit path rather than a hand-written imitation of it: the
# `codegen` number comes from the binary's own constant, which is the property
# that makes the release-time equality check meaningful.
mkdir -p "$tmp/serve" "$tmp/rel/bin" "$tmp/rel/share/nros"
cp "$cli" "$tmp/rel/bin/nros"
echo "9.9.9-nros7" >"$tmp/rel/share/nros/VERSION"
# The codegen number the PACKAGED binary emits, read here rather than inside
# arm 6. Arm 7 needs it, and under `set -u` a value defined only on arm 6's
# success branch takes the whole script down when an EARLIER arm failed — one
# broken arm would then hide every arm after it, which is the reverse of what a
# multi-arm test is for. Arm 6 still asserts the INSTALLED binary reports the
# same number, which is the claim that arm is about.
emitted="$("$cli" --codegen-version)"
"$cli" toolchain manifest \
    --store-version 9.9.9-nros7 \
    --index 2026-09-10 \
    --nano-ros abc1234 \
    --write "$tmp/rel/share/nros/manifest.toml" >/dev/null \
    || { echo "SKIP: this nros has no \`toolchain manifest\` (rebuild: just setup-cli)" >&2; exit 0; }
tar --zstd -cf "$tmp/serve/nros-asset.tar.zst" -C "$tmp/rel" bin share
( cd "$tmp/serve" && sha256sum nros-asset.tar.zst >nros-asset.tar.zst.sha256 )

# A PRE-W2 asset: `share/nros/VERSION` and no manifest. That is a real state on
# any host that installed before D7 landed, and the installer must be
# indifferent to it — `install.sh` chooses the prefix from VERSION, in POSIX
# shell, before anything is unpacked, and must never need a TOML parser.
mkdir -p "$tmp/legacy/bin" "$tmp/legacy/share/nros"
cp "$cli" "$tmp/legacy/bin/nros"
echo "9.9.8-nros1" >"$tmp/legacy/share/nros/VERSION"
tar --zstd -cf "$tmp/serve/legacy-asset.tar.zst" -C "$tmp/legacy" bin share
( cd "$tmp/serve" && sha256sum legacy-asset.tar.zst >legacy-asset.tar.zst.sha256 )

# Port 0 lets the OS choose, so parallel runs of this test cannot collide — the
# fixed-port version of this file would have been a flake generator.
python3 -c '
import http.server, os, socketserver, sys, threading
os.chdir(sys.argv[1])
srv = socketserver.TCPServer(("127.0.0.1", 0), http.server.SimpleHTTPRequestHandler)
print(srv.server_address[1], flush=True)
srv.serve_forever()
' "$tmp/serve" >"$tmp/port" 2>/dev/null &
srv_pid=$!
port=""
for _ in $(seq 1 50); do
    port="$(cat "$tmp/port" 2>/dev/null)"
    [ -n "$port" ] && break
    sleep 0.1
done
[ -n "$port" ] || { echo "FAIL: loopback server never reported a port" >&2; exit 1; }
base="http://127.0.0.1:$port"

run_install() {
    local home="$1" url="$2"
    NROS_HOME="$home" NROS_INSTALL_URL="$url" sh "$installer" >"$home.log" 2>&1
}

# --- 1 + 2: it installs, at the version the ASSET names -------------------
home="$tmp/h1"
if run_install "$home" "$base/nros-asset.tar.zst"; then
    prefix="$home/sdk/nros/9.9.9-nros7"
    [ -x "$prefix/bin/nros" ] || fail "1: no binary at $prefix/bin/nros"
    [ -f "$prefix/.nros-provenance" ] || fail "1: no provenance marker"
    [ -L "$home/bin/nros" ] || fail "1: $home/bin/nros is not a symlink"
    [ "$(readlink "$home/bin/nros")" = "$prefix/bin/nros" ] \
        || fail "1: front link points at $(readlink "$home/bin/nros")"
    "$home/bin/nros" --codegen-version >/dev/null 2>&1 \
        || fail "1: the fronted binary does not run"
    # The whole point of arm 2: `nros --version` says 0.5.0, the asset says
    # 9.9.9-nros7, and the STORE must use the asset's — otherwise a later
    # `nros setup --tool nros` installs a second copy under the other name.
    [ -d "$prefix" ] || fail "2: prefix is not the asset's version"
    got="$(ls "$home/sdk/nros")"
    [ "$got" = "9.9.9-nros7" ] || fail "2: store holds [$got], not just the asset's version"
    ok "installs at the asset's version, fronts it, and it runs"
else
    fail "1: install failed
$(cat "$home.log")"
fi

# --- 3: a corrupted asset installs NOTHING --------------------------------
cp "$tmp/serve/nros-asset.tar.zst.sha256" "$tmp/serve/keep.sha256"
printf 'deadbeef  nros-asset.tar.zst\n' >"$tmp/serve/nros-asset.tar.zst.sha256"
home="$tmp/h2"
if run_install "$home" "$base/nros-asset.tar.zst"; then
    fail "3: a checksum mismatch INSTALLED"
else
    nros_grep_q "checksum MISMATCH" "$home.log" || fail "3: refused without naming the cause"
    [ -e "$home" ] && fail "3: refused but left $home behind"
    ok "a checksum mismatch refuses, and installs nothing"
fi
cp "$tmp/serve/keep.sha256" "$tmp/serve/nros-asset.tar.zst.sha256"

# --- 4: no checksum beside the asset is not a fallback --------------------
cp "$tmp/serve/nros-asset.tar.zst" "$tmp/serve/unsigned.tar.zst"
home="$tmp/h3"
if run_install "$home" "$base/unsigned.tar.zst"; then
    fail "4: an asset with no .sha256 INSTALLED"
else
    nros_grep_q "refusing to install unverified" "$home.log" \
        || fail "4: refused for the wrong reason"
    ok "an asset with no .sha256 refuses"
fi

# --- 5: nothing to download says what to do instead -----------------------
home="$tmp/h4"
if run_install "$home" "$base/absent.tar.zst"; then
    fail "5: a 404 INSTALLED"
else
    nros_grep_q "bootstrap.sh" "$home.log" \
        || fail "5: a missing release must name the source build as the way forward"
    ok "a missing release names the source build"
fi

# --- 6: the installed binary READS its own manifest, from the file ---------
# RFC-0097 D7's "read by the CLI, not parsed out of a filename": the asset is
# `nros-asset.tar.zst` and the prefix is `9.9.9-nros7`, so neither name carries
# a codegen number. The only place it can come from is share/nros/manifest.toml.
home="$tmp/h1"
installed="$home/sdk/nros/9.9.9-nros7/bin/nros"
if [ -x "$installed" ]; then
    arm_start
    [ "$("$installed" --codegen-version)" = "$emitted" ] \
        || fail "6: the installed binary emits a different codegen than the one packaged"
    "$installed" toolchain manifest >"$tmp/read-back.txt" 2>&1
    nros_grep_q "^codegen = $emitted$" "$tmp/read-back.txt" \
        || fail "6: the installed binary does not read back its own codegen
$(cat "$tmp/read-back.txt")"
    nros_grep_q "^index = \"2026-09-10\"$" "$tmp/read-back.txt" \
        || fail "6: the manifest lost its index field in transit"
    arm_ok "the installed binary reads share/nros/manifest.toml back from the prefix"
else
    fail "6: arm 1 left no installed binary to read a manifest from"
fi

# --- 7: same codegen -> no re-emit; different codegen -> warns FIRST -------
# D7's acceptance, end to end through the real binary. The second toolchain is
# fabricated in the store rather than installed, because what is under test is
# the COMPARISON, and two releases that differ only in `codegen` cannot be built
# from one checkout.
arm_start
store="$home"
same="$store/sdk/nros/9.9.9-nros8"
newer="$store/sdk/nros/9.9.9-nros9"
mkdir -p "$same/share/nros" "$newer/share/nros"
"$cli" toolchain manifest --store-version 9.9.9-nros8 --write "$same/share/nros/manifest.toml" >/dev/null
sed "s/^codegen = .*/codegen = $((emitted + 1))/" "$tmp/rel/share/nros/manifest.toml" \
    | sed 's/^version = .*/version = "9.9.9-nros9"/' >"$newer/share/nros/manifest.toml"

proj="$tmp/proj"
mkdir -p "$proj"
printf '[toolchain]\nversion = "9.9.9-nros7"\n' >"$proj/nros-toolchain.toml"

"$cli" pin --dir "$proj" --root "$store" --dry-run 9.9.9-nros8 >"$tmp/pin-same.txt" 2>&1 \
    || fail "7: \`nros pin\` failed
$(cat "$tmp/pin-same.txt")"
if nros_grep_q "warning" "$tmp/pin-same.txt"; then
    fail "7: two releases declaring the same codegen must not warn
$(cat "$tmp/pin-same.txt")"
fi
nros_grep_q "UNCHANGED" "$tmp/pin-same.txt" \
    || fail "7: the same-codegen verdict is not stated
$(cat "$tmp/pin-same.txt")"

"$cli" pin --dir "$proj" --root "$store" --dry-run 9.9.9-nros9 >"$tmp/pin-diff.txt" 2>&1 \
    || fail "7: \`nros pin\` failed on a codegen change
$(cat "$tmp/pin-diff.txt")"
nros_grep_q "^warning: codegen $emitted -> $((emitted + 1))" "$tmp/pin-diff.txt" \
    || fail "7: a codegen change did not warn
$(cat "$tmp/pin-diff.txt")"
nros_grep_q "nros sync" "$tmp/pin-diff.txt" \
    || fail "7: the warning does not name the remedy"
# And it warned BEFORE it would have acted: --dry-run wrote nothing.
nros_grep_q '9\.9\.9-nros7' "$proj/nros-toolchain.toml" \
    || fail "7: --dry-run moved the pin"
arm_ok "same codegen: no re-emit; different codegen: warns, names the remedy, writes nothing"

# --- 8: a pre-W2 asset still installs, and says what it cannot answer ------
home="$tmp/h5"
if run_install "$home" "$base/legacy-asset.tar.zst"; then
    # `arm_ok`, not `ok`: measured 2026-09-10 against the S2 mutation below —
    # with the "declares nothing" sentence removed, this arm printed its FAIL
    # line AND an `[OK]` line for the same arm. The exit code was still right,
    # so the lie was only in the transcript, which is the half a reader reads.
    arm_start
    legacy="$home/sdk/nros/9.9.8-nros1/bin/nros"
    [ -x "$legacy" ] || fail "8: an asset with no manifest did not install"
    "$legacy" toolchain manifest >"$tmp/legacy-manifest.txt" 2>&1
    nros_grep_q "no share/nros/manifest.toml" "$tmp/legacy-manifest.txt" \
        || fail "8: a manifest-less release must SAY it declares nothing
$(cat "$tmp/legacy-manifest.txt")"
    arm_ok "a pre-RFC-0097 asset installs from VERSION alone, and declares nothing rather than guessing"
else
    fail "8: an asset with no manifest failed to install
$(cat "$home.log")"
fi

if [ "$FAILURES" -ne 0 ]; then
    echo "nros-installer-tests: $FAILURES failure(s)" >&2
    exit 1
fi
echo "nros-installer-tests: OK — installs, fronts, refuses everything unverified, and declares its components."
