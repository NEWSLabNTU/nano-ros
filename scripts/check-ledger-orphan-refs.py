#!/usr/bin/env python3
"""A ledger row may not cite a file that does not exist.

phase-428 W6. The ledger validates existence in ONE direction: `--check` fails
on an unledgered difference, and nothing ever asks whether a row still has a
subject. Measured consequences at the time this was written:

* **43 rows cite `rclcpp_compat.hpp`**, deleted in stage 6 step A;
* six `rust:nros_*` log rows are `blocked-needs-decision` on a decision made
  2026-09-04, describing forwarders removed in phase-417 W-B5;
* `rust:init` is a mis-filed row whose prose is about `nros::logging::init`,
  so `nros::init()` — which HAS an rclrs counterpart — reads as having none.

That is the campaign's own generalisation ("nothing validates a REASON") one
layer up, at the ledger. This gate closes the cheapest, most mechanical half of
it: a row whose reason names a file can be checked against the tree.

WHAT IS CHECKED, two spellings of a citation:

* `path/to/file.ext` — a repo-relative path (`packages/`, `docs/`, ...) must
  exist on disk. A directory-qualified path under any OTHER root (`rclcpp/
  qos.hpp`, `rmw/rmw.h`, `rclrs/src/context.rs`) is an UPSTREAM include path
  and is out of scope. That is the convention this gate relies on: cite an
  upstream file WITH its directory, and a bare name is always ours.
* `nros-crate/src/file.rs` — a CRATE-relative path (issue 1545). Its first
  segment is a hyphenated package name of ours, or any `nros-*` name, so it is ours:
  the file must exist under that package's directory, found by its
  `Cargo.toml` `name` (not the directory name — `nros-rmw-cffi` lives at
  `packages/rmw/cffi`). An `nros-*` segment no package carries is a deleted
  crate, and is reported too. The first version read every such path as
  upstream, so two dead ones sat green.
* `file.ext` — a bare basename must match at least one tracked file of that
  name (`git ls-files`), UNLESS the same sentence says the file is gone. The
  allow-list is exactly three words, `deleted` / `removed` / `retired`, and
  the sentence is the scope: "`x.hpp` holds it. The shim was deleted." is
  still an orphan, because the first sentence claims the file is there. The
  first version matched the path spelling only, so the 42 rows that named
  `rclcpp_compat.hpp` by bare name stayed green until a reader found them
  (PR #634 fixed the rows; the gate learned the spelling afterwards).

Both spellings may carry a line (`init.rs:126`) or a range (`qos.hpp:40-52`);
the suffix is stripped, not checked. A phase work-item label (`W4.c`, `Q1.b`)
has the shape of a bare name and is not one: a stem of one or two letters and
digits is skipped.

WHAT IS NOT: whether the LINE number is still right, whether the reason is
still true, or whether the row's subject still exists as a symbol. Those need
the extractor, not a filesystem stat — stated so a green here is not read as
more than it is.
"""
import json
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
LEDGER = ROOT / "docs/reference/api-parity-ledger"
EXTS = ("hpp", "h", "rs", "py", "cpp", "c", "toml", "just", "md", "json", "jinja", "sh")
REPO_ROOTS = ("packages/", "scripts/", "docs/", "just/", "cmake/", "examples/", "book/")

# One citation: an optional directory prefix, a basename with one of EXTS, and
# an optional `:line` / `:from-to` suffix. The look-behind keeps the match from
# starting inside a longer path or a dotted token; the look-ahead keeps it from
# ending inside one.
CITE_RE = re.compile(
    r"(?<![\w/.\-])((?:[\w.\-]+/)*)([\w\-]+(?:\.[\w\-]+)*\.(?:%s))(?::\d+(?:-\d+)?)?(?![\w/])"
    % "|".join(EXTS)
)
# `W4.c`, `Q1.b`, `F2.md`: a work-item label, not a file.
# Include roots of the upstream ROS 2 headers the ledger compares against —
# a citation under one names THEIR file, which this tree does not carry.
UPSTREAM_ROOTS = ("rclrs", "rclcpp", "rcl", "rmw", "rcutils", "rclcpp_action", "rcl_action",
                  "rosidl_runtime_cpp", "rclc", "rclpy", "rcpputils",
                  "rmw_dds_common", "rclcpp_lifecycle", "rcl_lifecycle", "lifecycle_msgs",
                  "builtin_interfaces", "std_msgs", "action_msgs", "unique_identifier_msgs")
