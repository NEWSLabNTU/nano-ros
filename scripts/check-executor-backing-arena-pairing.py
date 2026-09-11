#!/usr/bin/env python3
"""issue 1171 — the picolibc arena and the executor backing are ONE decision.

WHY THIS EXISTS. phase-392 W6 moved the executor's per-entry storage into the
named `.bss` static `nros_node::executor::backing::EXECUTOR_BACKING`. On Zephyr
the Rust global allocator is picolibc malloc, whose arena is itself a fixed
static sized by `CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE`, and before W6 the
backing was a `Box::leak` out of it. So an image that turns the static on
without lowering that arena reserves the same bytes TWICE.

Issue 1145 paired them on one leaf by copying the size out of `nm` output:

    CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE=961320   # 1048576 - 87256

That subtrahend is `ExecutorSizing::DEFAULT.u64_len() * 8`, derived from the
executor knobs. Move `MAX_CBS`, the arena or the rx buffer and it is silently
wrong, in whichever direction, and nothing said so. It was also wrong on the
OTHER board the same conf builds for: the derived size is 87,256 B on
mps2_an385 and 88,328 B on native_sim/native/64, so the second board kept
double-reserving 1,072 bytes.

THE MECHANISM this gate backs. `CONFIG_NROS_EXECUTOR_BACKING_U64S` (zephyr/
Kconfig) lets the image STATE the reservation instead of measuring it. The
reservation is then exactly `8 * words` bytes on every target, `nros-node`
refuses to compile if that is below what the executor needs, and the arena's
lowering is arithmetic anyone can check — which is what this does.

THE RULE. A `.conf` that states `CONFIG_NROS_EXECUTOR_BACKING_U64S=<words>`
with `words > 0` must also

  * set `CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE`, and
  * record what it lowered that arena FROM, as `# nros-arena-base: <bytes>`,

and the three numbers must satisfy

    arena + 8 * words == base

The marker is required because "how much was this lowered by?" has no other
answer in the file: the pre-W6 number is gone the moment it is edited, and a
reviewer cannot tell a paired arena from an arbitrary one. It is checked both
ways — a marker with no pairing is as broken as a pairing with no marker.

The gate also refuses a `CONFIG_NROS_EXECUTOR_BACKING_U64S` line in a conf when
`zephyr/Kconfig` does not declare that symbol. Kconfig silently ignores an
assignment to an undeclared symbol, so such a line reads as a decision and
reaches nothing — issue 0460's failure mode, one layer up.

THE OTHER HALF (issue 1284). `arena + 8 * words == base` says the three numbers
SUM. It says nothing about whether `words` is enough, and that is the half that
drifted: twelve leaves stayed exactly paired at 11041, then 11045, while the
executor grew to 11065 and then 11069 under them, and this gate stayed green
because the sum held. `executor::backing`'s const assertion catches it, but it
is a COMPILE error and no merge-gating lane builds a Zephyr Rust image.

So a stated backing is a CLAIM, and the claim is checked against the MEASURED
default in `just check node-std-tests` (pull_request AND merge_group):

  * host-width boards — `nros-node/tests/executor_backing_claims.rs` compares
    `words` with `ExecutorSizing::DEFAULT.u64_len()` as compiled on the host,
    naming the conf and both numbers;
  * every other board — the recipe compiles `nros-node` for that board's own
    target with `NROS_EXECUTOR_BACKING_U64S=<words>`, so the crate's const
    assertion rules on it at the board's pointer width.

This file owns the conf side of both: which confs claim, for which boards, and
which of them no measurement can vouch for. `--claims` prints that for the two
consumers above. A claim is REFUSED — a failure with a reason, never a skip —
when

  * no fixture row builds the conf, so nothing names the board it is for;
  * a row names a board this file has no pointer width / measurement for;
  * any fragment the image merges sets an EXECUTOR-SIZING knob, so the default
    measured without it is not the image's default;
  * `nros-node/build.rs` reads a knob this file has not classified (the knob
    table cannot go stale toward OK).

Usage:
    python3 scripts/check-executor-backing-arena-pairing.py [--self-test | --claims]
"""

import re
import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # python < 3.11
    import tomli as tomllib

