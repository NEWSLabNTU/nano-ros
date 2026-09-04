#!/usr/bin/env python3
"""A fetch without a digest is an unpinned submodule wearing CMake syntax.

RFC-0087 D5 (phase-420 W8). A **vendor package** is not a kind: it is an
ordinary package whose build fetches and builds an external source tree, exactly
as `zenoh_cpp_vendor` is in ROS 2. Nothing marks it, and nothing needs to —
putting it in the source tree is the user's responsibility, which is colcon's
contract. What D5 *does* require is the one thing the ecosystem cannot supply:

    Every `FetchContent_Declare` / `ExternalProject_Add` in a discovered package
    carries a digest, and any build script that downloads verifies one.

WHY THIS RULE HAS THE SAME REASONING AS `check-submodule-pins`, NOT A NEW ONE
----------------------------------------------------------------------------
That gate's own header states the problem it solves: `-Subproject commit
d3f0d26 / +Subproject commit 43ddb0e` is two hex strings, and *which one is
newer is not visible without asking the submodule* — so a pin can move
BACKWARDS in a large commit and nobody reading the diff can see it. Its answer
is that the pointer is a full commit id, so the question "which tree is this,
exactly" always has an answer, and the gate can go and ask it.

A fetch is the same pointer with the enforcement removed. `GIT_TAG v0.6.1`
names a ref on someone else's server; the ref is *theirs* to move, and if it
moves, every build here silently switches trees with no diff at all — not a
backward pin you could at least detect after the fact, but a change with no
local representation whatsoever. `URL` with no hash is worse still: the bytes
behind a URL are not even nominally immutable.

So this gate does not invent a second rationale. It asserts the property
`check-submodule-pins` already relies on — **an external tree is named by a
digest of its contents, never by a name someone else controls** — for the one
place in the build where git is not doing it for us. The two gates partition the
surface: a submodule's pin is a gitlink and lives under that gate; a fetch's pin
is an argument in a CMake call and lives under this one. W9 moves trees between
the two mechanisms, so the pair must agree on what a pin *is*, and they do.

WHICH DIGEST FORMS COUNT, AND WHY
---------------------------------
ACCEPTED — each names the bytes, not a label someone else can re-point:

  * `URL_HASH <ALGO>=<hex>` with ALGO in {SHA256, SHA384, SHA512, SHA3_256,
    SHA3_384, SHA3_512}. A content digest, verified by CMake before unpacking.
  * `GIT_TAG <full 40-hex or 64-hex>` — a commit id. This is *exactly* what a
    submodule gitlink is, so accepting it keeps the two gates telling one story:
    the same pin in the same form, checked by whichever gate owns the mechanism.
    (64 hex is git's SHA-256 object format; no repo here uses it yet, and
    hardcoding 40 would make this gate the thing that breaks when one does.)
  * A declaration with no download at all (`SOURCE_DIR` alone, or an explicitly
    emptied `DOWNLOAD_COMMAND`). Nothing crosses the network, so there is
    nothing to pin.

REJECTED, and each rejection is a distinct failure the tree could actually hit:

  * `GIT_TAG` at a tag, a branch, `HEAD`, or a short sha. A tag is mutable by
    the remote; a branch is mutable by definition; a short sha is a PREFIX, and
    a prefix is resolved by the remote rather than by us — none of the three is
    an identity.
  * `GIT_TAG "${SOME_VAR}"` — not statically checkable, so it cannot be
    *asserted* to be a digest. A gate that accepted it would be reporting a
    property it never established, which is the failure mode this repo's gate
    docs keep naming. Put the literal in the declaration.
  * `GIT_REPOSITORY` with no `GIT_TAG` at all: CMake takes the remote's default
    branch, which is the least pinned thing available.
  * `URL` with no hash.
  * `URL_MD5`, and `URL_HASH MD5=`/`SHA1=`. These are digests in form only.
    Both are chosen-prefix forgeable, so they answer "did the bytes arrive
    intact" and not "are these the bytes I pinned" — and the second question is
    the one a supply chain asks.
  * `URL_SHA256` / `URL_SHA512` and friends. These are *not* CMake options —
    ExternalProject documents `URL_HASH` and `URL_MD5` only — so CMake accepts
    them as an unknown keyword and verifies nothing. A misspelling that looks
    more careful than the correct spelling is worth naming explicitly.
  * `SVN_`/`HG_`/`CVS_REPOSITORY` and a non-empty `DOWNLOAD_COMMAND`: a fetch
    with no digest slot at all. Reported as un-digestable rather than silently
    skipped, because "this gate has no opinion here" is itself the finding.

SCOPE, AND THE PART THAT IS DELIBERATELY WIDER THAN D5's SENTENCE
-----------------------------------------------------------------
D5 says "in a discovered package". Enforcement is exactly that: a file is
in-scope for RULE 1 and RULE 2 when some ancestor directory holds a
`package.xml` (the nearest one is the owning package).

But the repo-root `cmake/` modules are `include()`d BY those package builds — a
fetch there runs as part of every one of them. A gate whose coverage is narrower
than the rule it enforces is this repo's issue-0196 shape, and it is not
hypothetical here: measured on 2026-09-04, the tree contains **zero** fetches
inside any of its 407 discovered packages and **one** outside them, in
`cmake/NanoRosCorrosion.cmake` — then at a movable `GIT_TAG v0.6.1`, which is
issue 1060, since fixed. Scoped to D5's sentence alone this gate would have
reported OK on an empty set while the tree's only real fetch sat one directory
up.

So everything tracked is SCANNED and classified; out-of-package findings are
reported on every run and held by a BASELINE that may only shrink. That gives
the class coverage without turning a finding this wave does not own into a
day-one red. A red lane on day one has no signal capacity, which is the other
failure this repo has measured.

VACUITY
-------
An empty subject set is REPORTED, never silently passed. `NOTHING TO CHECK` is
printed with the population it searched, so "the rule holds" and "the rule has
no subject yet" are different lines of output rather than the same green.

SELFTEST
--------
`_selftest()` runs on the normal path, not behind a flag (phase-395,
`check-gate-selftests`): a negative control nobody runs decays into a comment.
It drives the real classifiers over synthetic inputs in both directions, so a
rule that stops being able to fail takes this gate down with it.

Buildless: `git ls-files` plus regexes. No cmake, no cargo, no network.

Usage::

    check-vendor-fetch-pinned.py            # the gate
    check-vendor-fetch-pinned.py --audit    # full classification, never fails
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Trees that are not ours. `git ls-files` already reduces a submodule to a
# single gitlink entry, so this is belt-and-braces for a tracked leftover.
EXCLUDED_PREFIXES = ("third-party/",)

CMAKE_SUFFIXES = ("CMakeLists.txt", ".cmake", ".cmake.in")
SCRIPT_SUFFIXES = ("build.rs", ".sh", ".py")

FETCH_CALL = re.compile(r"\b(FetchContent_Declare|ExternalProject_Add)\s*\(", re.I)

# Strong content digests. MD5/SHA1 are deliberately absent — see the header.
STRONG_ALGOS = {"SHA256", "SHA384", "SHA512", "SHA3_256", "SHA3_384", "SHA3_512"}
WEAK_ALGOS = {"MD5", "SHA1"}

# `URL_SHA256 <hex>` and friends: not CMake options, so CMake verifies nothing.
FAKE_HASH_KEYWORDS = re.compile(r"^URL_(?:SHA\d+|SHA3_\d+)$", re.I)

FULL_SHA = re.compile(r"^[0-9a-fA-F]{40}$|^[0-9a-fA-F]{64}$")

# Reaching the network from a build script.
DOWNLOAD_PRIMITIVES = re.compile(
    r"\b(?:reqwest|ureq|hyper::Client|curl\s+-|curl\s+\"|wget\s|"
    r"urllib\.request|urlretrieve|requests\.get|git\s+clone)\b"
)
# Evidence that the same file checks what it got.
DIGEST_EVIDENCE = re.compile(
    r"\b(?:sha256|sha512|sha3_256|sha3_512|sha256sum|sha512sum|URL_HASH)\b", re.I
)

# ---------------------------------------------------------------------------
# Known out-of-package fetches. A RATCHET: an entry that disappears from the
# tree must be deleted from here, and a fetch that is not listed fails.
#
# Keyed `<repo-relative path>::<declared name>` so moving the declaration inside
# its file does not churn this list, while moving it to another file does.
# ---------------------------------------------------------------------------
BASELINE = {
    "cmake/NanoRosCorrosion.cmake::Corrosion": (
        "NOT an unpinned fetch — a gate LIMIT. `GIT_TAG` is "
        "`${_nros_corrosion_commit}`, and a variable is something this gate "
        "cannot follow, so it cannot ESTABLISH the digest even though "
        "`_nros_corrosion_pin()` returns the literal commit "
        "`1499b14e4906a2890f5cee1547c8848db261753d`. Reporting a property it "
        "never established would be worse than not reporting one, so the entry "
        "stays. Issue 1060 — which this entry was filed against, when the tag "
        "`v0.6.1` really was the ref — is RESOLVED. What retires the entry is "
        "the commit id appearing literally in the declaration, which today "
        "would cost the FATAL_ERROR cross-check between the index and this "
        "file. Note this fetch is the FALLBACK — the supported path is the SDK "
        "store (`nros setup --tool corrosion`), whose `dist` assets are "
        "sha256-verified — so it is reached only when the store misses. Issue "
        "0500 explains why reading the configure's printed "
        "`Corrosion <ver> via <origin>` line is the only way to know which "
        "ran."
    ),
}


def tracked_files():
    """Tracked, repo-relative, with the not-ours trees dropped."""
    out = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return [
        p
        for p in out.split("\0")
        if p and not p.startswith(EXCLUDED_PREFIXES)
    ]


def package_dirs(paths):
    """Every directory holding a `package.xml` — RFC-0087 D1's one rule."""
    return {os.path.dirname(p) for p in paths if os.path.basename(p) == "package.xml"}