UPSTREAM_ROOTS_RE = re.compile(r"(^|/)(%s)/" % "|".join(UPSTREAM_ROOTS))
LABEL_RE = re.compile(r"^[A-Za-z]{1,2}\d+$")
# The allow-list. Small on purpose: a sentence that says a file is gone may
# name it; nothing else excuses a bare name that matches no tracked file.
GONE_WORDS = ("deleted", "removed", "retired")
GONE_RE = re.compile(r"\b(?:%s)\b" % "|".join(GONE_WORDS), re.IGNORECASE)
SENTENCE_RE = re.compile(r"(?<=[.!?])\s+|\n+")


def tracked_paths():
    out = subprocess.check_output(["git", "-C", str(ROOT), "ls-files", "-z"])
    return [p for p in out.decode().split("\0") if p]


def tracked_basenames():
    return {p.rsplit("/", 1)[-1] for p in tracked_paths()}


PKG_NAME_RE = re.compile(r'^\s*name\s*=\s*"([^"]+)"', re.M)


def crate_dirs():
    """{crate name: repo-relative dir} for every tracked OWN Cargo package.

    Issue 1545: a CRATE-RELATIVE citation (`nros-core/src/error.rs`) has a
    directory prefix, so the first version filed it as an upstream include
    path and never looked — and the crate name is not the directory name
    (`nros-rmw-cffi` lives at `packages/rmw/cffi`), so it cannot be joined to
    a repo root either. The package name is the only key that resolves it.
    """
    out = {}
    for p in tracked_paths():
        if not p.endswith("Cargo.toml") or "third-party/" in p:
            continue
        try:
            text = (ROOT / p).read_text(errors="replace")
        except OSError:
            continue
        pkg = text.split("[package]", 1)
        if len(pkg) != 2:
            continue
        m = PKG_NAME_RE.search(pkg[1].split("\n[", 1)[0])
        if m:
            out.setdefault(m.group(1), p.rsplit("/", 1)[0] if "/" in p else "")
    return out


def is_own_crate_segment(seg, crates):
    """Is `seg/...` a crate-relative path of OURS?

    An `nros-*` segment is ours even when no crate carries it any more -- that
    is exactly the deleted-crate citation this must report. Another package
    name counts only when it is hyphenated: the Rust facade crate is named
    `nros`, and `nros/parameter.h` is the public INCLUDE path (resolved
    against an include dir, not a crate root), cited ~90 times in the ledger.
    """
    return seg.startswith("nros-") or (seg in crates and "-" in seg)


def strings_of(value):
    """Every string in a row, nested objects (`rename`, `their_rename`) included."""
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for v in value.values():
            yield from strings_of(v)
    elif isinstance(value, list):
        for v in value:
            yield from strings_of(v)


def orphans_in(text, basenames, crates):
    """Yield (kind, citation) for every citation in `text` that resolves to nothing."""
    for sentence in SENTENCE_RE.split(text):
        for m in CITE_RE.finditer(sentence):
            prefix, name = m.group(1), m.group(2)
            if prefix:
                path = prefix + name
                if path.startswith(REPO_ROOTS):
                    if not (ROOT / path).exists():
                        yield ("path", path)
                    continue
                seg, _, rest = path.partition("/")
                if is_own_crate_segment(seg, crates):
                    d = crates.get(seg)
                    if d is None or not (ROOT / d / rest).exists():
                        yield ("crate", path)
                    continue
                # issue 1616 (W7): a prefix that is neither a repo root nor one
                # of our crates is not thereby UPSTREAM. `src/x.rs` was skipped
                # as if it were an include path. Fall through to the bare check:
                # the file must exist SOMEWHERE in the tree, or be named gone.
                if name in basenames or GONE_RE.search(sentence) or UPSTREAM_ROOTS_RE.search(prefix):
                    continue
                yield ("prefixed", path)
                continue
            if LABEL_RE.match(name.rsplit(".", 1)[0]):
                continue
            if name in basenames or GONE_RE.search(sentence):
                continue
            yield ("bare", name)


def scan(files, basenames, crates):
    bad = []
    for f in files:
        rows = json.loads(f.read_text())
        for key, val in rows.items():
            if not isinstance(val, dict):
                continue
            blob = " ".join(strings_of(val))
            for kind, cite in orphans_in(blob, basenames, crates):
                bad.append((f.name, key, kind, cite))
    return bad


