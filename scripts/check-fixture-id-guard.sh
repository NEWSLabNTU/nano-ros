#!/usr/bin/env bash
# Issue 0406 — assert that a fixture builder narrowed to a non-matching id
# fails, and that the cases which must stay green stay green.
#
# The bug this locks out is a SILENT one: exit 0 having built nothing. Nothing
# downstream can notice that, which is why it needs a gate rather than a
# convention. The distinction being pinned is that loudness is keyed on the
# SPELLING of the filter, not on the emptiness alone:
#
#   --id (flag) = this invocation targets this builder -> empty is fatal
#   NROS_FIXTURE_ID (env) = a sweep-wide narrowing crossing builders -> empty
#                           is normal, EXCEPT when the id exists nowhere
#
# Most cases exercise the shared guard directly (no CLI, no SDK, no build), and
# one runs a real builder end to end so the gate also proves it is wired in.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# shellcheck source=scripts/build/fixture-id-guard.sh
source scripts/build/fixture-id-guard.sh

fails=0

# Ids picked from the manifest at run time — hardcoding one would rot the day
# it is renamed, and this gate would then pass by testing nothing.
ws_id="$(python3 scripts/build/fixtures-manifest.py list-workspaces --platform linux --lang rust \
    | head -1 | cut -d$'\x1f' -f1)"
fx_id="$(python3 - <<'PY'
import re
s = open("examples/fixtures.toml").read()
parts = re.split(r'^\[\[(\w+)\]\]\s*$', s, flags=re.M)
for i in range(1, len(parts), 2):
    if parts[i] == "fixture":
        m = re.search(r'^id\s*=\s*"([^"]+)"', parts[i + 1], re.M)
        if m:
            print(m.group(1))
            break
PY
)"

if [ -z "$ws_id" ] || [ -z "$fx_id" ]; then
    echo "check-fixture-id-guard: could not sample ids from examples/fixtures.toml" >&2
    exit 1
fi

# expect_rc <want-rc> <label> <command...>
#
# The guard EXITS rather than returning, so each case runs in a subshell.
expect_rc() {
    local want="$1" label="$2"
    shift 2
    local out rc
    out="$("$@" 2>&1)"
    rc=$?
    if [ "$rc" -ne "$want" ]; then
        echo "  FAIL  ${label}: expected rc=${want}, got rc=${rc}"
        echo "        output: ${out}"
        fails=$((fails + 1))
        return
    fi
    # A pass that says nothing is the bug itself — every case must speak.
    if [ -z "$out" ]; then
        echo "  FAIL  ${label}: rc=${rc} as expected, but printed NOTHING"
        fails=$((fails + 1))
        return
    fi
    echo "  ok    ${label} (rc=${rc})"
}

guard() { # <id> <source> <kind> <platform> <lang>
    ( nros_fixture_id_no_match "$1" "$2" "$3" "$4" "$5" )
}

echo "check-fixture-id-guard: an id filter that matches nothing"

# The case that started it: a real id, aimed at the builder that cannot build
# it. Fatal, because nothing else in this invocation will build it either.
expect_rc 2 "flag + id of another kind is fatal" \
    guard "$ws_id" flag fixture linux rust

# A typo can never match anywhere, in any stage, under any spelling.
expect_rc 2 "flag + id that exists nowhere is fatal" \
    guard "no-such-fixture-id-anywhere" flag fixture linux rust
expect_rc 2 "env + id that exists nowhere is fatal" \
    guard "no-such-fixture-id-anywhere" env workspace_fixture linux rust

# The sweep cases: a narrowing that crosses builders leaves some stages with
# nothing to do. That is correct, and must not fail the run.
expect_rc 0 "env + id of another kind passes with a note" \
    guard "$fx_id" env workspace_fixture linux rust
expect_rc 0 "env + right kind, wrong coordinates passes with a note" \
    guard "$ws_id" env workspace_fixture linux cpp

# Right kind, wrong coordinates, aimed deliberately: still a wrong invocation.
expect_rc 2 "flag + right kind, wrong coordinates is fatal" \
    guard "$ws_id" flag workspace_fixture linux cpp

echo "check-fixture-id-guard: platform vocabulary"
expect_rc 2 "unknown platform is fatal" \
    bash -c 'source scripts/build/fixture-id-guard.sh; nros_fixture_require_known_platform natve'

echo "check-fixture-id-guard: manifest parser failure (issue 1264)"

# A parser that cannot even start (no tomllib/tomli — the real trigger, on a
# python3.10 host with neither installed — or any other import-time failure)
# must be reported as a PARSER failure, never folded into "no such id" / "no
# such platform". Those are confident claims a dead parser has no evidence
# for, and reading its empty output as one is exactly issue 1264.
#
# Faked with a `python3` shadowing PATH rather than by uninstalling tomllib:
# the real failure is host-Python-version-dependent (this repo's own dev
# hosts vary), and this gate must reproduce identically on any of them.
fake_python_dir="$(mktemp -d)"
cat >"$fake_python_dir/python3" <<'FAKE_PY'
#!/usr/bin/env bash
echo "Traceback (most recent call last):" >&2
echo "ModuleNotFoundError: No module named 'tomllib'" >&2
exit 1
FAKE_PY
chmod +x "$fake_python_dir/python3"