def owning_package(path, pkg_dirs):
    """The NEAREST enclosing package directory, or None if outside every one.

    Nearest rather than outermost because packages nest here (an example
    workspace holds node packages), and a fetch belongs to the package whose
    build actually runs it.
    """
    d = os.path.dirname(path)
    while True:
        if d in pkg_dirs:
            return d
        if not d:
            return None
        d = os.path.dirname(d)


def _strip_cmake_comments(text):
    """Blank out `#` comments, leaving offsets intact.

    Offsets must survive: line numbers are computed from the same string, and a
    comment that shifted them would misreport every finding after it. Quoted
    `#` is left alone — a URL fragment is not a comment.
    """
    out, i, n, in_string = [], 0, len(text), False
    while i < n:
        c = text[i]
        if in_string:
            out.append(c)
            if c == "\\" and i + 1 < n:
                out.append(text[i + 1])
                i += 2
                continue
            if c == '"':
                in_string = False
            i += 1
            continue
        if c == '"':
            in_string = True
            out.append(c)
            i += 1
            continue
        if c == "#":
            while i < n and text[i] != "\n":
                out.append(" ")
                i += 1
            continue
        out.append(c)
        i += 1
    return "".join(out)


def fetch_blocks(text):
    """Yield (call, name, args, offset) for every fetch declaration.

    Paren-balanced rather than line- or regex-scoped: every real declaration in
    the wild spans lines, and a `)` inside a quoted argument (a shell
    `DOWNLOAD_COMMAND`, say) must not close the call.
    """
    clean = _strip_cmake_comments(text)
    for m in FETCH_CALL.finditer(clean):
        i = m.end()
        depth, n, in_string = 1, len(clean), False
        while i < n and depth:
            c = clean[i]
            if in_string:
                if c == "\\":
                    i += 2
                    continue
                if c == '"':
                    in_string = False
            elif c == '"':
                in_string = True
            elif c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
                if depth == 0:
                    break
            i += 1
        body = clean[m.end():i]
        toks = _tokens(body)
        name = toks[0] if toks else "<unnamed>"
        yield m.group(1), name, toks[1:], m.start()


