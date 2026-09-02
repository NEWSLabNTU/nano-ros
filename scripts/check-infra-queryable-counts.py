#!/usr/bin/env python3
"""Issue 0827 — the infrastructure-queryable counts have ONE definition each,
and it matches the number of servers actually created.

A service server IS a zenoh queryable, so these counts are a term in every
service-buffer pool the RMW sizes. They had SEVEN spellings and no definition:
the count of creation statements, two doc comments, `nros-zpico-build`'s
default-picking comment, and two RMW runtime messages, plus CLAUDE.md. Two had
drifted — both said lifecycle was 6, which is where the widely-quoted "twelve
slots before the application declares anything" came from. It is eleven.

A constant alone would not have caught that: it is still a hand-typed literal,
and a seventh parameter service would leave it stale exactly as the prose was.
So this ties the constant to the CREATION SITES, which is the thing that
actually decides the number.

Python rather than shell, deliberately: `check-gate-selftests`'s call detector
requires parentheses, which a bash function call never has, so a shell gate can
only ever classify as flag-only and land in a baseline that may only shrink.
"""

import os
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

SPIN = "packages/core/nros-node/src/executor/spin.rs"
PARAMS = "packages/core/nros-node/src/parameter_services.rs"
LIFECYCLE = "packages/core/nros-node/src/lifecycle_services.rs"
ACTION = "packages/core/nros-node/src/executor/action.rs"

# (label, creation fn, constant, file that must define it)
GROUPS = [
    ("ROS parameter services", "create_param_srv", "PARAM_SERVICE_QUERYABLES", PARAMS),
    ("REP-2002 lifecycle services", "create_lc_srv", "LIFECYCLE_SERVICE_QUERYABLES", LIFECYCLE),
]

# An RMW backend must NOT restate these: it does not depend on `nros-node` and
# can see neither the constants nor whether their features are compiled in. A
# number stated where it cannot be derived is a number that drifts — which is
# how both wrong spellings got there.
RESTATE = re.compile(r"(param|parameter).{0,40}services\s+(use|consume)?\s*\(?\d+\)?", re.I)


def sites(text, creator):
    return len(re.findall(rf"^\s+let [a-z0-9_]+ = {creator}::<", text, re.M))


def action_channels(text):
    """The DISTINCT service channels one action server opens.

    Counted by the `ServiceInfo` each `create_service` is handed, not by call
    sites: `action.rs` registers the same three channels twice (the typed and
    the raw arms), so a bare call count says six. The set is what an action
    costs on the wire.
    """
    return {m.group(1) for m in re.finditer(r"create_service\(&(\w+)_info", text)}


def action_server_topics(text):
    """The DISTINCT topics one action SERVER publishes: feedback and status.

    Counted by the topic each `create_publisher` is handed, not by call sites,
    for the same reason `action_channels` counts channels: the typed and raw
    registration arms both appear, so a bare call count doubles.
    """
    return {m.group(1) for m in re.finditer(r"create_publisher\(&(\w+)_topic", text)}


def action_client_topics(text):
    """The DISTINCT topics one action CLIENT subscribes to: feedback.

    Same counting rule. If status ever becomes a subscription rather than a
    poll, this set grows and ACTION_CLIENT_SUBSCRIPTIONS must move with it --
    which is the whole point of tying them.
    """
    return {m.group(1) for m in re.finditer(r"create_subscription\(&(\w+)_topic", text)}


def declared(text, name):
    m = re.search(rf"^pub const {name}: usize = (\d+);", text, re.M)
    return int(m.group(1)) if m else None


def read(root, rel):
    with open(os.path.join(root, rel), encoding="utf8") as fh:
        return fh.read()


MIRROR = re.compile(
    r"^const (PARAM_SERVICE_QUERYABLES|LIFECYCLE_SERVICE_QUERYABLES|ACTION_SERVER_QUERYABLES)"
    r": usize = (\d+);",
    re.M,
)

# Files OUTSIDE `packages/rmw` that also mirror a count. The CLI cannot depend
# on `nros-node` either — it is a host binary and that crate is `no_std` and
# built for the target — so `nros ws entity-facts` restates the action
# multiplier and is held to the definition here (phase-392 W5.b2).
EXTRA_MIRROR_FILES = ["packages/cli/nros-cli-core/src/cmd/entity_facts.rs"]


