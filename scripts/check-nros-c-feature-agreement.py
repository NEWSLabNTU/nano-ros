#!/usr/bin/env python3
"""The C and C++ roads must resolve `nros-c` to ONE feature set.

Issue 1461, and the class issues 1100 / 1156 / 1354 are all instances of.

WHY THIS IS A RULE

Every road that builds a mixed C/C++ image runs cargo TWICE into ONE
`--target-dir`: once `--package nros-c`, once `--package nros-cpp` (whose
manifest forwards `nros-c/<feature>`). Cargo keys a unit by its feature SET, so
if the two invocations ask for different sets, the directory gets two `nros-c`
units — and both write the same build-script byproduct:

    <target-dir>/nros-c-generated/nros/nros_config_generated.h

Nothing in cargo arbitrates that path. What it costs has been measured three
times, and the three costs look nothing alike:

  * issue 1100 — the sets differed by `alloc` + `param-services`, so the
    probed SIZES differed, and the second build in any directory died on
    `write_header_if_absent_or_verify`'s divergence panic. Fatal, loud.
  * issue 1354 — `check cpp`'s own lanes, `NROS_CPP_CONFIG_VARIANT` mismatched
    against the archive: an undefined reference at link, which reads as a race.
  * issue 1461 — the sets differed only by a back-compat ALIAS spelling
    (`cffi-zenoh-cffi` for `rmw-zenoh`), so every probed size agreed and the
    two headers differed in exactly one line. Nothing failed; the file's mtime
    just moved on every build, it is a `cargo:rerun-if-changed` input and is
    listed in the staticlib's dep-info, and the fixture staleness probe read
    that as an edited source. Three `native_example_pubsub_e2e` coordinates
    returned a STALE message instead of a runtime result for EIGHTEEN DAYS.

The third is the one a gate is for: it is invisible in every build log.

WHAT IS CHECKED

A. Each backend descriptor's `[rmw.link] c_cffi_feature` must select the same
   `nros-c` features as the `nros-cpp` half (`<cargo_feature>-cffi`) forwards.
   Derived from the two manifests — nothing here enumerates a feature.

B. No in-tree build glue may name an `nros-c` back-compat ALIAS (a feature
   whose whole definition is one other `nros-c` feature). The aliases exist so
   out-of-tree glue that still passes the old name does not error; naming one
   in-tree is exactly the set split rule A forbids, one road over.

Run: check-nros-c-feature-agreement.py [--self-test]
"""

import glob
import os
import re
import subprocess
import sys

try:
    import tomllib  # 3.11+
except ModuleNotFoundError:  # 3.10 backport, same spelling as the sibling gates
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

C_MANIFEST = "packages/api/nros-c/Cargo.toml"
CPP_MANIFEST = "packages/api/nros-cpp/Cargo.toml"

# Files the alias scan must not read: the manifest that DEFINES the aliases,
# and this gate, whose docstring and self-test fixtures necessarily spell them.
# Measured rather than foreseen — the first `just check nros-c-feature-agreement`
# after committing the script reported five hits, all of them in the script,
# because until the commit `git ls-files` had not listed it.
SCAN_EXEMPT = {C_MANIFEST, "scripts/check-nros-c-feature-agreement.py"}

# `default` is a single-entry feature and is NOT an alias in the sense that
# matters: nobody passes it on a `--features` line, and `--no-default-features`
# is what every road here uses.
NOT_AN_ALIAS = {"default"}

# Where build glue lives. A file that names an alias in a COMMENT is fine — the
# comments explaining this very rule name them — so full-line comments are
# stripped before the scan.
#
# Roots plus a suffix filter, NOT pathspec globs. `zephyr/**/*.txt` matches
# NOTHING for `zephyr/CMakeLists.txt` — a file one level down, and the single
# most important file this gate reads — and a glob that silently matches
# nothing narrows a gate toward OK, which is issue 0196's shape. Measured: the
# first draft of this gate reported the two descriptors and stayed silent about
# three live alias uses in `zephyr/CMakeLists.txt`.
GLUE_ROOTS = ["cmake", "zephyr", "integrations", "packages", "just", "scripts", "examples"]
GLUE_SUFFIXES = (
    ".cmake",
    ".just",
    ".sh",
    ".py",
    ".toml",
    "CMakeLists.txt",
    "Makefile",
    "Make.defs",
)