_TOKEN = re.compile(r'"((?:[^"\\]|\\.)*)"|(\S+)')


def _tokens(body):
    """CMake argument tokens, quotes removed.

    An EMPTY quoted argument is a real token and must survive: `DOWNLOAD_COMMAND
    ""` is how a declaration says "fetch nothing", and dropping it would turn a
    deliberate no-download into a bare `SOURCE_DIR` guess.
    """
    out = []
    for m in _TOKEN.finditer(body):
        out.append(m.group(1) if m.group(1) is not None else m.group(2))
    return out


def classify_fetch(call, args):
    """('ok' | 'violation' | 'no-download', reason).

    The verdict is about the DIGEST only. Whether the declaration is otherwise
    sane is cmake's problem.
    """
    kw = {}
    for i, tok in enumerate(args):
        if re.match(r"^[A-Z][A-Z0-9_]*$", tok) and tok not in kw:
            kw[tok] = args[i + 1] if i + 1 < len(args) else ""

    for k in args:
        if FAKE_HASH_KEYWORDS.match(k) and k.upper() != "URL_HASH":
            return (
                "violation",
                f"`{k}` is not a CMake option (ExternalProject documents "
                "`URL_HASH ALGO=value` and `URL_MD5` only), so CMake accepts it "
                "as an unknown keyword and verifies NOTHING. Write "
                "`URL_HASH SHA256=<hex>`.",
            )

    has_url = "URL" in kw
    has_git = "GIT_REPOSITORY" in kw
    other_vcs = [k for k in ("SVN_REPOSITORY", "HG_REPOSITORY", "CVS_REPOSITORY") if k in kw]
    dl_cmd = kw.get("DOWNLOAD_COMMAND", None)

    if dl_cmd is not None and dl_cmd.strip() == "":
        return "no-download", "DOWNLOAD_COMMAND is empty — nothing is fetched."
    if dl_cmd:
        return (
            "violation",
            "a bespoke DOWNLOAD_COMMAND has no digest slot CMake can verify. "
            "Use URL + URL_HASH, or GIT_TAG at a full commit id.",
        )
    if other_vcs:
        return (
            "violation",
            f"{other_vcs[0]} names a revision in a system with no content "
            "digest we can assert. Mirror the tree and fetch it by URL_HASH or "
            "by full commit id.",
        )

    if has_url:
        if "URL_MD5" in kw:
            return (
                "violation",
                "URL_MD5 is chosen-prefix forgeable: it answers 'did the bytes "
                "arrive intact', not 'are these the bytes I pinned'. Use "
                "`URL_HASH SHA256=<hex>`.",
            )
        raw = kw.get("URL_HASH", "")
        if not raw:
            return "violation", "URL with no URL_HASH — the bytes behind a URL are not immutable."
        if "=" not in raw:
            return "violation", f"URL_HASH `{raw}` is not `<ALGO>=<hex>`."
        algo, _, digest = raw.partition("=")
        algo = algo.strip().upper()
        if algo in WEAK_ALGOS:
            return (
                "violation",
                f"URL_HASH {algo} is a digest in form only — {algo} is "
                "chosen-prefix forgeable. Use one of "
                f"{', '.join(sorted(STRONG_ALGOS))}.",
            )
        if algo not in STRONG_ALGOS:
            return "violation", f"URL_HASH algorithm `{algo}` is not one this gate can vouch for."
        if not re.fullmatch(r"[0-9a-fA-F]{32,128}", digest.strip()):
            return "violation", f"URL_HASH {algo} value `{digest}` is not a hex digest."
        return "ok", f"URL_HASH {algo}"

    if has_git:
        tag = kw.get("GIT_TAG", "")
        if not tag:
            return (
                "violation",
                "GIT_REPOSITORY with no GIT_TAG: CMake takes the remote's "
                "default branch, which is the least pinned thing available.",
            )
        if "$" in tag:
            return (
                "violation",
                f"GIT_TAG `{tag}` is a variable, so this gate cannot ESTABLISH "
                "that it is a digest — and a gate that reports a property it "
                "never established is worse than no gate. Put the commit id in "
                "the declaration.",
            )
        if FULL_SHA.match(tag):
            return "ok", f"GIT_TAG {tag[:12]}… (full commit id)"
        if re.fullmatch(r"[0-9a-fA-F]{7,39}", tag):
            return (
                "violation",
                f"GIT_TAG `{tag}` is a SHORT sha — a prefix the REMOTE resolves, "
                "not an identity. Write the full commit id.",
            )
        return (
            "violation",
            f"GIT_TAG `{tag}` is a tag or branch name, which the remote owns and "
            "may re-point. That switches this build to another tree with no "
            "local diff at all — strictly worse than the backward submodule pin "
            "`check-submodule-pins` exists for, which at least leaves a hex "
            "string behind. Write the full commit id.",
        )

    return "no-download", "no URL and no repository — a local SOURCE_DIR declaration."