def self_test(basenames, crates):
    """Negative controls, on the NORMAL path.

    `check-gate-selftests` requires this and it is right to: a control nobody
    runs decays into a comment. It also matters specifically here — the
    citation regex is the whole gate, and a regex that quietly stops matching
    would leave a permanently green check over a growing pile of dead
    references. Each control plants one spelling the gate must catch and one
    it must not; the bare-name controls are the exact shape the first version
    missed.
    """
    import tempfile

    dead = "rclcpp_compat.hpp"
    assert dead not in basenames, "self-test: %s came back; pick a different planted orphan" % dead
    assert "qos.hpp" in basenames and "init.rs" in basenames, "self-test: the live controls moved"
    assert crates.get("nros-rmw-cffi") == "packages/rmw/cffi", "self-test: the crate-relative control moved"
    assert "nros-gone-crate" not in crates, "self-test: the planted dead crate exists"

    d = pathlib.Path(tempfile.mkdtemp())
    (d / "planted.json").write_text(
        json.dumps(
            {
                "cpp:real": {"why": "see packages/api/nros-cpp/include/nros/qos.hpp for the table"},
                "cpp:orphan": {"why": "see packages/api/nros-cpp/include/nros/%s:428" % dead},
                "cpp:upstream": {"why": "upstream rclcpp/qos.hpp says otherwise"},
                "cpp:bare-orphan": {"why": "`%s:117` ships `ParametersQoS()` at `QoS(10)`." % dead},
                "cpp:bare-range": {"why": "the setters at `%s:120-131` store and never read." % dead},
                "cpp:bare-gone": {"why": "then `%s`, which was deleted at stage 6 step A." % dead},
                "cpp:bare-live": {"why": "`qos.hpp:40` mirrors `init.rs:126-140` exactly."},
                "cpp:label": {"why": "phase-417 W4.c added it; Q1.b measured it."},
                "cpp:nested": {"why": "x", "rename": {"resolution": "was in `%s`." % dead}},
                # Issue 1545: crate-relative, keyed by PACKAGE name, not directory.
                "cpp:crate-live": {"why": "regenerates nros-rmw-cffi/src/generated.rs."},
                "cpp:crate-orphan": {"why": "we had it in nros-core/src/error.rs then."},
                "cpp:crate-gone-crate": {"why": "see nros-gone-crate/src/lib.rs for it."},
                "cpp:prefixed-orphan": {"why": "see `src/rerun_missing_x.rs` -- it holds it."},
                "cpp:prefixed-upstream": {"why": "upstream rclrs/src/never_here.rs does it."},
                "cpp:other-sentence": {
                    "why": "`%s` still holds the profile. The shim was deleted." % dead
                },
            }
        )
    )
    hits = scan([d / "planted.json"], basenames, crates)
    caught = {k for _, k, _, _ in hits}
    want = {
        "cpp:orphan", "cpp:bare-orphan", "cpp:bare-range", "cpp:nested", "cpp:other-sentence",
        "cpp:crate-orphan", "cpp:crate-gone-crate", "cpp:prefixed-orphan",
    }
    missed = want - caught
    assert not missed, "self-test: planted dead reference(s) not caught: %s" % sorted(missed)
    spurious = caught - want
    assert not spurious, "self-test: a live or out-of-scope citation was flagged: %s" % sorted(
        spurious
    )
    kinds = {k: kind for _, k, kind, _ in hits}
    assert kinds["cpp:orphan"] == "path" and kinds["cpp:bare-orphan"] == "bare", kinds
    assert kinds["cpp:crate-orphan"] == "crate", kinds
    print("check-ledger-orphan-refs --self-test: %d case(s) OK" % (len(want) + 6))


def main():
    basenames = tracked_basenames()
    crates = crate_dirs()
    self_test(basenames, crates)

    files = sorted(LEDGER.glob("*.json"))
    if not files:
        print("check-ledger-orphan-refs: no ledger shards found at %s" % LEDGER, file=sys.stderr)
        return 1
    bad = scan(files, basenames, crates)
    if bad:
        print(
            "FAIL: %d ledger row(s) cite a file that does not exist.\n"
            "      A reason naming a deleted file is a reason nobody can check.\n"
            "      `path`: a repo-relative path with no file behind it.\n"
            "      `crate`: a crate-relative path (`nros-core/src/x.rs`) whose crate or file is gone.\n"
            "      `bare`: a basename no tracked file carries, in a sentence that does not\n"
            "      say it was %s. An UPSTREAM file is cited with its directory\n"
            "      (`rmw/rmw.h`), which puts it out of scope.\n"
            % (len(bad), " / ".join(GONE_WORDS)),
            file=sys.stderr,
        )
        for shard, key, kind, cite in bad[:40]:
            print("  %-16s %-44s %-4s -> %s" % (shard, key, kind, cite), file=sys.stderr)
        if len(bad) > 40:
            print("  … and %d more" % (len(bad) - 40), file=sys.stderr)
        return 1
    print(
        "check-ledger-orphan-refs: OK — %d shard(s), every cited repo path and bare "
        "filename resolves" % len(files)
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