BACKING_KEY = "CONFIG_NROS_EXECUTOR_BACKING_U64S"
ARENA_KEY = "CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE"
BASE_MARKER = "nros-arena-base"

# A word of the reservation is a `MaybeUninit<u64>`, so it is 8 bytes on every
# target nano-ros builds for -- which is the whole reason the knob is spelled in
# words rather than bytes: the DERIVED size is target-dependent and a stated one
# is not.
BYTES_PER_WORD = 8

BACKING_RE = re.compile(rf"^\s*{BACKING_KEY}\s*=\s*(-?\d+)\s*$", re.M)
ARENA_RE = re.compile(rf"^\s*{ARENA_KEY}\s*=\s*(\d+)\s*$", re.M)
BASE_RE = re.compile(rf"#\s*{BASE_MARKER}\s*:\s*(\d+)")

# --- issue 1284: the claim half -------------------------------------------
#
# Every `NROS_*` name `packages/core/nros-node/build.rs` reads, classified. The
# gate harvests the names from build.rs and fails on one that is in neither
# table, and on a table entry build.rs no longer reads — both directions, so the
# table cannot drift toward OK.
#
# SIZING: feeds `ExecutorSizing::DEFAULT` (cbs / sc / nodes / arena). An image
# that sets one has a default the host build did not measure. The Zephyr
# spelling is `CONFIG_<name>`, read from `$DOTCONFIG` (issue 0460).
SIZING_KNOBS = {
    "NROS_EXECUTOR_MAX_CBS": "`cbs`, and every per-slot arena term",
    "NROS_DECLARED_EXECUTOR_MAX_CBS": "the declared rung of `cbs` (issue 1199)",
    "NROS_EXECUTOR_MAX_SC": "`sc`",
    "NROS_EXECUTOR_MAX_NODES": "`nodes`",
    "NROS_DECLARED_EXECUTOR_MAX_NODES": "the declared rung of `nodes` (issue 1233)",
    "NROS_EXECUTOR_ARENA_SIZE": "`arena`, directly",
    "NROS_EXECUTOR_ACTION_CLIENTS": "the arena's action-client term",
    "NROS_DECLARED_EXECUTOR_ACTION_CLIENTS": "the declared rung of the same",
    "NROS_SUBSCRIPTION_BUFFER_SIZE": "the arena's rx-buffer terms",
    "NROS_DECLARED_SUBSCRIPTION_BUFFER_SIZE": "the declared rung of the same (issue 1233)",
    "NROS_SUBSCRIBER_BUFFER_SIZE": "the arena's pub/sub region",
    "NROS_PUBSUB_QOS_DEPTH": "the arena's pub/sub region depth (issue 1190)",
    "NROS_DECLARED_MAX_QOS_DEPTH": "the declared rung of that depth",
    "NROS_ENTITY_COUNT_SUBSCRIPTION": "the per-kind arena sum (phase-403)",
    "NROS_ENTITY_COUNT_TIMER": "the per-kind arena sum",
    "NROS_ENTITY_COUNT_SERVICE_SERVER": "the per-kind arena sum",
    "NROS_ENTITY_COUNT_ACTION_CLIENT": "the per-kind arena sum",
    "NROS_ENTITY_COUNT_ACTION_SERVER": "the per-kind arena sum",
    "NROS_ENTITY_DECLARED_DEPTHS": "the per-kind subscription region sum",
    "NROS_ENTITY_UNDECLARED_DEPTH_COUNT_SUBSCRIPTION": "the same sum's guard",
}
NOT_SIZING = {
    "NROS_EXECUTOR_BACKING_U64S": "the claim itself",
    "NROS_EXECUTOR_BACKING_SECTION": "placement only, not size",
    "NROS_BOOT_REPORT": "a cfg, no size",
    "NROS_PARAM_SERVICE_BUFFER_SIZE": "emitted as a const, not in the arena sum",
    "NROS_DECLARED_PARAM_SERVICE_SHAPE":
        "the declared rung of that same buffer (phase-446 F3): it sizes a "
        "runtime `ParamServiceBuffers`, never a term of the arena sum",
    "NROS_EXECUTOR_MAX_SHUTDOWN_CBS": "sizes the Executor HEADER, not the backing",
}
# Not knobs, but inputs that move a knob's resolution in build.rs: the Kconfig
# reader and the board/platform descriptor rung (`BuildRungs::from_build_env`).
# The host measurement is only the image's when none of these reached it.
BUILD_ENV_RUNGS = (
    "DOTCONFIG",
    "NROS_PLATFORM_NAME",
    "NROS_BOARD_TOML",
    "NROS_BOARD",
    "NROS_PLATFORMS_DIR",
)
# The descriptor rung, per knob name, as `BuildRungs::executor_rungs` spells it.
DESCRIPTOR_KNOB_RE = re.compile(
    r"^\s*(max_cbs|max_sc|max_nodes|arena_size|action_clients|"
    r"subscription_buffer_size)\s*=",
    re.M,
)
ZEPHYR_PLATFORM_DESCRIPTOR = "packages/platform/nros-platform-zephyr/nros-platform.toml"

