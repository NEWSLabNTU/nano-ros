#!/usr/bin/env python3
"""Every input the CLI source stamp folds must be one the refusal can NAME.

Issue 1018, the residue phase-424 and phase-429 left standing.

THE RULE
--------
The in-tree `nros` refuses to run when its binary does not match its sources::

    Error: in-tree nros CLI is STALE -- its sources changed since it was built

Phase-429 measured that refusal and found it CORRECT in each of the three stops
issue 1018 reported, including the one the issue calls interesting: moving the
`play_launch` submodule pin forward. That pin is a genuine CLI build input --
`build.rs` bakes it as `NROS_PLAY_LAUNCH_SHA`, and issue 0561 records what a
stamp blind to it costs.

What was NOT correct was the EXPLANATION. The stamp is one FNV fold of several
different kinds of input, and one number cannot say which of them moved, so the
message guessed by elimination: no uncommitted CLI edits, therefore "the
checkout moved, e.g. a branch switch". A contributor who had moved a submodule
pin and touched nothing else was told their checkout had moved. It had not.

Elimination cannot be repaired, either, because two inputs can move at once and
the residual arm then names whichever one it checked last. So the stamp is
computed PER INPUT and baked per input, and this gate holds the three places
that have to agree:

1. ``source_stamp_components()`` folds one component per input, each SEEDED with
   its own label, so the label is part of the hashed domain rather than a
   comment. That is what makes it harvestable from the code that folds it.
2. ``STAMP_INPUTS`` lists exactly those labels.
3. ``stale_guard.rs`` carries one ``// ATTRIBUTES: <label>`` arm per label.

WHAT IT CHECKS, AND IN BOTH DIRECTIONS
--------------------------------------
* every folded label is in ``STAMP_INPUTS`` -- a new input that is stamped but
  not listed is an input nothing can be said about;
* every ``STAMP_INPUTS`` entry is actually folded -- a list may not name a
  phantom, which is how an authored map drifts toward OK (CLAUDE.md's
  rmw-parity case: two green tools disagreeing by 25 symbols);
* every ``STAMP_INPUTS`` entry has an attribution arm, and every attribution arm
  names a listed input;
* the declared array arity matches the number of entries.

And the tag must sit ON its arm: after ``// ATTRIBUTES: X`` the next line that
is neither blank nor a comment must be the match arm ``"X" =>``. Comments in
between are fine and expected -- requiring literal adjacency is the mistake
``check-c-array-pool-floors`` made, where two knobs that documented themselves
between the two lines were invisible to a gate that reported 21 arrays over a
tree with 23.

WHAT IT DELIBERATELY DOES NOT CHECK
-----------------------------------
That an arm's TEXT is right. A tag proves a decision was taken, not that the
sentence under it is true -- issue 1167's lesson one lane over, that a guard
which exists is not a guard that fires. The measurement lives in
``stale_guard::attribution_tests``: it perturbs each input in a real checkout
and reads what comes out, and its ``perturb`` match has no catch-all, so a label
added to ``STAMP_INPUTS`` without a perturbation case does not COMPILE. This
gate is the buildless half, on the fast line, where a compiler is not available.

Usage::

    check-stale-cli-attribution.py [--selftest]
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

STAMP_FILE = "packages/cli/nros-cli-core/src/source_stamp.rs"
GUARD_FILE = "packages/cli/nros-cli-core/src/stale_guard.rs"

# The function whose folds ARE the stamp's inputs. Harvesting from anywhere else
# in the file would pick up `fnv1a` calls that hash content, not components.
COMPONENT_FN = "pub fn source_stamp_components"

LIST_RE = re.compile(r"pub const STAMP_INPUTS:\s*\[&str;\s*(\d+)\]\s*=\s*\[(.*?)\];", re.S)
# The component seed: `fnv1a(b"<label>", FNV_OFFSET)`. Anchored on FNV_OFFSET so
# a fold that CONTINUES a hash (`fnv1a(b"x", h)`) is not mistaken for a new
# component -- only a fresh seed starts one.
SEED_RE = re.compile(r'fnv1a\(b"([a-z0-9_]+)",\s*FNV_OFFSET\)')
TAG_RE = re.compile(r"^\s*//\s*ATTRIBUTES:\s*([a-z0-9_]+)\s*$")
ARM_RE = re.compile(r'^\s*"([a-z0-9_]+)"\s*=>')


def declared_inputs(stamp_src):
    """(labels, declared_arity) from `pub const STAMP_INPUTS`."""
    m = LIST_RE.search(stamp_src)
    if not m:
        return None, None
    arity = int(m.group(1))
    labels = re.findall(r'"([a-z0-9_]+)"', m.group(2))
    return labels, arity


def function_body(src, header):
    """The text of `header`'s body, brace-balanced from its opening `{`."""
    i = src.find(header)
    if i < 0:
        return ""
    j = src.find("{", i)
    if j < 0:
        return ""
    depth, k = 1, j + 1
    while k < len(src) and depth:
        if src[k] == "{":
            depth += 1
        elif src[k] == "}":
            depth -= 1
        k += 1
    return src[j:k]