# Roads that build `nros-c` and `nros-cpp` into ONE target dir. Their absence
# from the scanned set makes this gate quieter without making it look wrong, so
# it is a failure rather than a smaller report.
REACH_MUST_HOLD = [
    "zephyr/CMakeLists.txt",
    "integrations/nuttx/Makefile",
    "cmake/NanoRosFeatureSet.cmake",
    "packages/rmw/zenoh/nros-rmw-zenoh/nros-rmw.toml",
]


def features_of(manifest_rel):
    with open(os.path.join(ROOT, manifest_rel), "rb") as fh:
        return tomllib.load(fh).get("features", {})


def closure_in(features, seed):
    """The `nros-c` features a `--features` request resolves to, within one crate.

    Only BARE entries name a feature of this crate. `nros/std`, `dep:x` and
    `x?/y` reach other packages and cannot change this crate's own set.
    """
    out, todo = set(), list(seed)
    while todo:
        f = todo.pop()
        if f in out or f not in features:
            continue
        out.add(f)
        for entry in features[f]:
            if "/" not in entry and not entry.startswith("dep:"):
                todo.append(entry)
    return out


def forwarded_to_c(cpp_features, seed):
    """The `nros-c` features `nros-cpp` forwards for a `--features` request."""
    seen, todo, fwd = set(), list(seed), set()
    while todo:
        f = todo.pop()
        if f in seen or f not in cpp_features:
            continue
        seen.add(f)
        for entry in cpp_features[f]:
            if entry.startswith("nros-c/"):
                fwd.add(entry.split("/", 1)[1])
            elif "/" not in entry and not entry.startswith("dep:"):
                todo.append(entry)
    return fwd


def all_forwards(cpp_features):
    """Every `nros-c` feature ANY `nros-cpp` feature forwards."""
    out = set()
    for entries in cpp_features.values():
        for entry in entries:
            if entry.startswith("nros-c/"):
                out.add(entry.split("/", 1)[1])
    return out


def aliases_of(c_features, cpp_features):
    """The `nros-c` back-compat aliases, DERIVED — never a list kept here.

    An alias is a feature of `nros-c` whose only effect is to enable one other
    feature that `nros-cpp` ALSO names, while `nros-cpp` cannot name the alias
    itself. So the two roads have a spelling each for one selection, and only
    one of the two spellings can make their feature sets equal.

    Both halves of that are load-bearing. Without the "one other feature"
    half, every forwarding feature qualifies. Without the "nros-cpp names the
    target" half, `rmw-cyclonedds = ["rmw-cffi"]` reads as an alias for
    `rmw-cffi` — it is not: `nros-cpp` forwards `nros-c/rmw-cyclonedds`, so it
    is the name both roads already agree on, and banning it would ban the
    agreement.
    """
    forwarded = all_forwards(cpp_features)
    out = {}
    for name, entries in c_features.items():
        if name in NOT_AN_ALIAS or len(entries) != 1 or name in forwarded:
            continue
        only = entries[0]
        if "/" in only or only.startswith("dep:") or only not in c_features:
            continue
        if only not in forwarded:
            continue
        out[name] = only
    return out


