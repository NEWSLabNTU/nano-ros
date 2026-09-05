#!/usr/bin/env python3
"""phase-379 W5 — `nros::prelude` is the rclrs-shaped API, not everything.

The surface is 709 items. A ROS 2 developer reading a ported node should meet
the names that correspond to rclrs/rclcpp/rclc, and not step over the RTOS
machinery that exists because the target is an MCU.

The membership rule is MECHANICAL rather than taste, which is the only reason it
can be enforced at all:

    a name belongs in `nros::prelude` IFF the parity ledger does not classify it
    as an `extension` — i.e. it has a correspondent upstream

`extension` is precisely "ours-only, deliberately". Those names belong in
`nros::embedded`, unless they are load-bearing for startup, which is the
ALLOWED_EXTENSIONS list below: an argued allow-list, one reason per name, not a
place to put anything inconvenient.

Without this the rule is a comment in `lib.rs`, and a comment does not survive
the next person adding a re-export to the prelude because it was handy.

# Every verdict gets an ANSWER, not a default

The rule above turns on one verdict and used to say nothing about the others,
so a verdict added to the ledger later became prelude-eligible by OMISSION --
nobody decided it, the `!= "extension"` test just fell through. PRELUDE_EXCLUDES
below states the answer for each verdict the ledger may carry, and an unknown
verdict is a hard failure here, so adding one to `scripts/api-parity.py` forces
the decision at this gate too.

`their-rename` (phase-425) is the case that made this concrete: it means the
names differ, the capability MATCHES, and ours is the spelling the broader ROS 2
ecosystem uses. A correspondent therefore EXISTS, which is precisely what the
membership rule asks for, so a `their-rename` name is prelude-eligible and is
NOT excluded. Filing such an item as `extension` instead — which is what an
earlier attempt on `rust:ParameterType` did, for want of a better bucket — would
have evicted a name every ported parameter node has to write. That is the gate
working as designed on a mis-verdicted row, not a reason to weaken it.
"""
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LIB = os.path.join(ROOT, "packages", "api", "nros", "src", "lib.rs")
LEDGER = os.path.join(ROOT, "docs", "reference", "api-parity-ledger")

# What each ledger verdict means for the prelude: True = the name has no
# upstream correspondent, so a ROS 2 developer should not meet it through the
# glob. Every verdict in `scripts/api-parity.py`'s VERDICTS must appear here;
# `rust_extensions` fails on one that does not, rather than letting it default
# to "eligible".
PRELUDE_EXCLUDES = {
    # ours-only, deliberately — the only verdict that says "ROS 2 has none".
    "extension": True,
    # ours differs in SHAPE under a platform constraint; the name corresponds.
    "divergence": False,
    # ROS 2 has it and we do not, so there is no name of ours to place.
    "declined": False,
    # as above; the row records that we should.
    "gap": False,
    # the names differ and OURS should change — it still corresponds.
    "rename": False,
    # the names differ and THEIRS should change: ours is the ecosystem's
    # spelling, so the correspondent exists and the name belongs in the prelude.
    "their-rename": False,
}

# Extensions that may sit in the prelude anyway. Each needs a REASON, and the
# reason has to be "a node cannot start without it" — not "it is convenient".
ALLOWED_EXTENSIONS = {
    # Startup. Nothing opens a session or drives the executor without these.
    "ExecutorConfig": "nothing opens a session without it; rclrs takes a Context instead",
    "SpinOptions": "the argument to every spin call",
    "ExecutorConfigEnvExt": "issue 0687 — `from_env()` is an extension trait, so the spelling needs it in scope",
    "SpinOnceResult": "what `spin_once` returns; unavoidable at the first call site",
    "SpinPeriodResult": "as above, for the periodic form",
    "SpinPeriodPollingResult": "as above, for the polling form",
    "SessionMode": "names the transport mode a session opens in",
    "TransportError": "the error half of every runtime Result a node handles",
    "Trigger": "the executor wake primitive a node passes to spin",
    # Defining messages. rclrs spells the trait `Message`; the CONCEPT
    # corresponds, only the name is ours, and a user cannot declare a topic
    # type without naming these.
    "RosMessage": "the message trait — rclrs's `Message` under our spelling",
    "Serialize": "message codegen implements it; a hand-written type must name it",
    "Deserialize": "as above, receive side",
    "TopicInfo": "describes the topic a handle was created on",
    "MessageInfo": "the per-sample metadata a subscription callback receives",
    # Actions. Ours are UUID-addressed by decision (see the action ledger
    # rows), so the identity vocabulary IS the user-facing API here — there is
    # no goal handle to hang it off.
    "RosAction": "the action trait, counterpart to RosMessage",
    "GoalId": "the UUID a goal is named by — our actions are identity-addressed",
    "GoalStatus": "the spec's goal state, returned to the caller",
    "GoalResponse": "accept/reject, returned from the goal callback",
    "GoalInfo": "goal id + stamp, as the spec pairs them",
    "GoalStatusStamped": "the status topic's payload",
    "FeedbackStream": "how a client reads feedback without owning a handle",
    # Node construction and parameters.
    "NodeConfig": "the node's own construction options",
    "ParameterDefault": "declaring a parameter requires naming its default",
    "ParameterError": "the error half of every parameter call",
    # Lifecycle. rclcpp has LifecycleNode; our polling shape differs enough
    # that the names are ours, but a lifecycle node cannot be written without
    # them.
    "LifecyclePollingNode": "the lifecycle node type itself",
    "LifecycleCallbackFn": "the transition callback signature",
    "LifecycleTransition": "the transition being requested",
    "LifecycleError": "the error half of a transition",
    "TransitionResult": "what a transition callback returns",
}