def classify_script(text):
    """('ok' | 'violation' | 'no-download', reason) for a build script."""
    hits = sorted({m.group(0).strip() for m in DOWNLOAD_PRIMITIVES.finditer(text)})
    if not hits:
        return "no-download", ""
    if DIGEST_EVIDENCE.search(text):
        return "ok", f"downloads ({', '.join(hits)}) and verifies a digest"
    return (
        "violation",
        f"downloads ({', '.join(hits)}) with no digest verified in the same "
        "file. RFC-0087 D5: any build script that downloads verifies one.",
    )


def _line_of(text, offset):
    return text.count("\n", 0, offset) + 1


# ---------------------------------------------------------------------------


def _selftest():
    """Drive the real classifiers, both directions. Runs on every invocation.

    Every case here is a shape the tree can actually grow, and each one asserts
    a DIFFERENT arm — a selftest that only proves 'a good input passes' would
    still pass if every rejection were deleted.
    """
    accept = [
        ("URL_HASH sha256", "FetchContent_Declare", ["URL", "https://x/a.tgz", "URL_HASH", "SHA256=" + "a" * 64]),
        ("URL_HASH sha512", "FetchContent_Declare", ["URL", "https://x/a.tgz", "URL_HASH", "SHA512=" + "b" * 128]),
        ("full sha-1 GIT_TAG", "FetchContent_Declare", ["GIT_REPOSITORY", "https://x/y.git", "GIT_TAG", "c" * 40]),
        ("full sha-256 GIT_TAG", "ExternalProject_Add", ["GIT_REPOSITORY", "https://x/y.git", "GIT_TAG", "d" * 64]),
    ]
    reject = [
        ("bare URL", ["URL", "https://x/a.tgz"]),
        ("URL_MD5", ["URL", "https://x/a.tgz", "URL_MD5", "e" * 32]),
        ("URL_HASH MD5", ["URL", "https://x/a.tgz", "URL_HASH", "MD5=" + "e" * 32]),
        ("URL_HASH SHA1", ["URL", "https://x/a.tgz", "URL_HASH", "SHA1=" + "e" * 40]),
        ("fake URL_SHA256 keyword", ["URL", "https://x/a.tgz", "URL_SHA256", "f" * 64]),
        ("tag GIT_TAG", ["GIT_REPOSITORY", "https://x/y.git", "GIT_TAG", "v0.6.1"]),
        ("branch GIT_TAG", ["GIT_REPOSITORY", "https://x/y.git", "GIT_TAG", "main"]),
        ("short sha GIT_TAG", ["GIT_REPOSITORY", "https://x/y.git", "GIT_TAG", "5621b26"]),
        ("variable GIT_TAG", ["GIT_REPOSITORY", "https://x/y.git", "GIT_TAG", "${PIN}"]),
        ("no GIT_TAG", ["GIT_REPOSITORY", "https://x/y.git"]),
        ("bespoke DOWNLOAD_COMMAND", ["DOWNLOAD_COMMAND", "curl -o x https://x/a"]),
        ("svn", ["SVN_REPOSITORY", "https://x/svn"]),
    ]
    skip = [
        ("local SOURCE_DIR", ["SOURCE_DIR", "/tmp/x"]),
        ("emptied DOWNLOAD_COMMAND", ["SOURCE_DIR", "/tmp/x", "DOWNLOAD_COMMAND", ""]),
    ]
    bad = []
    for label, call, args in accept:
        v, why = classify_fetch(call, args)
        if v != "ok":
            bad.append(f"accept case {label!r} classified {v} ({why})")
    for label, args in reject:
        v, why = classify_fetch("FetchContent_Declare", args)
        if v != "violation":
            bad.append(f"reject case {label!r} classified {v} ({why})")
    for label, args in skip:
        v, why = classify_fetch("FetchContent_Declare", args)
        if v != "no-download":
            bad.append(f"skip case {label!r} classified {v} ({why})")

    # The block extractor: multi-line, a `#` comment inside the call, a quoted
    # `)` that must not close it, and a `#` inside a quoted URL that is not a
    # comment.
    sample = (
        'FetchContent_Declare(demo\n'
        '  # a comment mentioning ) and URL_HASH\n'
        '  URL "https://example.invalid/a.tgz#frag"\n'
        '  URL_HASH SHA256=' + "9" * 64 + '\n'
        '  PATCH_COMMAND "sh -c \\"echo )\\""\n'
        ')\n'
        'FetchContent_Declare(second GIT_REPOSITORY https://x/y.git GIT_TAG v1)\n'
    )
    blocks = list(fetch_blocks(sample))
    if len(blocks) != 2:
        bad.append(f"block extractor found {len(blocks)} declarations, want 2")
    else:
        if blocks[0][1] != "demo" or classify_fetch(blocks[0][0], blocks[0][2])[0] != "ok":
            bad.append(f"block extractor mis-parsed the first declaration: {blocks[0]!r}")
        if blocks[1][1] != "second":
            bad.append(f"block extractor mis-parsed the second declaration: {blocks[1]!r}")
        if _line_of(sample, blocks[1][3]) != 7:
            bad.append(
                "comment stripping shifted line numbers: second declaration "
                f"reported at line {_line_of(sample, blocks[1][3])}, want 7"
            )

    # Package ownership: nearest enclosing package.xml, and None outside.
    pkgs = {"packages/a", "packages/a/nested"}
    for path, want in (
        ("packages/a/CMakeLists.txt", "packages/a"),
        ("packages/a/nested/CMakeLists.txt", "packages/a/nested"),
        ("packages/a/nested/deep/x.cmake", "packages/a/nested"),
        ("cmake/Other.cmake", None),
    ):
        got = owning_package(path, pkgs)
        if got != want:
            bad.append(f"owning_package({path!r}) = {got!r}, want {want!r}")

    # Build scripts.
    for label, text, want in (
        ("curl, no digest", 'curl -fsSL "$URL" -o x.tgz\n', "violation"),
        ("curl + sha256", 'curl -fsSL "$URL" -o x.tgz\nsha256sum -c x.sha256\n', "ok"),
        ("git clone, no digest", 'git clone --branch v1 https://x/y.git\n', "violation"),
        ("no network", 'cargo build\n', "no-download"),
    ):
        v, _ = classify_script(text)
        if v != want:
            bad.append(f"script case {label!r} classified {v}, want {want}")

    if bad:
        print("check-vendor-fetch-pinned SELFTEST FAILED:", file=sys.stderr)
        for b in bad:
            print(f"  - {b}", file=sys.stderr)
        print(
            "\n  The gate's own decision procedure is wrong, so any verdict it\n"
            "  prints about the tree is worthless. Fix the classifier.",
            file=sys.stderr,
        )
        sys.exit(2)