def check_descriptors(c_features, cpp_features):
    """Rule A."""
    fails = []
    paths = sorted(glob.glob(os.path.join(ROOT, "packages/rmw/*/*/nros-rmw.toml")))
    if not paths:
        return ["no nros-rmw.toml found — refusing to pass on an empty set"]
    checked = 0
    for path in paths:
        with open(path, "rb") as fh:
            data = tomllib.load(fh)
        c_feat = data.get("rmw", {}).get("link", {}).get("c_cffi_feature", "")
        if not c_feat:
            continue  # not bundled into the umbrella — correct for cyclone/uorb
        rel = os.path.relpath(path, ROOT)
        name = rel.split(os.sep)[2]  # packages/rmw/<name>/<crate>/nros-rmw.toml
        cpp_feat = f"rmw-{name}-cffi"
        if c_feat not in c_features:
            fails.append(f"{rel}: c_cffi_feature = {c_feat!r} is not a feature of nros-c")
            continue
        if cpp_feat not in cpp_features:
            fails.append(
                f"{rel}: declares c_cffi_feature = {c_feat!r} but nros-cpp has no "
                f"{cpp_feat!r} to agree with"
            )
            continue
        checked += 1
        want = closure_in(c_features, forwarded_to_c(cpp_features, [cpp_feat]))
        got = closure_in(c_features, [c_feat])
        if got != want:
            only_c = ", ".join(sorted(got - want)) or "-"
            only_cpp = ", ".join(sorted(want - got)) or "-"
            fails.append(
                f"{rel}: the C and C++ roads resolve nros-c differently for {name!r}.\n"
                f"    `--package nros-c --features {c_feat}` gives: {sorted(got)}\n"
                f"    `--package nros-cpp --features {cpp_feat}` forwards: {sorted(want)}\n"
                f"    only on the C side: {only_c}\n"
                f"    only via nros-cpp:  {only_cpp}\n"
                f"    Both build into ONE --target-dir, so this is two nros-c units\n"
                f"    writing one nros-c-generated/nros/nros_config_generated.h\n"
                f"    (issues 1461, 1100). Name the feature nros-cpp forwards."
            )
    if checked == 0:
        fails.append("no descriptor declares c_cffi_feature — the rule checked nothing")
    return fails


def strip_full_line_comments(text):
    """Blank out full-line comments, keeping the LINE NUMBERING.

    Dropping them instead would report a line number that does not exist in the
    file the reader opens — measured at "line 138" for `zephyr/CMakeLists.txt`'s
    line 341, which is a report that costs more than it saves.
    """
    return "\n".join("" if line.lstrip().startswith("#") else line for line in text.splitlines())