def prelude_names(text):
    """Identifiers re-exported by `pub mod prelude`.

    Parsed from the module body rather than by expanding the glob, because the
    question is what the glob PUBLISHES, and that is what is written here.
    """
    start = text.index("pub mod prelude {")
    depth, i = 0, start
    while i < len(text):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                break
        i += 1
    body = text[start : i + 1]
    names = set()
    for group in re.findall(r"pub use crate::\{(.*?)\};", body, re.S):
        for raw in group.split(","):
            name = raw.strip().split(" as ")[0].strip()
            # skip comments and paths
            if not name or name.startswith("//") or "::" in name:
                continue
            if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name):
                names.add(name)
    for single in re.findall(r"pub use crate::([A-Za-z_][A-Za-z0-9_]*);", body):
        names.add(single)
    return names


def load_ledger(ledger_dir):
    """Every shard merged into one {key: row}, documentation dropped."""
    rows = {}
    for fn in sorted(os.listdir(ledger_dir)):
        if not fn.endswith(".json"):
            continue
        with open(os.path.join(ledger_dir, fn)) as fh:
            data = json.load(fh)
        for key, row in data.items():
            if key.startswith("_") or not isinstance(row, dict):
                continue
            rows[key] = row
    return rows


def rust_extensions(rows):
    """({item: key} the prelude must not export, [unknown verdicts seen]).

    Takes an already-merged ledger so the self-test can drive it with rows that
    exist nowhere on disk — a classifier exercised only against the real ledger
    proves the real ledger, not the classifier.
    """
    out = {}
    unknown = {}
    for key, row in rows.items():
        verdict = row.get("verdict")
        if verdict not in PRELUDE_EXCLUDES:
            unknown.setdefault(verdict, key)
            continue
        if not PRELUDE_EXCLUDES[verdict]:
            continue
        lang, _, item = key.partition(":")
        if lang != "rust":
            continue
        # WHOLE items only. A `Type::method` row says the METHOD is
        # ours-only, NOT the type: `Node::publisher_count` is an extension
        # while `Node` plainly corresponds to rclrs's `Node`. Collapsing to
        # the owning type flagged 47 names, almost all of them types that
        # obviously have correspondents — the rule has to ask about the
        # name the prelude actually exports.
        if "::" in item or "(" in item:
            continue
        item = item.strip()
        if item:
            out.setdefault(item, key)
    return out, sorted(unknown.items())


