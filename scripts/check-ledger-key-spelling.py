#!/usr/bin/env python3
"""A ledger key must be a key the report can PRINT.

Issue 1351. The ledger's schema already says it — "Key is `<lang>:<normalized
item>` exactly as the report prints it. Run the report to get the spelling; do
not guess it." — and nothing checked it, so a guess survived as a row that
correlates with nothing and reads exactly like a row somebody has not written
yet.

THE CLASS, which is why this exists rather than a fix to two rows.

`scripts/api-parity.py --check` walks the DIFFERENCES and asks each whether the
ledger has a row. Nothing walks the ledger and asks whether a row can ever be
answered. So three states are indistinguishable to every gate:

    a row nobody has written yet              absent
    a row that cannot be matched by the tool  present, inert
    a row whose subject was deleted           present, inert   (issue 1323)

Issue 1323 owns the third and needs the extractor to answer it: "does this
subject still exist?" is a question about clang, rustdoc and a ROS surface. The
SECOND is answerable with no extractor at all, because it is a property of the
KEY: `correlate.normalize` is the function that computes every key the report
prints, and it is idempotent on its own output. So a ledger key that is not a
FIXED POINT of it is a key the report can never emit — no correlation can reach
it, in any bucket, whatever the tree does.

MEASURED on the day this landed: 4 of 2770 rows, and all four were real.

    rust:NodeCtx::create_timer_on_clock            -> Node::create_timer_on_clock
    rust:NodeCtx::create_timer_on_clock_in_group   -> Node::create_timer_on_clock_in_group
    cpp:GenericTimer::GenericTimer<FunctorT, std::shared_ptr<Clock>>
    cpp:WallTimer::WallTimer<FunctorT, std::shared_ptr<Clock>>

The first two are issue 1351's own: written by phase-430 W4 while the extractor
could not reach `NodeCtx` at all, so the key was guessed, and the guess could
not have been right — `TYPE_SYNONYMS` folds `NodeCtx` onto `Node`, which is a
deliberate correlator rule and not an accident. The other two are the same
defect from a different direction: the extractor renders that constructor's
defaulted template argument as empty, so the key it prints is
`GenericTimer::GenericTimer<FunctorT, >`, and the rows naming the SOURCE
spelling had been inert since they were written (the report served those lines
by inheritance from the type row, which is what the `divergence*` asterisk
says).

WHY A HARD FAILURE AND NOT A COUNTED EXEMPTION. The obvious alternative is to
let a row DECLARE that it is outside the tool's reach and ratchet the count down.
Two reasons not to. A declaration in prose is not checkable, which is the exact
failure being fixed — the two rows above said, in prose, that they were recorded
ahead of the tool, and a reader had no way to tell that from a typo. And an
exemption list is a place to park this defect, where the fix is cheap and
mechanical: re-key the row to the spelling this gate prints, or delete it. There
is no legitimate reason for a row to carry a key its own tool cannot emit.

WHAT THIS IS NOT. It does not check that the key has a SUBJECT — a perfectly
well-spelled key for a symbol nobody has ever shipped passes here and is issue
1323's. It does not check the reason, the line numbers, or the verdict. A green
here means only that every row is ADDRESSABLE by the report.
"""

import json
import os
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
LEDGER = ROOT / "docs/reference/api-parity-ledger"

sys.path.insert(0, str(ROOT / "scripts" / "api_parity"))
import correlate  # noqa: E402

# The report's own lane names, as `scripts/api-parity.py` spells them, mapped
# onto the language token `correlate.normalize` takes. DERIVED from the ledger
# key prefix rather than authored per row.
LANGS = {"c": "c", "cpp": "c++", "rust": "rust"}

# `normalize`'s two behaviours. It branches on `kind`: a TYPE-ish item keeps its
# own name and drops its module path, a MEMBER-ish one keeps `Owner::name`. A
# ledger key does not record which it is, so a key is accepted when it is a
# fixed point under EITHER — conservative in the only direction that matters,
# since a false positive here would be a gate nobody can satisfy.
KINDS = ("function", "type")