# expect_dead_parser <label> <command...>
#
# Distinct from expect_rc: a dead parser's rc is the PARSER's exit status
# (not a fixed fatal code), so this checks shape (non-zero, parser's own
# traceback reached the caller, no "does not exist" claim) rather than one rc.
expect_dead_parser() {
    local label="$1"
    shift
    local out rc
    out="$(PATH="$fake_python_dir:$PATH" "$@" 2>&1)"
    rc=$?
    if [ "$rc" -eq 0 ]; then
        echo "  FAIL  ${label}: a dead parser returned rc=0"
        fails=$((fails + 1))
        return
    fi
    case "$out" in
        *ModuleNotFoundError*) ;;
        *)
            echo "  FAIL  ${label}: the parser's own traceback did not reach the caller"
            echo "        output: ${out}"
            fails=$((fails + 1))
            return
            ;;
    esac
    case "$out" in
        *"no row anywhere carries id"* | *"unknown platform"* | *"no fixture rows for platform"*)
            echo "  FAIL  ${label}: a dead parser was reported as a real 'does not exist'"
            echo "        output: ${out}"
            fails=$((fails + 1))
            return
            ;;
    esac
    echo "  ok    ${label} (rc=${rc})"
}

expect_dead_parser "require_known_platform on a dead parser names the parser, not a typo" \
    bash -c 'source scripts/build/fixture-id-guard.sh; nros_fixture_require_known_platform threadx-linux'
expect_dead_parser "id_no_match on a dead parser names the parser, not a typo" \
    bash -c 'source scripts/build/fixture-id-guard.sh; nros_fixture_id_no_match some-id flag fixture linux rust'

rm -rf "$fake_python_dir"

echo "check-fixture-id-guard: wired into the builders"
# End to end through a real builder — proves the guard is actually reached,
# not merely present. fixtures-build.sh needs no CLI or SDK to get this far.
expect_rc 2 "fixtures-build.sh --id of a workspace row is fatal" \
    bash scripts/build/fixtures-build.sh linux rust --id "$ws_id"

# And the case that must stay silent and green: no id filter, a real platform,
# a language with no rows. Sweeps hit this constantly.
empty_out="$(bash scripts/build/fixtures-build.sh threadx-linux mixed 2>&1)"
empty_rc=$?
if [ "$empty_rc" -ne 0 ]; then
    echo "  FAIL  unfiltered empty coordinate: expected rc=0, got rc=${empty_rc}"
    fails=$((fails + 1))
elif [ -n "$empty_out" ]; then
    echo "  FAIL  unfiltered empty coordinate: expected silence, got: ${empty_out}"
    fails=$((fails + 1))
else
    echo "  ok    unfiltered empty coordinate stays silent and green"
fi

# issue 1616 (W7) — WHICH builders must call the guard is harvested, not the
# one `fixtures-build.sh` row above: every shell script that takes an id filter
# (a `--id)` arm, or `=${NROS_FIXTURE_ID...}`) must call
# `nros_fixture_id_no_match` in CODE. `workspace-fixtures-build.sh` could drop
# its call and exit 0 on a no-match with this gate green.
echo "check-fixture-id-guard: every id-taking builder calls the guard"
if ! python3 - <<'PY'
import re, sys
sys.path.insert(0, "scripts/lib")
import comments, file_kinds, harvest

TAKES = re.compile(r"(^|\s)--id\)|=\s*\"?\$\{NROS_FIXTURE_ID\b", re.M)
CALL = re.compile(r"(^|[;&|({]\s*|\s)nros_fixture_id_no_match(\s|$)", re.M)

def takes_and_calls(text):
    code = comments.strip_comments(text, "sh")
    return bool(TAKES.search(code)), bool(CALL.search(code))

# Normal-path selftest, both directions.
assert takes_and_calls('case "$1" in\n  --id) x=$2;;\nesac\nnros_fixture_id_no_match "$x" flag k p l\n') == (True, True)
assert takes_and_calls('f="${NROS_FIXTURE_ID:-}"\n# nros_fixture_id_no_match "$f"\nexit 0\n') == (True, False)

EXEMPT = {"scripts/build/fixture-id-guard.sh": "the guard's own definition file"}
takers, bad = [], []
for rel in file_kinds.files_of_kind("shell"):
    if not rel.startswith("scripts/") or rel == "scripts/check-fixture-id-guard.sh":
        continue
    takes, calls = takes_and_calls(open(rel, errors="replace").read())
    if takes or rel in EXEMPT:
        takers.append(rel)
        if takes and not calls and rel not in EXEMPT:
            bad.append(rel)
checked, problems = harvest.reconcile(takers, EXEMPT, what="id-taking builder")
for b in bad:
    print(f"  FAIL  {b} takes an id filter and never calls nros_fixture_id_no_match")
for p in problems:
    print(f"  FAIL  {p}")
if bad or problems:
    sys.exit(1)
print(f"  ok    {len(checked)} id-taking builder(s) call the guard: {', '.join(checked)}")
PY
then
    fails=$((fails + 1))
fi

if [ "$fails" -ne 0 ]; then
    echo "check-fixture-id-guard: ${fails} case(s) failed" >&2
    exit 1
fi
echo "check-fixture-id-guard: OK"
