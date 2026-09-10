#!/usr/bin/env bash
# Issue 1264 — `nros_platform_rmws` must tell a dead manifest parser apart
# from a platform that genuinely has no fixture rows. It used to run the
# parser with `2>/dev/null` and read empty output either way, so a python3
# with no `tomllib`/`tomli` (stock on Ubuntu 22.04) produced the identical
# confident-but-wrong "no fixture rows for platform 'threadx-riscv64'" — a
# name the manifest carries 42 times.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# shellcheck source=scripts/build/platform-rmws.sh
source scripts/build/platform-rmws.sh

fails=0

# _dead_parser_verdict <output> <rc>
#
# Pure predicate, separated from the real-function cases below so `self_test`
# can drive it against SYNTHETIC output — a checker whose predicate always
# says "ok" would pass every case with no code under test doing anything
# right. 0 ("this correctly reports a parser failure") requires all three:
# a non-zero rc, the parser's own text reaching the caller, and NONE of the
# "does not exist" messages a genuine empty-but-successful parse would print.
_dead_parser_verdict() {
    local out="$1" rc="$2"
    [ "$rc" -ne 0 ] || return 1
    case "$out" in
        *ModuleNotFoundError*) ;;
        *) return 1 ;;
    esac
    case "$out" in
        *"no row anywhere carries id"* | *"unknown platform"* | *"no fixture rows for platform"*)
            return 1
            ;;
    esac
    return 0
}

# self_test — negative control for `_dead_parser_verdict`, run unconditionally
# on the normal path (never behind a flag): a negative control nobody runs
# decays into a comment. Reproduces the exact pre-1264 message by hand rather
# than by breaking real code, so it demonstrates the PREDICATE can fail
# without depending on `platform-rmws.sh` staying broken to prove it.
self_test() {
    # The pre-1264 shape: `2>/dev/null` discarded the traceback and printed
    # only the typo message. The predicate must REJECT this.
    if _dead_parser_verdict \
        "nros_platform_rmws: no fixture rows for platform 'threadx-riscv64'" 1; then
        echo "[FAIL] self_test: predicate accepted the pre-1264 misdiagnosis" >&2
        return 1
    fi
    # rc=0 is never a parser failure, however the message reads.
    if _dead_parser_verdict "ModuleNotFoundError: No module named 'tomllib'" 0; then
        echo "[FAIL] self_test: predicate accepted rc=0 as a parser failure" >&2
        return 1
    fi
    # The traceback with neither typo message present — the fixed shape —
    # must pass.
    if ! _dead_parser_verdict \
        "nros_manifest_query: ... failed (exit 1):
  ModuleNotFoundError: No module named 'tomllib'" 1; then
        echo "[FAIL] self_test: predicate rejected the correct fixed shape" >&2
        return 1
    fi
    return 0
}
self_test || fails=$((fails + 1))

echo "check-platform-rmws: a real platform resolves its backends"
out="$(nros_platform_rmws linux)"
rc=$?
if [ "$rc" -ne 0 ] || [ -z "$out" ]; then
    echo "  FAIL  linux: expected rc=0 and non-empty output, got rc=${rc} out=${out}"
    fails=$((fails + 1))
else
    echo "  ok    linux -> $(tr '\n' ' ' <<<"$out")"
fi

echo "check-platform-rmws: a typo'd platform is reported as one"
out="$(nros_platform_rmws not-a-real-platform 2>&1)"
rc=$?
if [ "$rc" -eq 0 ]; then
    echo "  FAIL  typo'd platform returned rc=0"
    fails=$((fails + 1))
elif [[ "$out" != *"no fixture rows for platform"* ]]; then
    echo "  FAIL  typo'd platform: expected the 'no fixture rows' message, got: ${out}"
    fails=$((fails + 1))
else
    echo "  ok    typo'd platform reports 'no fixture rows' (rc=${rc})"
fi

echo "check-platform-rmws: a dead parser is reported as one (issue 1264)"

# Faked with a `python3` shadowing PATH rather than by uninstalling tomllib —
# the real failure is host-Python-version-dependent, and this gate must
# reproduce identically everywhere it runs.
fake_python_dir="$(mktemp -d)"
cat >"$fake_python_dir/python3" <<'FAKE_PY'
#!/usr/bin/env bash
echo "Traceback (most recent call last):" >&2
echo "ModuleNotFoundError: No module named 'tomllib'" >&2
exit 1
FAKE_PY
chmod +x "$fake_python_dir/python3"

out="$(PATH="$fake_python_dir:$PATH" nros_platform_rmws threadx-riscv64 2>&1)"
rc=$?
rm -rf "$fake_python_dir"

if _dead_parser_verdict "$out" "$rc"; then
    echo "  ok    dead parser reported as a parser failure, not as a missing platform (rc=${rc})"
else
    echo "  FAIL  dead parser was misreported — see issue 1264"
    echo "        rc: ${rc}"
    echo "        output: ${out}"
    fails=$((fails + 1))
fi

if [ "$fails" -ne 0 ]; then
    echo "check-platform-rmws: ${fails} case(s) failed" >&2
    exit 1
fi
echo "check-platform-rmws: OK"
