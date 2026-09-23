#!/usr/bin/env bash
# phase-463 W4 (issue 1419) -- the census RUNS, and it goes stale BY CONTENT.
#
# The four moves the phase doc names, in order, over one fixture workspace:
#
#   1. STALE SOURCE  a component gains a subscription; the configure's
#                    `--require-fresh` check refuses, naming the tree that
#                    changed and the digest it expected.
#   2. RE-RUN        take a census of the code as it is now. The freshness
#                    refusal is gone and the COMPARISON refuses instead, with
#                    `missing-in-contract`: the code creates an endpoint the
#                    contract does not declare. That is the safety island's
#                    E3a, one build before `ExecutorFull` on a board with no
#                    wired console.
#   3. ADD ROW       declare it. The check passes.
#   4. TOUCH ONLY    rewrite a source file with identical bytes. NOTHING is
#                    stale.
#
# Move 4 is the point of the whole wave and the reason staleness is
# content-addressed rather than mtime-keyed. A `git pull`, a rebase or a
# `git stash pop` rewrites tracked files without changing them; an mtime-keyed
# census would read STALE for the whole tree afterwards, and a gate that cries
# stale on every rebase is a gate people turn off. The asymmetry that has to
# hold is: touching a file is not a change, ADDING AN ENTITY is.
#
# WHY A FIXTURE AND NOT AN IMAGE. The census producer is the entry's own native
# binary (phase-463 W2), and building one needs a workspace, a toolchain and
# minutes. What this gate is about is not the producer -- W2's own tests cover
# the ABI -- it is the FRESHNESS RULE and the POLICY, both of which are
# properties of the recorded provenance and the sources beside it. So the
# fixture's "entry binary" is a script that reports what its component source
# declares, which is exactly the relation a real entry has to its code, and the
# four moves are then the real verb, the real provenance walk and the real
# check.
#
# Usage: check-entity-census.sh
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repo_root" || exit 2

fail() { echo "check-entity-census: FAIL -- $*" >&2; exit 1; }

# ---------------------------------------------------------------------------
# The CLI
# ---------------------------------------------------------------------------
# Resolution order, then a VERIFICATION. An `nros` on PATH may predate this
# wave, and a gate that runs a museum binary reports on a tree nobody has.
# `--require-fresh` is the flag this wave adds, so its presence IS the test.
nros_bin() {
    local c
    for c in \
        "${NROS_CLI:-}" \
        "${CARGO_TARGET_DIR:-}/release/nros" \
        "${CARGO_TARGET_DIR:-}/debug/nros" \
        "$repo_root/packages/cli/target/release/nros" \
        "$repo_root/packages/cli/target/debug/nros" \
        "$(command -v nros 2>/dev/null || true)"
    do
        [ -n "$c" ] && [ -x "$c" ] || continue
        if "$c" ws entity-census check --help 2>&1 | grep -q -- '--require-fresh'; then
            printf '%s' "$c"
            return 0
        fi
    done
    return 1
}

# No CLI, no gate -- RECORDED, never silent. This lane is documented to run
# green on a pristine detached worktree with no CLI and no sources, and a gate
# that built one to satisfy itself would put a multi-minute compile on the
# per-push line. Same contract `check-cli-tests` and `check-cli-fresh` keep,
# through the same ledger, so `just check` says what did not run instead of
# printing a success line that covers it.
if ! nros="$(nros_bin)"; then
    # shellcheck source=scripts/build/check-skip.sh
    source "$repo_root/scripts/build/check-skip.sh"
    nros_check_skip entity-census \
        "no in-tree nros CLI carrying \`--require-fresh\` (just setup-cli)"
    exit 0
fi
echo "check-entity-census: nros = $nros"

# ---------------------------------------------------------------------------
# The fixture workspace
# ---------------------------------------------------------------------------
ws="$(mktemp -d "${TMPDIR:-/tmp}/nros-entity-census.XXXXXX")" || exit 2
trap 'rm -rf "$ws"' EXIT

node_pkg="$ws/src/census_fixture_node"
bringup="$ws/src/census_fixture_bringup"
mkdir -p "$node_pkg/src" "$bringup/launch" "$bringup/config" "$ws/build/nros/census"

cat > "$node_pkg/package.xml" <<'XML'
<?xml version="1.0"?>
<package format="3">
  <name>census_fixture_node</name>
  <version>0.0.0</version>
  <description>phase-463 W4 fixture: one component whose source states what it creates.</description>
  <maintainer email="dev@example.invalid">dev</maintainer>
  <license>Apache-2.0</license>