def folded_inputs(stamp_src):
    """Labels seeded as components, in fold order."""
    body = function_body(stamp_src, COMPONENT_FN)
    out = []
    for label in SEED_RE.findall(body):
        if label not in out:
            out.append(label)
    return out


def attribution_tags(guard_src):
    """`(label, arm_or_None)` for each `// ATTRIBUTES:` tag, in file order.

    `arm` is the label of the first match arm at or after the tag, skipping
    blank and comment lines -- so a tag may document itself at length and still
    be held to the arm it introduces, while a tag that has drifted off its arm
    (or onto nothing) is reported.
    """
    lines = guard_src.splitlines()
    out = []
    for n, line in enumerate(lines):
        m = TAG_RE.match(line)
        if not m:
            continue
        arm = None
        for later in lines[n + 1 :]:
            s = later.strip()
            if not s or s.startswith("//"):
                continue
            a = ARM_RE.match(later)
            arm = a.group(1) if a else None
            break
        out.append((m.group(1), arm))
    return out


def findings(read):
    """Every disagreement between the three places, as printable lines."""
    stamp_src = read(STAMP_FILE)
    guard_src = read(GUARD_FILE)
    bad = []

    declared, arity = declared_inputs(stamp_src)
    if declared is None:
        return [f"{STAMP_FILE}: no `pub const STAMP_INPUTS: [&str; N]` declaration"]
    if arity != len(declared):
        bad.append(
            f"{STAMP_FILE}: STAMP_INPUTS declares [&str; {arity}] but lists "
            f"{len(declared)} labels"
        )

    folded = folded_inputs(stamp_src)
    if not folded:
        bad.append(
            f"{STAMP_FILE}: `{COMPONENT_FN}` seeds no component "
            '(expected `fnv1a(b"<label>", FNV_OFFSET)` per input)'
        )
    for label in folded:
        if label not in declared:
            bad.append(
                f"{STAMP_FILE}: `{label}` is folded as a stamp component but is "
                "not in STAMP_INPUTS -- an input nothing can attribute"
            )
    for label in declared:
        if label not in folded:
            bad.append(
                f"{STAMP_FILE}: STAMP_INPUTS names `{label}` but no component "
                "seeds it -- a list that names a phantom drifts toward OK"
            )

    tags = attribution_tags(guard_src)
    tagged = [label for label, _ in tags]
    for label, arm in tags:
        if label not in declared:
            bad.append(
                f"{GUARD_FILE}: `// ATTRIBUTES: {label}` names no STAMP_INPUTS entry"
            )
        if arm != label:
            bad.append(
                f"{GUARD_FILE}: `// ATTRIBUTES: {label}` does not sit on its arm "
                f'(next code line is {arm and chr(34) + arm + chr(34) or "not a match arm"})'
            )
    for label in declared:
        if label not in tagged:
            bad.append(
                f"{GUARD_FILE}: stamp input `{label}` has no `// ATTRIBUTES: {label}` "
                "arm -- the refusal would have to guess what moved"
            )
    return bad


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        return selftest(verbose=True)
    # On the NORMAL path, every time -- a negative control nobody runs decays
    # into a comment (AGENTS.md "a gate must run its own selftest").
    selftest()

    bad = findings(lambda rel: (REPO / rel).read_text(errors="replace"))
    if bad:
        print("check-stale-cli-attribution: FAILED", file=sys.stderr)
        for line in bad:
            print(f"  {line}", file=sys.stderr)
        print(
            "\nThe stale-CLI refusal explains itself from the stamp's components. An\n"
            "input it cannot name is an input it describes as something else: issue\n"
            "1018's stop 2 was a submodule pin move reported as `the checkout moved,\n"
            "e.g. a branch switch`, which sent a contributor to look at a tree that\n"
            "was fine. Add the label to STAMP_INPUTS, seed its component with\n"
            '`fnv1a(b"<label>", FNV_OFFSET)`, and give it a `// ATTRIBUTES: <label>`\n'
            "arm in stale_guard.rs saying what a reader should do about it.",
            file=sys.stderr,
        )
        return 1
    declared, _ = declared_inputs((REPO / STAMP_FILE).read_text())
    print(
        f"check-stale-cli-attribution: OK ({len(declared)} stamp inputs, "
        "each folded, listed and attributed)"
    )
    return 0