# A board's pointer width, and the Rust target that measures it when that width
# is not the host's (None: only a host of that width can measure it). A board
# absent from this table is REFUSED, never guessed.
BOARD_TARGETS = {
    "native_sim/native/64": (64, None),
    "mps2_an385": (32, "thumbv7m-none-eabi"),
}

# Fragments EVERY Zephyr image may merge after the leaf's own (the board / line
# tails `scripts/build/zephyr-fixture-leaves.sh` appends). All of them are read,
# not the one a board picks: a sizing knob in any of them is refused, which is
# the conservative direction and needs no second copy of that script's `case`.
SHARED_FRAGMENT_GLOBS = ("cmake/zephyr/*.conf", "zephyr/*.conf")

NAME_LITERAL_RE = re.compile(r'"(NROS_[A-Z0-9_]+)"')


def sizing_assignments(text):
    """-> [(key, value)] for every executor-sizing knob a fragment assigns."""
    out = []
    for knob in SIZING_KNOBS:
        for m in re.finditer(rf"^\s*CONFIG_{knob}\s*=\s*(\S+)", text, re.M):
            out.append((f"CONFIG_{knob}", m.group(1)))
    return out


def classify(build_rs_names):
    """-> complaints: a name build.rs reads that no table classifies, or a table
    entry build.rs no longer reads."""
    bad = []
    known = set(SIZING_KNOBS) | set(NOT_SIZING)
    for name in sorted(build_rs_names - known):
        bad.append(
            f"nros-node/build.rs reads `{name}`, which this gate has not "
            f"classified. Add it to SIZING_KNOBS if it feeds "
            f"ExecutorSizing::DEFAULT, else to NOT_SIZING with a reason."
        )
    for name in sorted(known - build_rs_names):
        bad.append(
            f"`{name}` is classified here but nros-node/build.rs no longer "
            f"reads it — delete the entry"
        )
    return bad


def last_int(pattern, text):
    """The LAST assignment wins, the way Kconfig merges fragments (issue 0876)."""
    found = pattern.findall(text)
    return int(found[-1]) if found else None


def check_conf(text):
    """-> list of complaint strings for one conf file's body."""
    words = last_int(BACKING_RE, text)
    arena = last_int(ARENA_RE, text)
    base = last_int(BASE_RE, text)

    stated = words is not None and words > 0
    if not stated and base is None:
        return []

    bad = []
    if base is not None and not stated:
        bad.append(
            f"carries `# {BASE_MARKER}: {base}` but states no {BACKING_KEY}, so "
            f"nothing says what the arena was lowered BY"
        )
    if stated and base is None:
        bad.append(
            f"states {BACKING_KEY}={words} but no `# {BASE_MARKER}: <bytes>`, so "
            f"the arena's lowering cannot be checked against anything"
        )
    if stated and arena is None:
        bad.append(
            f"states {BACKING_KEY}={words} but does not set {ARENA_KEY}, so the "
            f"reservation is made and nothing is given back"
        )
    # `words is not None` is implied by `stated`, but a type checker cannot see
    # through the alias, so it is spelled out rather than suppressed.
    if stated and words is not None and base is not None and arena is not None:
        want = base - words * BYTES_PER_WORD
        if arena != want:
            bad.append(
                f"{ARENA_KEY}={arena}, but {base} - {BYTES_PER_WORD} * {words} "
                f"= {want}. Set the arena to {want}, or restate the base."
            )
    return bad


