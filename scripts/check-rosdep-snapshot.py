#!/usr/bin/env python3
"""The vendored rosdep snapshot is pinned, normalized, unprobed and MARKED.

RFC-0099 D8 / phase-447 D3. `nros-rosdep-snapshot.toml` is a 150 KB generated
data file: a projection of ros/rosdistro's rosdep database onto the four
package managers this tree installs through, consulted as the last rung of the
`<depend>` ladder before UNKNOWN.

A data blob nobody reads is a supply-chain surface, so this gate asks the four
questions a reviewer would have to ask by hand, offline:

  1. IS IT PINNED?  `upstream`, a 40-hex `ref`, and every input file with a
     sha256. Without those, "is this what upstream says?" has no answer and a
     bump's diff is 150 KB of unattributable text.
  2. IS IT NORMALIZED?  Keys sorted and unique, managers in a fixed order.
     This is what makes a bump LEGIBLE: a sorted file diffs line-per-changed-
     package, so a reviewer reads what upstream changed rather than a reflow.
  3. IS IT UNPROBED, HONESTLY?  No entry may carry a `check` or a `role`. The
     upstream database has no probe to project, and RFC-0062's amendment is
     explicit that the probe is what makes a missing prereq diagnosable. A
     hand-written probe here would survive exactly until the next regeneration.
     No entry may be EMPTY either: a key that resolves a `<depend>` and then
     installs nothing is a silent success.
  4. IS THE MARKING REACHED?  Every consumer that resolves through the snapshot
     must also name `RosdepSnapshot::PROVENANCE_NOTE`. This is the half a
     reader cannot check by eye and the half that matters: a fallback that
     supplies packages without saying they are unprobed is the objection
     RFC-0062 raised, re-created quietly.

Not checked here, because it needs the network: that the committed bytes ARE
the projection of the pinned ref. That is `python3 scripts/gen-rosdep-snapshot.py
--verify`, a maintainer action — a gate that reaches the network fails on a
pristine offline worktree, which this tier is documented to support.

Run:  python3 scripts/check-rosdep-snapshot.py [--self-test]
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SNAPSHOT = os.path.join(ROOT, "nros-rosdep-snapshot.toml")
INDEX = os.path.join(ROOT, "nros-sdk-index.toml")
GENERATOR = os.path.join(ROOT, "scripts", "gen-rosdep-snapshot.py")

MANAGERS = ["apt", "dnf", "pacman", "brew"]

# The two modules that DEFINE the rung. Everything else that reaches it is a
# consumer and owes the reader the provenance note.
DEFINING = {
    "packages/cli/nros-cli-core/src/orchestration/rosdep_snapshot.rs",
    "packages/cli/nros-cli-core/src/orchestration/prereq_resolve.rs",
}
# What a consumer must name. `PROVENANCE_NOTE` is the one spelling of the
# sentence, so requiring the CONSTANT rather than a phrase means a reworded
# note cannot leave a consumer behind.
MARKER = "PROVENANCE_NOTE"
# How a consumer reaches the rung at all.
REACHES = ("Resolution::RosdepSnapshot", "rosdep_snapshot::", "rosdep_fallback")


def loads(text):
    try:
        import tomllib as toml
    except ModuleNotFoundError:  # Python < 3.11 — the repo's interpreter is 3.10
        import tomli as toml
    return toml.loads(text)


def load(path):
    with open(path, encoding="utf8") as fh:
        return loads(fh.read())


def key_order(text):
    """The `[key.*]` names in FILE order, as written (quotes stripped)."""
    out = []
    for m in re.finditer(r'^\[key\.("?)([^\]"]+)\1\]\s*$', text, re.M):
        out.append(m.group(2))
    return out


def manager_order(text):
    """[(key, [manager, ...])] in file order — for the normalization check."""
    out = []
    cur = None
    for line in text.split("\n"):
        m = re.match(r'^\[key\.("?)([^\]"]+)\1\]\s*$', line)
        if m:
            cur = (m.group(2), [])
            out.append(cur)
            continue
        if line.startswith("["):
            cur = None
            continue
        mm = re.match(r"^([a-z0-9_]+)\s*=", line)
        if mm and cur is not None:
            cur[1].append(mm.group(1))
    return out


def rust_files():
    """Every tracked .rs file under packages/, as (relpath, text).

    `git ls-files` — an index lookup, not a filesystem walk (`check-no-
    tracked-file-find`: measured 7m36s -> 0.8s for 232 paths on the same
    class of scan). It also does the right thing for free where the old
    `os.walk` needed an explicit prune list: a gitignored `target/` or
    `generated/` tree is simply not in the index.
    """
    listed = subprocess.run(
        ["git", "-C", ROOT, "ls-files", "-z", "packages/*.rs"],
        capture_output=True,
        check=True,
    ).stdout
    out = []
    for rel in listed.decode().split("\0"):
        if not rel:
            continue
        try:
            with open(os.path.join(ROOT, rel), encoding="utf8") as fh:
                out.append((rel, fh.read()))
        except OSError:
            continue
    return out


def check_consumers(files):
    """Files reaching the rung without naming the provenance marker."""
    bad = []
    for rel, text in files:
        if rel.replace(os.sep, "/") in DEFINING:
            continue
        if not any(r in text for r in REACHES):
            continue
        if MARKER not in text:
            bad.append(rel)
    return bad


def find_data_problems(text, doc):
    """Every problem the DATA has — the pin, normalization and unprobed-ness
    checks, as one pure function of (raw text, parsed doc).

    Pulled out of `main()` so `self_test()` can drive it with a synthetic bad
    snapshot. The COMMITTED file satisfies every one of these checks, so a
    test that only ever runs `main()` against it proves nothing about any
    single check: delete any one of the checks below and `main()` still
    prints `OK` against the real file, because the real file was never wrong
    in the way that check exists to catch. That gap is exactly how the first
    version of this gate shipped four of its checks unable to fail — mutating
    each one out was silent right up until this function existed.
    """
    problems = []

    # 1 — the pin.
    snap = doc.get("snapshot", {})
    if not snap.get("upstream"):
        problems.append("[snapshot] names no `upstream`.")
    ref = snap.get("ref", "")
    if not re.fullmatch(r"[0-9a-f]{40}", ref):
        problems.append("[snapshot] `ref` is not a full 40-hex commit: %r" % ref)
    files = snap.get("file", [])
    if not files:
        problems.append("[snapshot] lists no source file to re-fetch when reviewing a bump.")
    for f in files:
        if not re.fullmatch(r"[0-9a-f]{64}", f.get("sha256", "")):
            problems.append("[[snapshot.file]] %r has no usable sha256." % f.get("path", "?"))
    gen = snap.get("generator", "")
    if not gen or not os.path.isfile(os.path.join(ROOT, gen)):
        problems.append(
            "[snapshot] `generator` = %r does not name a file in this tree — a "
            "reader cannot find the tool that wrote this." % gen
        )
    if "DO NOT EDIT BY HAND" not in text.split("\n", 1)[0]:
        problems.append(
            "the first line must say the file is generated; a hand edit here is "
            "lost at the next bump with no diagnostic."
        )

    # 2 — normalization: sorted, unique, managers in a fixed order.
    order = key_order(text)
    if order and order != sorted(order):
        first = next((a for a, b in zip(order, sorted(order)) if a != b), order[0])
        problems.append(
            "keys are not sorted (first out of place: %r). A sorted file is what "
            "makes a bump's diff readable." % first
        )
    dupes = sorted({k for k in order if order.count(k) > 1})
    if dupes:
        problems.append("duplicate key(s): %s" % ", ".join(dupes))
    for key, mgrs in manager_order(text):
        expected = [m for m in MANAGERS if m in mgrs]
        if mgrs != expected:
            problems.append(
                "[key.%s] lists managers as %s; the fixed order is %s." % (key, mgrs, expected)
            )
            break

    # 3 — unprobed, honestly, and never empty.
    keys = doc.get("key", {})
    for key, entry in sorted(keys.items()):
        extra = sorted(set(entry) - set(MANAGERS))
        if extra:
            problems.append(
                "[key.%s] carries %s. The snapshot projects package lists and "
                "NOTHING else: it has no probe to project, so a `check` written "
                "here is a claim the next regeneration silently deletes." % (key, ", ".join(extra))
            )
            break
        if not any(entry.get(m) for m in MANAGERS):
            problems.append(
                "[key.%s] maps to no package manager — it would resolve a "
                "<depend> and then install nothing." % key
            )
            break

    return problems


# A minimal, hand-built snapshot the self-test mutates one field at a time.
# The committed 150 KB file cannot play this role: it already satisfies every
# check, so it can only prove a check passes, never that it would have caught
# the thing it exists to catch.
_GOOD_HEADER = "# nros-rosdep-snapshot.toml — DO NOT EDIT BY HAND\n"
_GOOD_PIN = (
    _GOOD_HEADER
    + "[snapshot]\n"
    + 'upstream = "https://example.invalid/rosdistro"\n'
    + 'ref = "0123456789abcdef0123456789abcdef01234567"\n'
    + 'generator = "scripts/gen-rosdep-snapshot.py"\n'
    + "[[snapshot.file]]\n"
    + 'path = "rosdep/base.yaml"\n'
    + 'sha256 = "%s"\n' % ("0" * 64)
)
_GOOD_KEY = '[key.a]\napt = ["a"]\n'


def _one_problem(text, fragment):
    """`find_data_problems` on `text` reports EXACTLY one problem, containing
    `fragment`. Exactly-one, not merely non-empty, so a case that is supposed
    to isolate ONE check cannot pass because some OTHER check also fired."""
    probs = find_data_problems(text, loads(text))
    assert len(probs) == 1, "expected exactly 1 problem, got %r" % (probs,)
    assert fragment in probs[0], probs[0]


def self_test():
    assert key_order('[key.aaa]\napt = ["a"]\n[key."g++"]\napt = ["b"]\n') == ["aaa", "g++"]
    # A `[[snapshot.file]]` table must not be read as a key.
    assert key_order("[[snapshot.file]]\npath = \"p\"\n") == []
    got = manager_order('[key.k]\napt = ["a"]\nbrew = ["b"]\n[snapshot]\nref = "x"\n')
    assert got == [("k", ["apt", "brew"])], got
    # A consumer naming the rung without the marker is caught; with it, passes.
    assert check_consumers([("a/b.rs", "Resolution::RosdepSnapshot")]) == ["a/b.rs"]
    assert check_consumers([("a/b.rs", "rosdep_snapshot::X PROVENANCE_NOTE")]) == []
    # A file that mentions neither is not a consumer.
    assert check_consumers([("a/b.rs", "fn main() {}")]) == []
    # The defining modules are exempt — they are where the note is DECLARED.
    d = sorted(DEFINING)[0]
    assert check_consumers([(d, "Resolution::RosdepSnapshot")]) == []

    # find_data_problems: a clean synthetic snapshot reports nothing …
    assert find_data_problems(_GOOD_PIN + _GOOD_KEY, loads(_GOOD_PIN + _GOOD_KEY)) == []
    # … and each check below fires ALONE when its one thing is wrong. Each
    # case starts from the same good snapshot and breaks exactly one field, so
    # "exactly one problem, naming the right thing" is the whole assertion.
    _one_problem(
        (_GOOD_PIN + _GOOD_KEY).replace(
            'upstream = "https://example.invalid/rosdistro"', 'upstream = ""'
        ),
        "upstream",
    )
    _one_problem(
        (_GOOD_PIN + _GOOD_KEY).replace(
            'ref = "0123456789abcdef0123456789abcdef01234567"', 'ref = "abc"'
        ),
        "40-hex",
    )
    _one_problem(
        _GOOD_HEADER
        + "[snapshot]\n"
        + 'upstream = "https://example.invalid/rosdistro"\n'
        + 'ref = "0123456789abcdef0123456789abcdef01234567"\n'
        + 'generator = "scripts/gen-rosdep-snapshot.py"\n'
        + _GOOD_KEY,
        "source file",
    )
    _one_problem(
        (_GOOD_PIN + _GOOD_KEY).replace('sha256 = "%s"' % ("0" * 64), 'sha256 = "nope"'),
        "sha256",
    )
    _one_problem(
        (_GOOD_PIN + _GOOD_KEY).replace(
            'generator = "scripts/gen-rosdep-snapshot.py"',
            'generator = "scripts/does-not-exist.py"',
        ),
        "generator",
    )
    _one_problem((_GOOD_PIN + _GOOD_KEY).replace(_GOOD_HEADER, ""), "first line")
    _one_problem(
        _GOOD_PIN + '[key.b]\ndnf = ["b"]\n[key.a]\napt = ["a"]\n',
        "not sorted",
    )
    # A non-manager field ALSO trips the manager-order check (it sees `check`
    # as an out-of-place "manager"), so this case is legitimately two
    # problems, not one — asserting exactly-one here would be the wrong shape
    # of test, not a looser one.
    extra_field_text = _GOOD_PIN + '[key.a]\napt = ["a"]\ncheck = "x"\n'
    extra_field_probs = find_data_problems(extra_field_text, loads(extra_field_text))
    assert len(extra_field_probs) == 2, extra_field_probs
    assert any("carries" in p for p in extra_field_probs), extra_field_probs
    _one_problem(_GOOD_PIN + "[key.ghost]\n", "no package manager")
    _one_problem(
        _GOOD_PIN + '[key.a]\ndnf = ["a"]\napt = ["a"]\n',
        "lists managers as",
    )

    sys.stdout.write("check-rosdep-snapshot self-test: OK\n")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return 0
    self_test()

    problems = []

    if not os.path.isfile(SNAPSHOT):
        sys.stderr.write(
            "error: %s is missing.\n"
            "Generate it: python3 scripts/gen-rosdep-snapshot.py --ref <rosdistro-sha>\n"
            % os.path.relpath(SNAPSHOT, ROOT)
        )
        return 1

    with open(SNAPSHOT, encoding="utf8") as fh:
        text = fh.read()
    doc = load(SNAPSHOT)

    # A block shape change that makes `key_order` see nothing would make every
    # check below vacuously true — this must fail LOUD, before any of them run.
    if not key_order(text):
        sys.stderr.write(
            "error: parsed ZERO [key.*] entries. The block shape changed; this "
            "gate would pass vacuously.\n"
        )
        return 1

    # 1-3 — the pin, normalization, and unprobed-ness. See find_data_problems.
    problems.extend(find_data_problems(text, doc))
    keys = doc.get("key", {})

    # 4 — the marking is reached by every consumer.
    orphans = check_consumers(rust_files())
    for rel in orphans:
        problems.append(
            "%s resolves through the rosdep snapshot without naming "
            "`RosdepSnapshot::%s`. A key from the snapshot is UNPROBED; a "
            "consumer that does not say so reports a vendored guess as if this "
            "tree had verified it." % (rel, MARKER)
        )

    if problems:
        sys.stderr.write("check-rosdep-snapshot: %d problem(s)\n\n" % len(problems))
        for p in problems:
            sys.stderr.write("  - %s\n\n" % p)
        sys.stderr.write(
            "  Regenerate:  python3 scripts/gen-rosdep-snapshot.py [--ref <sha>]\n"
            "  Verify:      python3 scripts/gen-rosdep-snapshot.py --verify  (needs network)\n"
        )
        return 1

    # Informational: how much of the snapshot the authored index already
    # answers for. Those keys resolve on the `[prereq.*]` rung WITH a probe;
    # the ladder order is what keeps it that way (unit-tested in
    # `prereq_resolve`), not this gate.
    shadowed = 0
    try:
        with open(INDEX, encoding="utf8") as fh:
            prereq = set(re.findall(r"^\[prereq\.([A-Za-z0-9._+-]+)\]", fh.read(), re.M))
        shadowed = len(prereq & set(keys))
    except OSError:
        pass

    ref = doc.get("snapshot", {}).get("ref", "")
    files = doc.get("snapshot", {}).get("file", [])
    sys.stdout.write(
        "check-rosdep-snapshot: OK — %d key(s) pinned at rosdistro %s, %d input "
        "file(s); %d also declared as [prereq.*] and resolved there instead.\n"
        % (len(keys), ref[:12], len(files), shadowed)
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