def selftest(verbose=False):
    ok = fail = 0

    def chk(what, cond):
        nonlocal ok, fail
        if cond:
            ok += 1
            if verbose:
                print(f"  ok   {what}")
        else:
            fail += 1
            print(f"  FAIL {what}", file=sys.stderr)

    def tree(list_src, fold_src, guard_src):
        stamp = (
            f"{list_src}\n"
            f"{COMPONENT_FN}(root: &Path) -> Option<Vec<(&'static str, String)>> {{\n"
            f"{fold_src}"
            "    Some(out)\n}\n"
            # Prose and a CONTINUING fold outside the component function must
            # not be harvested as inputs.
            'fn other() { let h = fnv1a(b"decoy", h); }\n'
        )
        return {STAMP_FILE: stamp, GUARD_FILE: guard_src}.get

    good_list = 'pub const STAMP_INPUTS: [&str; 2] = ["cli_sources", "play_launch_pin"];'
    good_fold = (
        '    let mut h = fnv1a(b"cli_sources", FNV_OFFSET);\n'
        '    let mut h = fnv1a(b"play_launch_pin", FNV_OFFSET);\n'
    )
    good_guard = (
        "fn attribution() {\n    match label {\n"
        "        // ATTRIBUTES: cli_sources\n"
        '        "cli_sources" => {}\n'
        "        // ATTRIBUTES: play_launch_pin\n"
        "        //\n"
        "        // A tag may document itself at length before its arm.\n"
        '        "play_launch_pin" => {}\n'
        "    }\n}\n"
    )

    chk("all three agreeing passes", findings(tree(good_list, good_fold, good_guard)) == [])

    # A new input is stamped but nobody lists or attributes it.
    unlisted = good_fold + '    let mut h = fnv1a(b"sdk_pin", FNV_OFFSET);\n'
    out = findings(tree(good_list, unlisted, good_guard))
    chk("a folded input missing from STAMP_INPUTS FAILS",
        any("`sdk_pin` is folded" in f for f in out))

    # Listed and folded, but no attribution arm.
    three = 'pub const STAMP_INPUTS: [&str; 3] = ["cli_sources", "play_launch_pin", "sdk_pin"];'
    out = findings(tree(three, unlisted, good_guard))
    chk("a listed input with no attribution arm FAILS",
        any("has no `// ATTRIBUTES: sdk_pin`" in f for f in out))

    # An arm for something that is not an input.
    stray = good_guard.replace(
        "        // ATTRIBUTES: cli_sources\n",
        "        // ATTRIBUTES: retired_input\n        // ATTRIBUTES: cli_sources\n",
    )
    out = findings(tree(good_list, good_fold, stray))
    chk("an attribution arm naming no input FAILS",
        any("`// ATTRIBUTES: retired_input` names no STAMP_INPUTS entry" in f for f in out))

    # A list that names something nothing folds.
    out = findings(tree(three, good_fold, good_guard))
    chk("STAMP_INPUTS naming an unfolded phantom FAILS",
        any("STAMP_INPUTS names `sdk_pin` but no component" in f for f in out))

    # The declared arity must match.
    out = findings(tree(three.replace("[&str; 3]", "[&str; 2]"), unlisted, good_guard))
    chk("a mismatched array arity FAILS", any("declares [&str; 2]" in f for f in out))

    # A tag that has drifted off its arm.
    drifted = good_guard.replace(
        '        // ATTRIBUTES: cli_sources\n        "cli_sources" => {}\n',
        '        // ATTRIBUTES: cli_sources\n        let x = 1;\n        "cli_sources" => {}\n',
    )
    out = findings(tree(good_list, good_fold, drifted))
    chk("a tag that does not sit on its arm FAILS",
        any("does not sit on its arm" in f for f in out))

    # …and a tag sitting on the WRONG arm is the same defect, differently spelt.
    swapped = good_guard.replace('        "cli_sources" => {}\n', '        "other" => {}\n')
    out = findings(tree(good_list, good_fold, swapped))
    chk("a tag sitting on a DIFFERENT arm FAILS",
        any("does not sit on its arm" in f for f in out))

    # Mutation: prose and a continuing fold are not component seeds.
    prose = good_fold + '    // the b"sdk_pin" input is documented here, not folded\n'
    chk("a label in a comment is not a stamp input",
        findings(tree(good_list, prose, good_guard)) == [])
    cont = good_fold + '    h = fnv1a(b"cli_sources", h);\n'
    chk("a fold that CONTINUES a hash is not a new component",
        findings(tree(good_list, cont, good_guard)) == [])

    # And the real tree, read from disk, must satisfy all of it.
    chk("the real source_stamp.rs declares STAMP_INPUTS",
        declared_inputs((REPO / STAMP_FILE).read_text())[0] is not None)

    if verbose:
        print(f"\n{ok} passed, {fail} failed")
    if fail:
        print("check-stale-cli-attribution self-test: FAILED", file=sys.stderr)
        raise SystemExit(1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