def main(argv):
    audit = "--audit" in argv
    _selftest()

    paths = tracked_files()
    pkg_dirs = package_dirs(paths)

    cmake_files = [p for p in paths if p.endswith(CMAKE_SUFFIXES)]
    script_files = [
        p
        for p in paths
        if p.endswith(SCRIPT_SUFFIXES) and owning_package(p, pkg_dirs) is not None
    ]

    in_pkg_fetches, out_pkg_fetches = [], []
    for rel in cmake_files:
        try:
            with open(os.path.join(ROOT, rel), encoding="utf8", errors="replace") as fh:
                text = fh.read()
        except OSError:
            continue
        if not FETCH_CALL.search(text):
            continue
        owner = owning_package(rel, pkg_dirs)
        for call, name, args, off in fetch_blocks(text):
            verdict, why = classify_fetch(call, args)
            row = (rel, _line_of(text, off), call, name, verdict, why, owner)
            (in_pkg_fetches if owner else out_pkg_fetches).append(row)

    script_rows = []
    for rel in script_files:
        try:
            with open(os.path.join(ROOT, rel), encoding="utf8", errors="replace") as fh:
                text = fh.read()
        except OSError:
            continue
        verdict, why = classify_script(text)
        if verdict != "no-download":
            script_rows.append((rel, verdict, why))

    failures = []

    print(
        f"check-vendor-fetch-pinned: {len(pkg_dirs)} discovered packages, "
        f"{len(cmake_files)} tracked CMake files, "
        f"{len(script_files)} in-package build scripts."
    )

    # --- RULE 1 ------------------------------------------------------------
    print("\nRULE 1 — every fetch inside a discovered package carries a digest")
    downloading = [r for r in in_pkg_fetches if r[4] != "no-download"]
    if not in_pkg_fetches:
        print(
            f"  NOTHING TO CHECK: 0 FetchContent_Declare / ExternalProject_Add\n"
            f"  calls exist inside any of the {len(pkg_dirs)} discovered packages.\n"
            "  That is a MEASURED FACT about this tree, not a pass — the rule has\n"
            "  no subject yet. The first vendor package to land (RFC-0087 D5)\n"
            "  gives it one."
        )
    else:
        for rel, line, call, name, verdict, why, owner in in_pkg_fetches:
            mark = {"ok": "OK", "no-download": "--", "violation": "FAIL"}[verdict]
            print(f"  [{mark}] {rel}:{line} {call}({name})  [{owner}]  {why}")
            if verdict == "violation":
                failures.append(f"{rel}:{line} {call}({name}) — {why}")
        if not downloading:
            print("  (every declaration is a local SOURCE_DIR — nothing is fetched.)")

    # --- RULE 2 ------------------------------------------------------------
    print("\nRULE 2 — an in-package build script that downloads verifies a digest")
    if not script_rows:
        print(
            f"  NOTHING TO CHECK: none of the {len(script_files)} build scripts\n"
            "  inside a discovered package reaches the network."
        )
    else:
        for rel, verdict, why in script_rows:
            mark = "OK" if verdict == "ok" else "FAIL"
            print(f"  [{mark}] {rel}: {why}")
            if verdict == "violation":
                failures.append(f"{rel} — {why}")

    # --- out of package scope: reported always, ratcheted ------------------
    print("\nOUTSIDE PACKAGE SCOPE — reported, and held by a shrink-only baseline")
    seen_keys = set()
    if not out_pkg_fetches:
        print("  NOTHING FOUND: no fetch lives outside a discovered package.")
    for rel, line, call, name, verdict, why, _owner in out_pkg_fetches:
        key = f"{rel}::{name}"
        seen_keys.add(key)
        if verdict != "violation":
            print(f"  [OK]   {rel}:{line} {call}({name})  {why}")
            if key in BASELINE:
                failures.append(
                    f"{key} is pinned now but still listed in BASELINE. Delete "
                    "the entry — a ratchet that keeps fixed rows stops being one."
                )
            continue
        if key in BASELINE:
            print(f"  [KNOWN] {rel}:{line} {call}({name})\n          {why}")
            print(f"          baseline: {BASELINE[key]}")
        else:
            print(f"  [FAIL] {rel}:{line} {call}({name})  {why}")
            failures.append(
                f"{rel}:{line} {call}({name}) — {why}\n"
                "    This fetch is outside every discovered package, so RFC-0087 "
                "D5's sentence does not reach it — but it runs inside those "
                "packages' builds, so the RULE does. Pin it, or add it to "
                "BASELINE with a reason and what would retire the entry."
            )
    for key in sorted(set(BASELINE) - seen_keys):
        failures.append(
            f"BASELINE names `{key}`, which no longer exists in the tree. "
            "Delete the entry."
        )

    print()
    if audit:
        print("check-vendor-fetch-pinned: --audit, reporting only.")
        return 0
    if failures:
        print("check-vendor-fetch-pinned: FAILED", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1
    empty_rules = int(not in_pkg_fetches) + int(not script_rows)
    suffix = f" ({empty_rules} rule(s) with NO SUBJECT — see above)" if empty_rules else ""
    print(f"check-vendor-fetch-pinned: OK{suffix}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