def rmw_rust_files(root, rmw_dir):
    """Tracked `.rs` under `rmw_dir`, via the git index rather than a walk.

    `check-no-tracked-file-find` measured 7m36s -> 0.8s for the same paths, and
    it is right: this gate reads every RMW source on the fast line.
    """
    out = subprocess.run(
        ["git", "-C", root, "ls-files", f"{rmw_dir}/*.rs"],
        capture_output=True, text=True,
    )
    if out.returncode == 0:
        return [p for p in out.stdout.split() if "/target/" not in p]
    found = []
    # walk-ok: the self-test builds a synthetic tree that is not a git
    # repository, so there is no index to query. Never reached on the real tree.
    for dirpath, dirnames, filenames in os.walk(os.path.join(root, rmw_dir)):
        dirnames[:] = [d for d in dirnames if d not in ("target", "build")]
        for fn in filenames:
            if fn.endswith(".rs"):
                found.append(os.path.relpath(os.path.join(dirpath, fn), root))
    return found


def check(root, rmw_dir="packages/rmw"):
    """Return a list of problem strings (empty == pass)."""
    problems = []
    try:
        spin = read(root, SPIN)
    except OSError as e:
        return [f"missing {SPIN}: {e}"]

    for label, creator, const, const_rel in GROUPS:
        n = sites(spin, creator)
        try:
            want = declared(read(root, const_rel), const)
        except OSError as e:
            problems.append(f"{const}: cannot read {const_rel}: {e}")
            continue
        if want is None:
            problems.append(
                f"{const} not found in {const_rel} — the count must have exactly "
                f"one definition, beside the code that creates them."
            )
        elif n == 0:
            # Never agree with a constant because the pattern stopped matching:
            # that is a blind gate reporting success.
            problems.append(
                f"no `{creator}::<...>` sites found in {SPIN} — the creation shape "
                f"changed and this gate is now blind. Fix the pattern; do not delete the check."
            )
        elif n != want:
            problems.append(
                f"{label}: {n} `{creator}` site(s) in {SPIN}, but {const} = {want}. "
                f"A service server is a queryable — update {const_rel} so every pool "
                f"sized from it moves too."
            )

    # phase-392 W5.b2 — an action server is THREE queryables, and the model
    # declares actions separately from services. Same rule as the two above:
    # the constant is held to the code that decides it, so a fourth action
    # channel cannot silently under-size every image declaring an action.
    try:
        action_src = read(root, ACTION)
    except OSError as e:
        problems.append(f"ACTION_SERVER_QUERYABLES: cannot read {ACTION}: {e}")
    else:
        chans = action_channels(action_src)
        want = declared(action_src, "ACTION_SERVER_QUERYABLES")
        if want is None:
            problems.append(
                f"ACTION_SERVER_QUERYABLES not found in {ACTION} — the count must "
                f"have exactly one definition, beside the services it counts."
            )
        elif not chans:
            problems.append(
                f"no `create_service(&<name>_info` sites found in {ACTION} — the "
                f"creation shape changed and this gate is now blind. Fix the "
                f"pattern; do not delete the check."
            )
        elif len(chans) != want:
            problems.append(
                f"action server: {len(chans)} distinct service channel(s) in "
                f"{ACTION} ({', '.join(sorted(chans))}), but "
                f"ACTION_SERVER_QUERYABLES = {want}. A model declaring an action "
                f"server sizes the queryable table from this."
            )

    # phase-392 W5.d — `nros-zpico-build` MIRRORS both counts, and cannot do
    # otherwise: it is a build-script helper, so it can neither depend on
    # `nros-node` to read the constants nor see that crate's features. A mirror
    # is acceptable only while something holds it to the definition — that is
    # the whole difference between this and the seven prose spellings it
    # replaced, of which two had drifted.
    definitions = {}
    for _, _, const, const_rel in GROUPS:
        try:
            definitions[const] = declared(read(root, const_rel), const)
        except OSError:
            definitions[const] = None
    try:
        definitions["ACTION_SERVER_QUERYABLES"] = declared(
            read(root, ACTION), "ACTION_SERVER_QUERYABLES"
        )
    except OSError:
        definitions["ACTION_SERVER_QUERYABLES"] = None

    scanned = list(rmw_rust_files(root, rmw_dir))
    scanned += [r for r in EXTRA_MIRROR_FILES if os.path.isfile(os.path.join(root, r))]
    for rel in scanned:
        full = os.path.join(root, rel)
        try:
            with open(full, encoding="utf8", errors="replace") as fh:
                text = fh.read()
        except OSError:
            continue
        for i, line in enumerate(text.split("\n"), 1):
            if RESTATE.search(line):
                problems.append(
                    f"{rel}:{i} states an infrastructure-queryable count. "
                    f"Name the knob and the cause; the counts live beside "
                    f"the code that creates them."
                )
        for m in MIRROR.finditer(text):
            const, value = m.group(1), int(m.group(2))
            want = definitions.get(const)
            if want is None:
                problems.append(
                    f"{rel} mirrors {const}, but the definition could not be read."
                )
            elif value != want:
                problems.append(
                    f"{rel} mirrors {const} = {value}, definition is {want}. "
                    f"Neither a build-script helper nor the host CLI can read "
                    f"the constant, so the mirror is held here instead "
                    f"(phase-392 W5.d / W5.b2)."
                )
    # phase-412 W1 -- an action costs PUBLISHER and SUBSCRIBER slots too, and a
    # consumer sizing those pools from a declaration has to add them. Same rule
    # and same failure if they drift: an image declaring an action is sized
    # short, and under-sizing halts the board rather than warning.
    try:
        action_src2 = read(root, ACTION)
    except OSError as e:
        problems.append(f"ACTION_SERVER_PUBLISHERS: cannot read {ACTION}: {e}")
    else:
        for const, fn in (("ACTION_SERVER_PUBLISHERS", action_server_topics),
                          ("ACTION_CLIENT_SUBSCRIPTIONS", action_client_topics)):
            topics = fn(action_src2)
            want = declared(action_src2, const)
            if want is None:
                problems.append(
                    f"{const} not found in {ACTION} — the count must have exactly "
                    f"one definition, beside the calls it counts."
                )
            elif not topics:
                problems.append(
                    f"{const}: found no matching creation calls in {ACTION}; the "
                    f"pattern this gate counts by has moved."
                )
            elif len(topics) != want:
                problems.append(
                    f"{const} says {want} but {ACTION} creates {len(topics)} "
                    f"distinct topic(s): {sorted(topics)}. A pool sized from this "
                    f"constant would be short for every image declaring an action."
                )

    return problems