SELF_TESTS = [
    # (name, body, expected number of complaints)
    ("paired exactly", f"# {BASE_MARKER}: 1048576\n{BACKING_KEY}=11041\n{ARENA_KEY}=960248\n", 0),
    ("silent about both", f"{ARENA_KEY}=1048576\n", 0),
    # the drift this gate exists for: a knob moved, the backing was restated,
    # the arena was not.
    ("arena not lowered with it",
     f"# {BASE_MARKER}: 1048576\n{BACKING_KEY}=12000\n{ARENA_KEY}=960248\n", 1),
    ("arena lowered too far",
     f"# {BASE_MARKER}: 1048576\n{BACKING_KEY}=11041\n{ARENA_KEY}=900000\n", 1),
    ("stated with no base",
     f"{BACKING_KEY}=11041\n{ARENA_KEY}=960248\n", 1),
    ("base with no statement",
     f"# {BASE_MARKER}: 1048576\n{ARENA_KEY}=960248\n", 1),
    ("stated with no arena at all",
     f"# {BASE_MARKER}: 1048576\n{BACKING_KEY}=11041\n", 1),
    # `0` declines the static, so there is nothing to pair; `-1` derives.
    ("declined", f"{BACKING_KEY}=0\n{ARENA_KEY}=1048576\n", 0),
    ("derived", f"{BACKING_KEY}=-1\n{ARENA_KEY}=1048576\n", 0),
    # last-wins, the way Zephyr merges fragments (issue 0876)
    ("last assignment wins",
     f"# {BASE_MARKER}: 1048576\n{BACKING_KEY}=11041\n{ARENA_KEY}=1\n{ARENA_KEY}=960248\n", 0),
]


SIZING_SELF_TESTS = [
    # (name, fragment body, expected number of sizing assignments)
    ("a sizing knob", "CONFIG_NROS_EXECUTOR_MAX_CBS=8\n", 1),
    ("the arena knob, even as the derive sentinel",
     "CONFIG_NROS_EXECUTOR_ARENA_SIZE=0\n", 1),
    ("the claim itself is not a sizing knob", f"{BACKING_KEY}=11069\n", 0),
    ("a commented-out knob", "# CONFIG_NROS_EXECUTOR_MAX_SC=16\n", 0),
    ("a prefix is not the knob", "CONFIG_NROS_EXECUTOR_MAX_CBS_EXTRA=1\n", 0),
]

CLASSIFY_SELF_TESTS = [
    # (name, names build.rs reads, expected number of complaints)
    ("exactly the tables", set(SIZING_KNOBS) | set(NOT_SIZING), 0),
    ("an unclassified read",
     set(SIZING_KNOBS) | set(NOT_SIZING) | {"NROS_EXECUTOR_NEW_TABLE"}, 1),
    ("a stale entry", (set(SIZING_KNOBS) | set(NOT_SIZING)) - {"NROS_EXECUTOR_MAX_SC"}, 1),
]


def self_test():
    bad = 0
    for name, body, expect in SELF_TESTS:
        got = check_conf(body)
        if len(got) != expect:
            print(f"  {name}: expected {expect} complaint(s), got {len(got)}: {got}")
            bad += 1
    for name, body, expect in SIZING_SELF_TESTS:
        got = sizing_assignments(body)
        if len(got) != expect:
            print(f"  {name}: expected {expect} sizing assignment(s), got {got}")
            bad += 1
    for name, names, expect in CLASSIFY_SELF_TESTS:
        got = classify(names)
        if len(got) != expect:
            print(f"  {name}: expected {expect} complaint(s), got {len(got)}: {got}")
            bad += 1
    total = len(SELF_TESTS) + len(SIZING_SELF_TESTS) + len(CLASSIFY_SELF_TESTS)
    if bad:
        print(f"check-executor-backing-arena-pairing --self-test: {bad} case(s) FAILED")
        return 1
    print(f"check-executor-backing-arena-pairing --self-test: {total} case(s) OK")
    return 0


