#!/bin/bash
# tests/census-prepass-parallel-tests.sh -- phase-530 W2 (Ruling R30, fix round 2)
#
# `scripts/build/census-prepass.sh` takes the leaf censuses side by side. Run
# against a FAKE checkout (the real script, stub `cargo.sh` / `leaf-cache.sh` /
# `fixtures-manifest.py`, a stub `nros` that fails for leaves named `bad*`),
# so no census is built and no disk is spent:
#
#   1. two of four leaves fail -> exit 1, BOTH named, each with the TAIL of its
#      log (the last lines, not only a path);
#   2. a failed `nros_cmake_frontend_jobs` -> exit 1 with a message, never a
#      take at width "";
#   3. a bash without `wait -p` (< 5.1) -> refused up front with a clear
#      message (`BASH_VERSINFO` is read-only, so the script reads its version
#      through `NROS_CENSUS_PREPASS_FAKE_BASH`, a test seam, when set);
#   4. all leaves pass -> exit 0.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
fail=0

fake="$WORK/repo"
mkdir -p "$fake/scripts/build" "$fake/bin"
cp "$ROOT/scripts/build/census-prepass.sh" "$fake/scripts/build/"
cat > "$fake/scripts/build/cargo.sh" <<'SH'
nros_cmake_frontend_jobs() {
    if [ -n "${FAKE_WIDTH_FAILS:-}" ]; then return 1; fi
    echo 2
}
nros_cargo_inner_jobs() { echo 1; }
SH
cat > "$fake/scripts/build/leaf-cache.sh" <<SH
nros_cli_bin() { echo "$fake/bin/nros"; }
SH
cat > "$fake/scripts/build/fixtures-manifest.py" <<'PY'
import os
print(os.environ.get("FAKE_LEAVES", "").replace(",", "\n"))
PY
cat > "$fake/bin/nros" <<'SH'
#!/bin/bash
leaf="${@: -1}"
for i in $(seq 1 30); do echo "noise line $i of $(basename "$leaf")"; done
case "$(basename "$leaf")" in
    bad*) echo "LAST-LINE-OF-$(basename "$leaf")"; exit 7 ;;
esac
SH
chmod +x "$fake/bin/nros"

expect() {
    if [[ "$out" != *"$2"* ]]; then
        echo "FAIL [$1]: expected \`$2\` in:"; echo "$out" | sed 's/^/    /'; fail=1
    fi
}

# 1. two failures among four: both named, with their logs' tails.
out="$(env NROS_BUILD_LOG_DIR=logs FAKE_LEAVES=ok1,bad1,ok2,bad2 bash "$fake/scripts/build/census-prepass.sh" --only leaves 2>&1)" && rc=0 || rc=$?
[ "$rc" = 1 ] || { echo "FAIL [1]: exit $rc, want 1"; fail=1; }
expect 1 "2 leaf census(es) FAILED"
expect 1 "LAST-LINE-OF-bad1"
expect 1 "LAST-LINE-OF-bad2"
if [[ "$out" == *"noise line 5 of bad1"* ]]; then
    echo "FAIL [1]: printed the whole log, not its tail"; fail=1
fi

# 2. the width helper fails: refused, loudly.
out="$(env NROS_BUILD_LOG_DIR=logs FAKE_LEAVES=ok1 FAKE_WIDTH_FAILS=1 bash "$fake/scripts/build/census-prepass.sh" --only leaves 2>&1)" && rc=0 || rc=$?
[ "$rc" = 1 ] || { echo "FAIL [2]: exit $rc, want 1"; fail=1; }
expect 2 "nros_cmake_frontend_jobs failed"

# 3. a bash with no `wait -p`: refused up front, naming the version.
out="$(env NROS_BUILD_LOG_DIR=logs FAKE_LEAVES=ok1 NROS_CENSUS_PREPASS_FAKE_BASH=5.0 bash "$fake/scripts/build/census-prepass.sh" --only leaves 2>&1)" && rc=0 || rc=$?
[ "$rc" = 1 ] || { echo "FAIL [3]: exit $rc, want 1"; fail=1; }
expect 3 "needs bash 5.1"

# 4. all pass.
out="$(env NROS_BUILD_LOG_DIR=logs FAKE_LEAVES=ok1,ok2,ok3 bash "$fake/scripts/build/census-prepass.sh" --only leaves 2>&1)" && rc=0 || rc=$?
[ "$rc" = 0 ] || { echo "FAIL [4]: exit $rc, want 0"; echo "$out"; fail=1; }

if [ "$fail" -ne 0 ]; then
    exit 1
fi
echo "census-prepass-parallel-tests: OK (failures named with log tails, width failure and old bash refused)"