def offenders(rows):
    """[(key, printed)] for every key `normalize` would spell differently.

    `rows` is an iterable of ledger keys. `printed` is what the report emits
    for the same item under the member reading, which is the spelling to
    re-key to in every case seen so far.
    """
    bad = []
    for key in rows:
        if key.startswith("_") or ":" not in key:
            continue
        lang, item = key.split(":", 1)
        cl = LANGS.get(lang)
        if cl is None:
            bad.append((key, "<unknown language prefix %r>" % lang))
            continue
        spellings = [correlate.normalize(cl, "ours", item, k) for k in KINDS]
        if item in spellings:
            continue
        bad.append((key, "%s:%s" % (lang, spellings[0])))
    return bad


def scan(files):
    out = []
    for f in files:
        doc = json.loads(f.read_text())
        for key, printed in offenders(doc):
            out.append((f.name, key, printed))
    return out


def self_test():
    """Negative controls, on the NORMAL path.

    `check-gate-selftests` requires it and it is right to: the whole gate is
    one call into `correlate.normalize`, and if that import ever resolved to a
    stub — or if the synonym tables were emptied — this would go permanently,
    silently green over a ledger that was drifting. Each control plants one
    spelling the gate must catch and one it must not, and the un-mutated cases
    are checked FIRST so a broken harness cannot be read as a caught mutation
    (issue 1204).
    """
    live = [
        "rust:Node::create_timer_on_clock",  # the corrected spelling
        "rust:init_with_args",  # a free function: no owner to fold
        "rust:LifecyclePollingNodeCtx",  # NOT `NodeCtx`; the synonym is exact
        "rust:Executor::create_node_on*",  # a glob key is still a key
        "cpp:QoS",  # a plain type
        "cpp:NodeBase::get_name",  # `_BASE_KEEP`: a real, separate type
        "cpp:GenericTimer::GenericTimer<FunctorT, >",  # what the extractor prints
        "c:clock_get_now_ns",  # already prefix-stripped
        "_doc",  # documentation, skipped
    ]
    spurious = offenders(live)
    assert not spurious, "self-test: a legitimate key was flagged: %s" % spurious

    mutated = {
        "rust:NodeCtx::create_timer_on_clock": "rust:Node::create_timer_on_clock",
        "rust:NodeState::create_publisher": "rust:Node::create_publisher",
        "cpp:PublisherBase::get_topic_name": "cpp:Publisher::get_topic_name",
        "c:rcl_publisher_init": "c:publisher_init",
        "go:Node::spin": "<unknown language prefix 'go'>",
    }
    caught = dict(offenders(list(mutated)))
    missed = set(mutated) - set(caught)
    assert not missed, "self-test: an unprintable key was not caught: %s" % sorted(missed)
    for key, want in mutated.items():
        assert caught[key] == want, "self-test: %s -> %r, wanted %r" % (key, caught[key], want)
    print(
        "check-ledger-key-spelling --self-test: %d live + %d mutated case(s) OK"
        % (len(live), len(mutated))
    )


def main():
    self_test()

    files = sorted(LEDGER.glob("*.json"))
    if not files:
        print("check-ledger-key-spelling: no ledger shards found at %s" % LEDGER, file=sys.stderr)
        return 1
    rows = sum(len(json.loads(f.read_text())) for f in files)
    bad = scan(files)
    if bad:
        print(
            "FAIL: %d ledger row(s) carry a key the report can never print.\n"
            "      `scripts/api-parity.py` keys every correlation through\n"
            "      `correlate.normalize`, so a key that is not a fixed point of it\n"
            "      matches nothing in any bucket — which reads exactly like a row\n"
            "      nobody has written yet (issue 1351). Re-key to the spelling on\n"
            "      the right, or delete the row if its type already answers.\n" % len(bad),
            file=sys.stderr,
        )
        for shard, key, printed in bad:
            # `normalize` splits on `::` with no bracket awareness, so a key
            # holding a qualified TEMPLATE ARGUMENT gets a suggestion that is
            # itself wrong. Say so rather than hand over a bad re-key: the
            # report's spelling is the authority and a run is how to read it.
            note = "  (`::` inside template args — read the spelling off a run)"
            print(
                "  %-14s %-56s -> %s%s"
                % (shard, key, printed, note if "<" in key else ""),
                file=sys.stderr,
            )
        return 1
    print(
        "check-ledger-key-spelling: OK — %d row(s) across %d shard(s), every key is one "
        "the report can print" % (rows, len(files))
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