def tracked_glue_files():
    try:
        listed = subprocess.run(
            ["git", "-C", ROOT, "ls-files", "-z", "--", *GLUE_ROOTS],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
    except (OSError, subprocess.CalledProcessError):
        return None
    return [p for p in listed.split("\0") if p and p.endswith(GLUE_SUFFIXES)]


def check_glue(c_features, cpp_features, files=None, read=None, reach_check=True):
    """Rule B."""
    aliases = aliases_of(c_features, cpp_features)
    if not aliases:
        return ["nros-c declares no back-compat aliases — the alias scan checked nothing"]
    if files is None:
        files = tracked_glue_files()
        if files is None:
            return ["`git ls-files` failed — refusing to report OK over an unknown file set"]
    if not files:
        return ["no build-glue files matched — refusing to report OK over an empty set"]
    missing = [p for p in REACH_MUST_HOLD if p not in files] if reach_check else []
    if missing:
        return [
            "the scanned file set does not reach "
            + ", ".join(missing)
            + " — each is a road that builds nros-c and nros-cpp into one target dir, "
            "so a set without it is a gate that got quieter, not a tree that got cleaner"
        ]
    read = read or (lambda p: open(os.path.join(ROOT, p), encoding="utf-8").read())
    patterns = {a: re.compile(rf"(?<![\w-]){re.escape(a)}(?![\w-])") for a in aliases}
    fails = []
    for rel in files:
        if rel in SCAN_EXEMPT:
            continue
        try:
            body = strip_full_line_comments(read(rel))
        except (OSError, UnicodeDecodeError):
            continue
        for alias, live in aliases.items():
            for n, line in enumerate(body.splitlines(), 1):
                if patterns[alias].search(line):
                    fails.append(
                        f"{rel}: names the nros-c back-compat alias {alias!r}; write "
                        f"{live!r}, the feature nros-cpp forwards.\n"
                        f"    The alias resolves to the same thing, but cargo keys a "
                        f"unit by its feature SET,\n"
                        f"    so it splits nros-c in two inside one --target-dir "
                        f"(issues 1461, 1100).\n"
                        f"    line {n}: {line.strip()}"
                    )
    return fails


def self_test():
    fails = 0

    def ok(msg):
        print(f"  ok   {msg}")

    def bad(msg, detail):
        nonlocal fails
        print(f"  FAIL {msg}: {detail}", file=sys.stderr)
        fails += 1

    c = {
        "rmw-cffi": ["nros/rmw-cffi"],
        "rmw-zenoh": ["rmw-cffi", "dep:nros-rmw-zenoh"],
        "cffi-zenoh-cffi": ["rmw-zenoh"],
        "default": ["rmw-cffi"],
    }
    cpp = {
        "rmw-cffi": ["nros/rmw-cffi", "nros-c/rmw-cffi"],
        "rmw-zenoh-cffi": ["rmw-cffi", "nros-c/rmw-zenoh", "dep:nros-rmw-zenoh"],
    }

    # The alias derivation: `default` is excluded, `rmw-cffi` reaches another
    # crate, so exactly one alias remains.
    got = aliases_of(c, cpp)
    if got == {"cffi-zenoh-cffi": "rmw-zenoh"}:
        ok("an alias is a feature whose whole definition is one own feature")
    else:
        bad("alias derivation", got)

    # Rule A, both directions, on the exact divergence issue 1461 measured.
    want = closure_in(c, forwarded_to_c(cpp, ["rmw-zenoh-cffi"]))
    if closure_in(c, ["rmw-zenoh"]) == want:
        ok("the live feature name agrees with what nros-cpp forwards")
    else:
        bad("aligned spelling", "reported a difference")
    if closure_in(c, ["cffi-zenoh-cffi"]) != want:
        ok("the ALIAS spelling does not — which is the defect (issue 1461)")
    else:
        bad("alias spelling", "was called equal to the live one")

    # Rule B's negative control: the scan must find an alias in glue, and must
    # NOT find one in a comment explaining the rule.
    files = ["fake/glue.cmake", "fake/doc.cmake"]
    bodies = {
        "fake/glue.cmake": 'set(_f "rmw-cffi,cffi-zenoh-cffi,platform-posix")\n',
        "fake/doc.cmake": "# never the `cffi-zenoh-cffi` back-compat alias\nset(_f rmw-zenoh)\n",
    }
    found = check_glue(c, cpp, files=files, read=lambda p: bodies[p], reach_check=False)
    if len(found) == 1 and found[0].startswith("fake/glue.cmake"):
        ok("the scan finds an alias in glue and ignores one in a comment")
    else:
        bad("glue scan", found)

    # And it must refuse to pass over nothing, both ways.
    if check_glue(c, cpp, files=[], read=lambda p: "", reach_check=False) and check_glue(
        {}, {}, files=files, reach_check=False
    ):
        ok("an empty file set and an empty alias set both REFUSE")
    else:
        bad("emptiness refusal", "reported OK over nothing")

    if fails:
        return 1
    print("check-nros-c-feature-agreement self-test OK")
    return 0


def main():
    if "--self-test" in sys.argv:
        return self_test()
    if self_test() != 0:
        sys.stderr.write("check-nros-c-feature-agreement: SELF-TEST FAILED\n")
        return 1
    c_features = features_of(C_MANIFEST)
    cpp_features = features_of(CPP_MANIFEST)
    fails = check_descriptors(c_features, cpp_features) + check_glue(c_features, cpp_features)
    if fails:
        sys.stderr.write("check-nros-c-feature-agreement: FAILED\n\n")
        for f in fails:
            sys.stderr.write(f"  {f}\n\n")
        return 1
    print("check-nros-c-feature-agreement: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