</package>
XML

# THE CODE. One `SUB:` line per subscription the component creates. The fixture
# entry below reads exactly this, which is the relation a real entry has to its
# components: the entry creates what the code says, and the census records what
# the entry created.
cat > "$node_pkg/src/node.cpp" <<'CPP'
// SUB: /sensing/velocity autoware_vehicle_msgs/msg/VelocityReport
CPP

cat > "$bringup/package.xml" <<'XML'
<?xml version="1.0"?>
<package format="3">
  <name>census_fixture_bringup</name>
  <version>0.0.0</version>
  <description>phase-463 W4 fixture bringup.</description>
  <maintainer email="dev@example.invalid">dev</maintainer>
  <license>Apache-2.0</license>
</package>
XML

# `refuse` on both keys, because this gate is asserting that the refusal
# HAPPENS. The shipped default is `warn`, which the last move below checks
# separately so that both arms of the policy are exercised.
cat > "$bringup/system.toml" <<'TOML'
[system]
name = "census_fixture"
rmw = "zenoh"
domain_id = 0

[census]
on_missing = "refuse"
on_stale = "refuse"
TOML

# `<bringup>/config/<model>.yaml`, which is one of the two layouts
# `model_gate::infer_bringup_dir` recognises; the contract path the model
# records resolves against the bringup root from there.
model="$bringup/config/system_model.yaml"
contract="$bringup/launch/fixture.contract.yaml"

cat > "$contract" <<'YAML'
version: 1

nodes:
  fixture_node:
    sub:
      velocity: { min_rate_hz: 10, qos: { depth: 1 } }

topics:
  /sensing/velocity:
    type: autoware_vehicle_msgs/msg/VelocityReport
    external: pub
    sub: [fixture_node/velocity]
    rate_hz: 10
YAML

# The resolved model, written the way `nros sync` writes one: it records the
# contract sidecar it folded in, with that file's hash. Two things ride on
# that. The phase-460 W1 model gate re-hashes it at every door, so the model
# has to be RE-STAMPED whenever the contract changes -- which is what a sync
# does and what `stamp_model` below stands for. And the check discovers the
# contract through it, which is how a cross configure names a contract without
# re-deriving `<stem>.contract.yaml` in cmake.
#
# No resolver pin: this model came from no resolve, and an invented pin would
# be a false statement in the one field phase-460 W1 exists to believe.
stamp_model() {
    local sha
    sha="$(sha256sum "$contract" | cut -d" " -f1)"
    cat > "$model" <<YAML
meta:
  version: 1
  inputs:
    - path: launch/fixture.contract.yaml
      sha256: $sha
structure:
  nodes:
    /fixture_node:
      scope: census_fixture_bringup/fixture.launch.xml
      pkg: census_fixture_node
      exec: fixture_node
  topics:
    /sensing/velocity:
      type: autoware_vehicle_msgs/msg/VelocityReport
      sub: [/fixture_node]
YAML
}

# THE ENTRY. Reports what the component source declares, into $NROS_CENSUS_OUT,
# in the recorder's schema-2 shape. This is the fixture's only invention.
cat > "$ws/fixture_entry" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
src="$(dirname "$0")/src/census_fixture_node/src/node.cpp"
subs=""
while read -r _ _ topic type; do
    [ -n "${topic:-}" ] || continue
    pkg="${type%%/*}"
    leaf="${type#*/}"
    [ -z "$subs" ] || subs="$subs,"
    subs="$subs{\"id\":\"$topic\",\"unresolved_topic\":{\"value\":\"$topic\",\"kind\":\"absolute\"},\"interface\":{\"package\":\"$pkg\",\"name\":\"$leaf\",\"kind\":\"message\"},\"qos\":{\"reliability\":\"reliable\",\"durability\":\"volatile\",\"history\":\"keep_last\",\"depth\":1,\"liveliness\":\"system_default\"}}"
done < <(grep '^// SUB:' "$src")
cat > "$NROS_CENSUS_OUT" <<JSON
{
  "version": 2,
  "nodes": [
    {
      "id": "fixture_node",
      "unresolved_name": { "value": "fixture_node", "kind": "relative" },
      "namespace": null,
      "publishers": [],
      "subscribers": [$subs],
      "services": [],
      "service_clients": [],
      "actions": [],
      "action_clients": [],
      "timers": []
    }
  ],
  "parameters": []
}
JSON
SH
chmod +x "$ws/fixture_entry"