def self_test_quiet():
    """`self_test` with its report on stderr, for `--claims` whose stdout is data."""
    real, sys.stdout = sys.stdout, sys.stderr
    try:
        return self_test()
    finally:
        sys.stdout = real


def tracked(repo, *globs):
    out = subprocess.run(
        ["git", "ls-files", "-z", "--", *globs],
        capture_output=True, text=True, check=True, cwd=repo,
    ).stdout.split("\0")
    return [f for f in out if f]


def kconfig_defaults(kconfig):
    """{knob: default} for every SIZING knob `zephyr/Kconfig` declares."""
    out = {}
    for knob in SIZING_KNOBS:
        m = re.search(
            rf"^config\s+{knob}\s*$(.*?)(?=^config\s|\Z)", kconfig, re.M | re.S
        )
        if not m:
            continue
        d = re.search(r"^\s*default\s+(-?\d+)\s*$", m.group(1), re.M)
        if d:
            out[knob] = int(d.group(1))
    return out


def claims(repo, stating):
    """-> (claims, refusals) for `stating` = {rel conf: words}.

    A claim is (conf, words, board, width, cross_target); a refusal is
    (conf, reason)."""
    rows = tomllib.loads(
        (repo / "examples" / "fixtures.toml").read_text(errors="replace")
    ).get("fixture", [])
    shared = tracked(repo, *SHARED_FRAGMENT_GLOBS)
    out, refused = [], []

    descriptor = repo / ZEPHYR_PLATFORM_DESCRIPTOR
    if descriptor.is_file():
        for m in DESCRIPTOR_KNOB_RE.finditer(descriptor.read_text(errors="replace")):
            refused.append((
                ZEPHYR_PLATFORM_DESCRIPTOR,
                f"sets executor knob `{m.group(1)}` for every Zephyr image, so "
                f"no Zephyr image's default is the one the host measured",
            ))

    for rel, words in sorted(stating.items()):
        leaf, name = rel.rsplit("/", 1)
        building = [
            r for r in rows
            if r.get("dir") == leaf and name in (r.get("conf_files") or [])
        ]
        if not building:
            refused.append((rel, (
                "no examples/fixtures.toml row builds this conf, so nothing names "
                "the board -- and so the pointer width -- the stated backing is for"
            )))
            continue
        for row in building:
            board = row.get("board", "")
            if board not in BOARD_TARGETS:
                refused.append((rel, (
                    f"built for board `{board}`, which has no entry in "
                    f"BOARD_TARGETS: no pointer width and no measurement is "
                    f"known for it. Add one; do not guess."
                )))
                continue
            width, cross = BOARD_TARGETS[board]
            fragments = [f"{leaf}/{c}" for c in row.get("conf_files") or []]
            fragments += tracked(repo, f"{leaf}/boards/*.conf") + shared
            for frag in dict.fromkeys(fragments):
                path = repo / frag
                if not path.is_file():
                    continue
                for key, value in sizing_assignments(path.read_text(errors="replace")):
                    refused.append((rel, (
                        f"its `{board}` image merges {frag}, which sets "
                        f"{key}={value}: the executor default measured without "
                        f"that knob is not this image's, so the stated backing "
                        f"cannot be checked against it"
                    )))
            out.append((rel, words, board, width, cross))
    return out, refused


def claim_half(repo, confs):
    """-> (claims, refusals, knob complaints) over the tracked `confs`."""
    stating = {}
    for rel in confs:
        path = repo / rel
        if not path.is_file():
            continue
        words = last_int(BACKING_RE, path.read_text(errors="replace"))
        if words is not None and words > 0:
            stating[rel] = words
    build_rs = (repo / "packages/core/nros-node/build.rs").read_text(errors="replace")
    complaints = classify(set(NAME_LITERAL_RE.findall(build_rs)))
    got, refused = claims(repo, stating)
    return got, refused, complaints