def _write(root, n_param, n_lc, c_param, c_lc, rmw_line, chans=3, c_action=3,
           c_cli_action=3):
    for rel in (SPIN, PARAMS, LIFECYCLE, ACTION, "packages/rmw/zenoh/x/src/service.rs"):
        os.makedirs(os.path.join(root, os.path.dirname(rel)), exist_ok=True)
    body = "".join(f"        let h{i} = create_param_srv::<T>(\n" for i in range(n_param))
    body += "".join(f"        let l{i} = create_lc_srv::<T>(\n" for i in range(n_lc))
    open(os.path.join(root, SPIN), "w").write(body)
    open(os.path.join(root, PARAMS), "w").write(
        f"pub const PARAM_SERVICE_QUERYABLES: usize = {c_param};\n")
    open(os.path.join(root, LIFECYCLE), "w").write(
        f"pub const LIFECYCLE_SERVICE_QUERYABLES: usize = {c_lc};\n")
    # Written TWICE, as the real file does (typed + raw arms), so the probe
    # for "distinct channels, not call sites" is a real one.
    act = f"pub const ACTION_SERVER_QUERYABLES: usize = {c_action};\n"
    # phase-412 W1 -- the publisher/subscriber multipliers live in the same
    # file and are checked by the same run, so the fixture has to carry them
    # or the clean case reports problems that belong to a missing fixture
    # rather than to a drifted count.
    act += "pub const ACTION_SERVER_PUBLISHERS: usize = 2;\n"
    act += "pub const ACTION_CLIENT_SUBSCRIPTIONS: usize = 1;\n"
    for _ in range(2):
        act += "".join(f"    .create_service(&chan{i}_info, qos)\n" for i in range(chans))
        act += "    .create_publisher(&feedback_topic, qos)\n"
        act += "    .create_publisher(&status_topic, qos)\n"
        act += "    .create_subscription(&feedback_topic, qos)\n"
    open(os.path.join(root, ACTION), "w").write(act)
    open(os.path.join(root, "packages/rmw/zenoh/x/src/service.rs"), "w").write(rmw_line + "\n")
    # The CLI mirror lives outside `packages/rmw`, so it is reached by a
    # different arm of the scan — exercise it rather than assume it.
    cli = EXTRA_MIRROR_FILES[0]
    os.makedirs(os.path.join(root, os.path.dirname(cli)), exist_ok=True)
    open(os.path.join(root, cli), "w").write(
        f"const ACTION_SERVER_QUERYABLES: usize = {c_cli_action};\n")