def self_test():
    """Negative control, on the normal path.

    A checker that cannot fail is a comment. Runs against synthetic text so it
    proves the DETECTION rather than the current state of the tree.
    """
    sample = """
pub mod prelude {
    pub use crate::{Node, CdrWriter};
    pub use crate::ExecutorConfig;
}
"""
    got = prelude_names(sample)
    if got != {"Node", "CdrWriter", "ExecutorConfig"}:
        print("selftest: prelude parse returned %r" % (sorted(got),), file=sys.stderr)
        return False
    # A brace inside the body must not end it early.
    nested = """
pub mod prelude {
    #[cfg(feature = "x")]
    pub use crate::{A, B};
}
"""
    if prelude_names(nested) != {"A", "B"}:
        print("selftest: cfg-gated re-export was not seen", file=sys.stderr)
        return False

    # The verdict classification, driven by rows that exist nowhere on disk.
    # Each case is one way the rule can go wrong, and the `their-rename` case is
    # the regression this gate is named in: a name whose ONLY difference from
    # ROS 2 is that upstream spells it worse still has a correspondent, so
    # excluding it from the prelude would evict `ParameterType`.
    synthetic = {
        "rust:OnlyOurs": {"verdict": "extension", "why": "x"},
        "rust:Shaped": {"verdict": "divergence", "why": "x"},
        "rust:TheyRenamed": {"verdict": "their-rename", "why": "x"},
        "rust:WeShouldRename": {"verdict": "rename", "why": "x"},
        "rust:Missing": {"verdict": "gap", "why": "x"},
        "rust:NotOurs": {"verdict": "declined", "why": "x"},
        "rust:Type::method": {"verdict": "extension", "why": "x"},
        "cpp:OnlyOursCpp": {"verdict": "extension", "why": "x"},
    }
    excluded, unknown = rust_extensions(synthetic)
    if set(excluded) != {"OnlyOurs"}:
        print("selftest: prelude exclusion set is %r, expected just the "
              "`extension`" % (sorted(excluded),), file=sys.stderr)
        return False
    if unknown:
        print("selftest: known verdicts reported unknown: %r" % (unknown,), file=sys.stderr)
        return False

    # A verdict the ledger grows and this gate has never heard of must be
    # REPORTED, not silently treated as prelude-eligible. Without this the
    # membership rule answers by omission for every future verdict.
    _, unknown = rust_extensions({"rust:Novel": {"verdict": "brand-new", "why": "x"}})
    if unknown != [("brand-new", "rust:Novel")]:
        print("selftest: an unknown verdict was not reported (%r)" % (unknown,),
              file=sys.stderr)
        return False

    # And every verdict `api-parity.py` accepts must have an answer here. Read
    # as text rather than imported: that module pulls in the whole correlator on
    # import, and this gate must stay a fast, dependency-free script.
    src = os.path.join(ROOT, "scripts", "api-parity.py")
    if os.path.exists(src):
        with open(src) as fh:
            m = re.search(r"^VERDICTS = \((.*?)\)", fh.read(), re.M | re.S)
        if not m:
            print("selftest: could not read VERDICTS from api-parity.py", file=sys.stderr)
            return False
        declared = set(re.findall(r'"([^"]+)"', m.group(1)))
        missing = sorted(declared - set(PRELUDE_EXCLUDES))
        if missing:
            print("selftest: PRELUDE_EXCLUDES has no answer for %r — decide whether "
                  "each belongs in `nros::prelude` before the ledger uses it"
                  % (missing,), file=sys.stderr)
            return False
    return True


def main():
    if not self_test():
        print("check-prelude-tiers: selftest failed — the checker is not trustworthy.", file=sys.stderr)
        return 1

    with open(LIB) as fh:
        names = prelude_names(fh.read())
    exts, unknown = rust_extensions(load_ledger(LEDGER))

    if unknown:
        print("check-prelude-tiers: the ledger carries %d verdict(s) this gate has "
              "no answer for:" % len(unknown))
        for verdict, key in unknown:
            print("  %-14s (first seen at %s)" % (verdict, key))
        print()
        print("  Prelude membership asks \"does this name have a correspondent")
        print("  upstream?\". Add each verdict to PRELUDE_EXCLUDES in")
        print("  %s with True (no correspondent," % os.path.relpath(__file__, ROOT))
        print("  keep it out of the glob) or False (it has one).")
        return 1

    bad = []
    for name in sorted(names):
        if name in exts and name not in ALLOWED_EXTENSIONS:
            bad.append((name, exts[name]))

    if bad:
        print("check-prelude-tiers: %d name(s) in `nros::prelude` are ledger `extension`s:" % len(bad))
        for name, key in bad:
            print("  %-28s (%s)" % (name, key))
        print()
        print("  An `extension` has no correspondent in rclrs/rclcpp/rclc, so a ROS 2")
        print("  developer reading a ported node should not meet it through the glob.")
        print("  Move it to `nros::embedded`, or add it to ALLOWED_EXTENSIONS in")
        print("  %s with a reason that is" % os.path.relpath(__file__, ROOT))
        print("  \"a node cannot start without it\" — phase-379 W5.")
        return 1

    print(
        "check-prelude-tiers: OK (selftest ok; %d prelude name(s), %d rust extension(s), %d allowed)"
        % (len(names), len(exts), len(ALLOWED_EXTENSIONS))
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