def print_claims(repo, confs):
    """`--claims`: the TSV the two claim consumers read (see the docstring)."""
    got, refused, complaints = claim_half(repo, confs)
    kconfig = (repo / "zephyr" / "Kconfig").read_text(errors="replace")
    print(f"scanned\t{len(confs)}")
    for knob in SIZING_KNOBS:
        print(f"knob\t{knob}")
    for rung in BUILD_ENV_RUNGS:
        print(f"rung\t{rung}")
    for knob, default in sorted(kconfig_defaults(kconfig).items()):
        print(f"kconfig\t{knob}\t{default}")
    for conf, words, board, width, cross in got:
        print(f"claim\t{conf}\t{words}\t{board}\t{width}\t{cross or '-'}")
    for conf, reason in refused:
        print(f"refuse\t{conf}\t{reason}")
    for complaint in complaints:
        print(f"refuse\tpackages/core/nros-node/build.rs\t{complaint}")
    return 1 if (refused or complaints) else 0


def main():
    repo = Path(
        subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
    )
    if "--self-test" in sys.argv:
        return self_test()

    # Tracked files only -- a filesystem walk reaches build output and other
    # checkouts' worktrees (issues 1157/1166).
    confs = tracked(repo, "*.conf")

    if "--claims" in sys.argv:
        # Machine output for `just check node-std-tests` and the nros-node
        # claims test. The self-test still runs, on stderr, so a consumer never
        # reads claims from a parser that no longer parses.
        if self_test_quiet():
            return 1
        return print_claims(repo, confs)

    # On the NORMAL path too, never behind a flag: a negative control nobody
    # runs decays into a comment (AGENTS.md "a gate must run its own selftest").
    if self_test():
        return 1

    kconfig = (repo / "zephyr" / "Kconfig").read_text(errors="replace")
    declared = re.search(
        r"^config\s+NROS_EXECUTOR_BACKING_U64S\s*$", kconfig, re.M
    ) is not None

    failures, paired = [], 0
    for rel in confs:
        path = repo / rel
        if not path.is_file():
            continue
        text = path.read_text(errors="replace")
        if BACKING_KEY not in text and BASE_MARKER not in text:
            continue
        if BACKING_KEY in text and not declared:
            failures.append(
                f"  {rel}: sets {BACKING_KEY}, which zephyr/Kconfig does not "
                f"declare — Kconfig ignores an assignment to an unknown symbol, "
                f"so the line reads as a decision and reaches nothing"
            )
            continue
        bad = check_conf(text)
        if not bad:
            paired += 1
        for complaint in bad:
            failures.append(f"  {rel}: {complaint}")

    # issue 1284 — the claim half's STATIC refusals belong on the fast line too:
    # they are text facts, and a conf nothing can vouch for should be refused
    # on the pull request, not only where the measurement runs.
    got, refused, complaints = claim_half(repo, confs)
    refusals = [f"  {conf}: {reason}" for conf, reason in refused]
    refusals += [f"  packages/core/nros-node/build.rs: {c}" for c in complaints]

    if failures:
        print("check-executor-backing-arena-pairing: FAIL\n")
        print("\n".join(failures))
        print(
            "\nThe executor backing is a `.bss` static since phase-392 W6, and on\n"
            "Zephyr the picolibc arena it used to be leaked out of is itself a\n"
            "fixed static. An image that states the reservation must give the\n"
            "same bytes back:\n"
            "\n"
            f"    # {BASE_MARKER}: <the arena before it was lowered>\n"
            f"    {BACKING_KEY}=<words>\n"
            f"    {ARENA_KEY}=<base - 8 * words>\n"
            "\n"
            "See issues 1145 and 1171. That the three numbers SUM says nothing\n"
            "about whether <words> MEETS the executor's default -- that half is\n"
            "`just check node-std-tests` (issue 1284)."
        )
        return 1

    if refusals:
        print("check-executor-backing-arena-pairing: FAIL (unverifiable claim)\n")
        print("\n".join(refusals))
        print(
            f"\nA conf stating {BACKING_KEY} claims the executor needs at most that\n"
            "many words. `just check node-std-tests` checks the claim against the\n"
            "MEASURED default, and can only do that for an image whose default is\n"
            "the one it measures. The claims above it cannot vouch for, so they\n"
            "fail here rather than pass unchecked (issue 1284)."
        )
        return 1

    print(
        "check-executor-backing-arena-pairing: OK "
        f"({paired} conf(s) pair the arena with a stated backing; "
        f"{len(got)} (conf, board) claim(s) attributed for node-std-tests; "
        f"{len(confs)} conf(s) scanned)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