def self_test():
    """Every probe asserts a failure this gate must catch, plus the clean case,
    so a gate that stopped matching anything cannot report success."""
    cases = [
        ((6, 5, 6, 5, "// nothing"), 0, "counts agree"),
        ((6, 5, 6, 6, "// nothing"), 1, "lifecycle constant drifted to 6 (the historical error)"),
        ((7, 5, 6, 5, "// nothing"), 1, "a 7th parameter service was added"),
        ((0, 5, 6, 5, "// nothing"), 1, "creation-site pattern stopped matching"),
        ((6, 5, 6, 5, "// parameter services use 6"), 1, "an RMW restated a count"),
        ((6, 5, 6, 5, "const PARAM_SERVICE_QUERYABLES: usize = 6;"), 0,
         "a build-script mirror that agrees"),
        ((6, 5, 6, 5, "const PARAM_SERVICE_QUERYABLES: usize = 7;"), 1,
         "a build-script mirror that drifted"),
        ((6, 5, 6, 5, "// nothing", 4, 3), 1, "a fourth action channel was added"),
        ((6, 5, 6, 5, "// nothing", 0, 3), 1, "the action creation pattern stopped matching"),
        ((6, 5, 6, 5, "// nothing", 3, 6), 1,
         "the action constant counted CALL SITES (6) rather than channels (3)"),
        ((6, 5, 6, 5, "// nothing", 3, 3, 4), 1,
         "the CLI mirror drifted — a file OUTSIDE packages/rmw"),
    ]
    failures = 0
    tmp = tempfile.mkdtemp()
    try:
        for args, want, label in cases:
            root = os.path.join(tmp, "t")
            shutil.rmtree(root, ignore_errors=True)
            _write(root, *args)
            got = 1 if check(root) else 0
            if got != want:
                sys.stderr.write(f"  self-test FAIL: {label} — got {got}, want {want}\n")
                failures += 1
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    if failures:
        sys.stderr.write(f"check-infra-queryable-counts self-test: FAILED ({failures})\n")
        sys.exit(1)
    print("check-infra-queryable-counts self-test: OK")


def main():
    # On the NORMAL path, not behind a flag: a negative control nobody runs
    # decays into a comment (`check-gate-selftests`).
    self_test()
    if "--self-test" in sys.argv:
        return
    problems = check(ROOT)
    if problems:
        sys.stderr.write("check-infra-queryable-counts: %d problem(s) — issue 0827:\n" % len(problems))
        for p in problems:
            sys.stderr.write(f"  - {p}\n")
        sys.exit(1)
    for label, creator, const, _ in GROUPS:
        n = sites(read(ROOT, SPIN), creator)
        print(f"  ok    {label}: {n} creation site(s) == {const}")
    print(
        f"  ok    action server: {len(action_channels(read(ROOT, ACTION)))} service "
        f"channel(s) == ACTION_SERVER_QUERYABLES"
    )
    print("infra-queryables: counts agree with their creation sites.")


if __name__ == "__main__":
    main()