census="$ws/build/nros/census/fixture_entry.json"
stamp_model

take_census() {
    ( cd "$ws" && "$nros" ws entity-census run \
        --entry fixture_entry \
        --workspace "$ws" \
        --binary "$ws/fixture_entry" \
        --model "$model" ) > "$ws/run.log" 2>&1
}

check() {
    ( cd "$ws" && "$nros" ws entity-census check \
        --census "$census" \
        --model "$model" \
        --system-toml "$bringup/system.toml" \
        --workspace "$ws" \
        --entry fixture_entry \
        --strict \
        "$@" ) > "$ws/check.log" 2>&1
}

want() { grep -q -- "$1" "$ws/check.log" || fail "$2 -- expected \"$1\" in:
$(cat "$ws/check.log")"; }

# ---------------------------------------------------------------------------
# Move 0 -- no census at all. `[census] on_missing = "refuse"`.
# ---------------------------------------------------------------------------
if check --require-fresh; then
    fail "move 0: a configure with NO census and on_missing=refuse must refuse"
fi
want "census missing" "move 0"
want "entity-census run --entry fixture_entry" "move 0 names the remedy"
echo "check-entity-census: move 0 ok -- no census refuses, and names the producer"

# ---------------------------------------------------------------------------
# Move 1 -- a census of the code as it is, then a SOURCE CHANGE.
# ---------------------------------------------------------------------------
take_census || fail "move 1: the census run failed:
$(cat "$ws/run.log")"
check --require-fresh || fail "move 1: a census just taken must be fresh:
$(cat "$ws/check.log")"

# The component gains a subscription. Nothing else moves: not the contract, not
# the model, not the entry.
cat >> "$node_pkg/src/node.cpp" <<'CPP'
// SUB: /system/operation_mode tier4_system_msgs/msg/OperationModeAvailability
CPP

if check --require-fresh; then
    fail "move 1: a census taken before the source changed must not pass --require-fresh"
fi
want "census stale" "move 1"
want "src/census_fixture_node" "move 1 names the tree that changed"
want "fnv1a64:" "move 1 names the digest it expected"
# The CONTRACT is not consulted while the census cannot be believed: a
# comparison against a museum census says something true about two documents
# and nothing about the code.
grep -q "missing-in-contract" "$ws/check.log" \
    && fail "move 1: a stale census must not be compared"
echo "check-entity-census: move 1 ok -- an edited source makes the census stale"

# ---------------------------------------------------------------------------
# Move 2 -- re-run the census. Fresh again, and now the COMPARISON refuses.
# ---------------------------------------------------------------------------
take_census || fail "move 2: the census re-run failed:
$(cat "$ws/run.log")"
if check --require-fresh; then
    fail "move 2: the code creates an endpoint the contract does not declare"
fi
grep -q "census stale" "$ws/check.log" \
    && fail "move 2: a census just taken is not stale:
$(cat "$ws/check.log")"
want "missing-in-contract" "move 2"
want "/system/operation_mode" "move 2 names the endpoint"
want "not waivable" "move 2: the UNDER direction has no waiver"
echo "check-entity-census: move 2 ok -- fresh census, missing-in-contract refusal"

# ---------------------------------------------------------------------------
# Move 3 -- declare the row. The configure passes.
# ---------------------------------------------------------------------------
cat > "$contract" <<'YAML'
version: 1

nodes:
  fixture_node:
    sub:
      velocity: { min_rate_hz: 10, qos: { depth: 1 } }
      operation_mode: { min_rate_hz: 10, qos: { depth: 1 } }

topics:
  /sensing/velocity:
    type: autoware_vehicle_msgs/msg/VelocityReport
    external: pub
    sub: [fixture_node/velocity]
    rate_hz: 10
  /system/operation_mode:
    type: tier4_system_msgs/msg/OperationModeAvailability
    external: pub
    sub: [fixture_node/operation_mode]
    rate_hz: 10
YAML

# A contract edit is a MODEL input change: `nros sync` re-resolves and
# re-stamps, and the phase-460 W1 gate refuses the model until it does. The
# census is NOT re-run -- the code did not change, and the phase doc's
# acceptance says this move configures.
stamp_model

check --require-fresh || fail "move 3: the contract now states the code:
$(cat "$ws/check.log")"
want "2 confirmed, 0 error" "move 3"
echo "check-entity-census: move 3 ok -- the declared row configures"

# ---------------------------------------------------------------------------
# Move 4 -- TOUCH ONLY. The point of the wave.
# ---------------------------------------------------------------------------
# Identical bytes, new mtime: what a rebase does to a whole tree. `touch(1)`
# alone would be the weaker test on a filesystem with coarse timestamps, so the
# file is REWRITTEN with the same content and then touched into the future.
body="$(cat "$node_pkg/src/node.cpp")"
printf '%s\n' "$body" > "$node_pkg/src/node.cpp"
touch -d '+1 hour' "$node_pkg/src/node.cpp" 2>/dev/null || touch "$node_pkg/src/node.cpp"
touch "$node_pkg/package.xml" "$bringup/launch/fixture.contract.yaml"

check --require-fresh || fail "move 4: TOUCHING a source is not a change, and a
census that goes stale on a rebase is a gate people turn off:
$(cat "$ws/check.log")"
want "2 confirmed, 0 error" "move 4"
echo "check-entity-census: move 4 ok -- a touch leaves the census fresh"

# ---------------------------------------------------------------------------
# Move 5 -- the OTHER policy arm. `warn` says the same thing and continues.
# ---------------------------------------------------------------------------
# Both arms matter: `warn` is what this phase LANDS with, so a `warn` that went
# silent would ship the defect issue 1419 is about with a gate's name on it.
cat > "$bringup/system.toml" <<'TOML'
[system]
name = "census_fixture"
rmw = "zenoh"
domain_id = 0
TOML
rm -f "$census"
check --require-fresh || fail "move 5: the landing default is warn, not refuse:
$(cat "$ws/check.log")"
want "WARNING" "move 5 is not silent"
want "on_missing" "move 5 names the key that decided"
echo "check-entity-census: move 5 ok -- the landing default warns and continues"

# ---------------------------------------------------------------------------
# The NEGATIVE CONTROL, on the normal path (phase-395)
# ---------------------------------------------------------------------------
# A gate that cannot go red is a comment. Two controls, both about THIS gate:
# its assertion helper must be able to say no, and the refusals above must be
# coming from `--require-fresh` rather than from something else that happens to
# fail at the same time.
self_test() {
    # The refusing policy again -- move 5 left the landing default in place.
    cat > "$bringup/system.toml" <<'TOML'
[system]
name = "census_fixture"
rmw = "zenoh"
domain_id = 0

[census]
on_missing = "refuse"
on_stale = "refuse"
TOML

    printf 'census check: 2 confirmed, 0 error(s)\n' > "$ws/check.log"
    want "2 confirmed" "self-test: a line that IS there"
    if ( want "a line no check has ever printed" "self-test" ) 2>/dev/null; then
        fail "self-test: \`want\` accepted an absent string, so every assertion above
asserts nothing and this gate is a comment"
    fi

    # A census that is BOTH stale and in disagreement with the contract, so
    # the two refusals below can be told apart. One undeclared subscription is
    # recorded IN the census; a second is added to the source afterwards, which
    # is what makes the census stale.
    printf '// SUB: /selftest/recorded std_msgs/msg/Empty\n' >> "$node_pkg/src/node.cpp"
    take_census || fail "self-test: the census run failed:
$(cat "$ws/run.log")"
    printf '// SUB: /selftest/unrecorded std_msgs/msg/Empty\n' >> "$node_pkg/src/node.cpp"

    if check --require-fresh; then
        fail "self-test: an edited source must be stale"
    fi
    want "census stale" "self-test: with the flag, staleness refuses"

    # Same census, same edited source, WITHOUT the flag: the comparison runs
    # and refuses on its own terms. If this said `census stale` too, the flag
    # would not be what gates freshness and move 1 would be measuring
    # something else.
    if check; then
        fail "self-test: the code still creates an endpoint the contract does not declare"
    fi
    if grep -q "census stale" "$ws/check.log"; then
        fail "self-test: freshness refused without --require-fresh, so the flag gates nothing"
    fi
    want "missing-in-contract" "self-test: the comparison still runs"
    echo "check-entity-census: self-test ok -- the assertions can fail, and the flag is what gates freshness"
}

self_test

echo "check-entity-census: PASS -- 6 moves (missing, fresh, stale, re-run, add-row, touch-only) + self-test"
