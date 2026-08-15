#!/usr/bin/env python3
"""phase-361 W4 — the `std`/`alloc` feature contract, asserted.

The contract itself is normative text in `docs/design/ARCHITECTURE.md` §2
("The `std` / `alloc` contract"); this file is the only thing that makes it an
INVARIANT rather than a state. Every figure phase-361 reports — 0 implicit
enables, one `#[global_allocator]`, no `no_std` crate defaulting to `std` — was
a measurement taken by hand, and each one had already drifted at least once
before it was measured.

## The five clauses

a. **One spelling of the heap, in both layers.** In the manifest: a crate
   declaring both features lists `std = ["alloc", …]`, and no OTHER feature body
   enables `alloc` or `std`. In the source: every heap gate is
   `cfg(feature = "alloc")`; `cfg(any(feature = "alloc", feature = "std"))` is
   rejected outright.

   The source half is not pedantry. Deleting the manifest edge and respelling it
   as `any(...)` at the use sites was tried in W2.a: identical semantics, 123
   sites, +88 `std`-mentioning branches for phase-359 (which DELETES `std`) to
   unwind. It also hides from `check-std-census.py`, whose regex anchors
   `feature = "std"` directly after `cfg(` — so the branch count moved by 88 and
   the ratchet reported no change.

b. **No `no_std`-capable crate has a non-empty `default` containing `std` or
   `alloc`.** W3's rule; the reason is issue 0591 (a default `std` splits each
   crate into two compile identities inside one cargo invocation).

c. **Every declared `std`/`alloc` feature is USED** — a `cfg` site, or forwarding
   to a dependency. Catches the declaration that is documentation rather than
   mechanism (`nros-platform/{alloc,threading}`, deleted in W2.b).

   **The search covers `tests/`, `benches/` and `examples/`, not just `src/`,
   and that is load-bearing.** W2.b's hand-grep was `src/`-scoped and therefore
   called `nros-rmw-cyclonedds/std` dead. It gates two whole integration-test
   files through an inner `#![cfg]` on the test crate root — a site no
   `src/`-scoped search can see. A gate repeating that scope would delete a live
   feature with more authority than the hand-grep had.

d. **No feature in a `default` set is unreachable.** If every dep-site on a crate
   in this workspace passes `default-features = false` and none names the
   feature, then a `default`-only feature is dead in every real build. That is
   issue 0593 exactly: `nros/ffi-size-markers` (the `#[used]` attribute keeping
   the `__NROS_SIZE_*` statics alive for the C/C++ opaque-storage macros) was
   reachable ONLY through `nros`'s default set, which both consumers disable —
   so it appeared in a whole-workspace build by feature unification and vanished
   in the `-p nros-c` build cmake actually runs.

e. **Exactly one `#[global_allocator]`, and it is `nros-platform`'s.** W8.c cut
   four to one: `nros-c` and `nros-platform` had defined one under IDENTICAL
   gates, kept apart by a manifest comment, and two more bypassed
   `nros_platform_alloc` entirely. A grep found them, so a grep is what guards
   them. Test/bench/example targets are exempt: a test binary is its own image
   and may install a counting allocator (`nros-tests/tests/loan_e2e.rs`).

## Scope

`packages/**` — nano-ros's own crates. Not `examples/**`: those are USER code by
construction (RFC-0026 standalone copy-out projects), and the contract's whole
point is that the user spells `std`/`alloc` at their own dep-sites. Generated
message crates are excluded for the same reason plus a stronger one: they are
produced per host by `nros sync` and are not tracked.

Run `--self-test` to verify each clause fires on a deliberate reintroduction.
"""
import os
import re
import sys

try:
    import toml
except ImportError:  # pragma: no cover - environment guard
    sys.stderr.write("check-feature-contract: python3 `toml` module required\n")
    sys.exit(2)

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCOPE = "packages"
HEAP_FEATURES = ("std", "alloc")

# The one crate that may define `#[global_allocator]` (W8.c).
ALLOCATOR_OWNER = os.path.join("packages", "platform", "nros-platform")

# `cfg(any(feature = "alloc", feature = "std"))` in either order, and the
# `not(any(...))` form. Whitespace-tolerant; comments are stripped first.
ANY_HEAP_CFG = re.compile(
    r'any\s*\(\s*feature\s*=\s*"(alloc|std)"\s*,\s*feature\s*=\s*"(alloc|std)"\s*\)'
)
GLOBAL_ALLOC = re.compile(r"^\s*#\[global_allocator\]")


def is_build_output(name):
    """A directory that holds build OUTPUT rather than authored source.

    One spelling, matching `example_portability`'s: `build*/` and `target*/` at
    ANY level. This is load-bearing for clause (d) — `nros sync`'s metadata
    probe writes real `Cargo.toml`s under `<leaf>/build/nros-metadata/`, and 58
    of them dep `nros` WITHOUT `default-features = false`. Counting those as
    authored dep-sites made clause (d) unfalsifiable: every `default` feature
    looked reachable because a generated probe manifest reached it.
    """
    return name.startswith("build") or name.startswith("target") or name == "generated"


def manifests(root=None):
    root = root or ROOT
    out = []
    for base, dirs, files in os.walk(os.path.join(root, SCOPE)):
        dirs[:] = [d for d in dirs if not is_build_output(d)]
        if "Cargo.toml" in files:
            out.append(os.path.join(base, "Cargo.toml"))
    return sorted(out)


def strip_comments(text):
    """Drop `//` line comments so prose quoting a forbidden spelling is not a hit.

    The declaration comments this gate's own fixes added legitimately NAME the
    rejected form while explaining why it was rejected; counting those would
    teach everyone to phrase the explanation around the checker.
    """
    out = []
    for line in text.splitlines():
        s = line.lstrip()
        if s.startswith("//"):
            continue
        out.append(line.split("//", 1)[0])
    return "\n".join(out)


def rust_files(crate_dir, subdirs=("src", "tests", "benches", "examples")):
    for sub in subdirs:
        d = os.path.join(crate_dir, sub)
        if not os.path.isdir(d):
            continue
        for base, dirs, files in os.walk(d):
            dirs[:] = [x for x in dirs if not is_build_output(x)]
            for f in files:
                if f.endswith(".rs"):
                    yield os.path.join(base, f)


def read(p):
    with open(p, errors="replace") as fh:
        return fh.read()


def load(man):
    try:
        return toml.load(man)
    except Exception as exc:  # a malformed manifest is someone else's gate
        sys.stderr.write(f"  (skipping unparseable {rel(man)}: {exc})\n")
        return None


def rel(p):
    return os.path.relpath(p, ROOT)


def is_no_std_capable(crate_dir):
    lib = os.path.join(crate_dir, "src", "lib.rs")
    if not os.path.isfile(lib):
        return False
    head = read(lib)[:8000]
    return "#![no_std" in head or "no_std)]" in head


# ---------------------------------------------------------------------------
# Clauses
# ---------------------------------------------------------------------------


def clause_a_manifest(mans):
    """`std` lists `alloc`; no other feature body enables either."""
    bad = []
    for man, doc in mans:
        feats = (doc.get("features") or {})
        if "std" in feats and "alloc" in feats and "alloc" not in (feats["std"] or []):
            bad.append(
                f"{rel(man)}: declares both `std` and `alloc` but `std` does not list `alloc`.\n"
                f"      `std` implies a heap; write it ONCE here as `std = [\"alloc\", …]`\n"
                f"      rather than at every use site (ARCHITECTURE §2)."
            )
        for name, body in feats.items():
            if name in ("std", "alloc", "default"):
                continue  # `default` is clause (b)'s business
            enabled = [x for x in (body or []) if x in HEAP_FEATURES]
            if enabled:
                bad.append(
                    f"{rel(man)}: feature `{name}` enables {enabled}.\n"
                    f"      A capability/backend/platform feature REQUIRES the heap, it does not\n"
                    f"      grant it — emit `compile_error!` naming the feature the user must add."
                )
    return bad


def clause_a_source(crate_dirs):
    """The heap gate has one spelling: `cfg(feature = \"alloc\")`."""
    bad = []
    for crate in crate_dirs:
        for f in rust_files(crate):
            body = strip_comments(read(f))
            for i, line in enumerate(body.splitlines(), 1):
                if ANY_HEAP_CFG.search(line):
                    bad.append(
                        f"{rel(f)}:{i}: `any(alloc, std)` is the manifest edge respelled per use site.\n"
                        f"      Write `cfg(feature = \"alloc\")` and let `std = [\"alloc\", …]` in the\n"
                        f"      manifest carry `std`. W2.a measured this form at +88 branches, invisible\n"
                        f"      to check-std-census."
                    )
    return bad


def clause_b(mans):
    """No `no_std`-capable crate defaults to `std`/`alloc`."""
    bad = []
    for man, doc in mans:
        dflt = ((doc.get("features") or {}).get("default")) or []
        offenders = [x for x in dflt if x in HEAP_FEATURES]
        if offenders and is_no_std_capable(os.path.dirname(man)):
            bad.append(
                f"{rel(man)}: `no_std`-capable crate has `default = {dflt}`.\n"
                f"      A default heap/std splits the crate into two compile identities in one\n"
                f"      cargo invocation (issue 0591) and hands embedded users a heap unasked."
            )
    return bad


def clause_c(mans):
    """Every declared `std`/`alloc` feature is used or forwards."""
    bad = []
    for man, doc in mans:
        crate = os.path.dirname(man)
        feats = doc.get("features") or {}
        for name in HEAP_FEATURES:
            if name not in feats:
                continue
            body = feats[name] or []
            if any("/" in x or x.startswith("dep:") for x in body):
                continue
            pat = re.compile(r'feature\s*=\s*"%s"' % name)
            if any(pat.search(read(f)) for f in rust_files(crate)):
                continue
            bad.append(
                f"{rel(man)}: feature `{name}` has no `cfg` site and forwards nowhere.\n"
                f"      Enabling it does nothing, which the manifest advertises as a knob.\n"
                f"      Wire it or delete it. (Searched src/ tests/ benches/ examples/ —\n"
                f"      an inner `#![cfg]` on a test crate root counts, see W2.b.)"
            )
    return bad


def clause_d(mans):
    """No `default` feature is unreachable from every dep-site."""
    by_name = {}
    for man, doc in mans:
        nm = (doc.get("package") or {}).get("name")
        if nm:
            by_name[nm] = (man, doc)

    sites = {}
    for man, doc in mans:
        sections = []
        for sec in ("dependencies", "dev-dependencies", "build-dependencies"):
            sections.append(doc.get(sec) or {})
        for tgt in (doc.get("target") or {}).values():
            for sec in ("dependencies", "dev-dependencies", "build-dependencies"):
                sections.append(tgt.get(sec) or {})
        # A consumer reaches a dependency's feature TWO ways, and counting only
        # the first is a false positive factory: `features = [...]` on the dep
        # line, and a `dep/feature` (or optional `dep?/feature`) entry in its own
        # `[features]` table. `nros-board-nuttx-qemu` uses the second —
        #     image-runtime = ["nros-board-nuttx/image-runtime"]
        # — with `default-features = false` on the dep line, which this clause
        # first reported as unreachable. It is reachable; the gate was not
        # looking where cargo looks.
        forwarded = {}
        for body in (doc.get("features") or {}).values():
            for entry in body or []:
                if "/" not in entry or entry.startswith("dep:"):
                    continue
                dep_part, _, feat = entry.partition("/")
                forwarded.setdefault(dep_part.rstrip("?"), set()).add(feat)

        for table in sections:
            for dep, spec in table.items():
                if not isinstance(spec, dict):
                    spec = {}
                named = set(spec.get("features", []) or [])
                named |= forwarded.get(dep, set())
                sites.setdefault(dep, []).append(
                    (spec.get("default-features", True), sorted(named))
                )

    bad = []
    for nm, (man, doc) in sorted(by_name.items()):
        dflt = ((doc.get("features") or {}).get("default")) or []
        ss = sites.get(nm) or []
        if not dflt or not ss:
            continue
        if any(takes_default for (takes_default, _) in ss):
            continue
        named = set()
        for (_, feats) in ss:
            named.update(feats)
        unreachable = [f for f in dflt if f not in named]
        if unreachable:
            bad.append(
                f"{rel(man)}: `default` names {unreachable}, and all {len(ss)} in-workspace\n"
                f"      dep-sites on `{nm}` pass `default-features = false` without naming them.\n"
                f"      Reachable only by feature unification in a whole-workspace build — it\n"
                f"      disappears in the per-package build cmake runs (issue 0593). Request it\n"
                f"      at the dep-sites that need it."
            )
    return bad


def clause_e(crate_dirs, root):
    """One `#[global_allocator]`, owned by `nros-platform`."""
    found = []
    for crate in crate_dirs:
        for f in rust_files(crate, subdirs=("src",)):
            for i, line in enumerate(read(f).splitlines(), 1):
                if GLOBAL_ALLOC.match(line):
                    found.append((f, i))
    owner = os.path.join(root, ALLOCATOR_OWNER)
    strays = [(f, i) for (f, i) in found if not f.startswith(owner + os.sep)]
    bad = []
    for f, i in strays:
        bad.append(
            f"{rel(f)}:{i}: a second `#[global_allocator]`.\n"
            f"      `{ALLOCATOR_OWNER}` is the sole owner (W8.c); the rest forward to it via\n"
            f"      `nros_platform_alloc`, so a second cannot be spelled. Four existed once,\n"
            f"      two under identical gates kept apart by a manifest comment."
        )
    if not found:
        bad.append(
            "no `#[global_allocator]` found in packages/**/src — the owner in "
            f"{ALLOCATOR_OWNER} has gone missing, which is not an improvement."
        )
    return bad


CLAUSES = (
    ("a/manifest", "`std` lists `alloc`; nothing else enables either", None),
    ("a/source", "the heap gate has one spelling", None),
    ("b", "no `no_std` crate defaults to `std`/`alloc`", None),
    ("c", "every declared `std`/`alloc` feature is used", None),
    ("d", "no `default` feature is unreachable", None),
    ("e", "exactly one `#[global_allocator]`", None),
)


def run(root=None):
    root = root or ROOT
    mans = []
    for m in manifests(root):
        doc = load(m)
        if doc is not None:
            mans.append((m, doc))
    crate_dirs = [os.path.dirname(m) for m, _ in mans]

    results = [
        ("a/manifest", clause_a_manifest(mans)),
        ("a/source", clause_a_source(crate_dirs)),
        ("b", clause_b(mans)),
        ("c", clause_c(mans)),
        ("d", clause_d(mans)),
        ("e", clause_e(crate_dirs, root)),
    ]
    return mans, results


def main():
    if "--self-test" in sys.argv:
        return self_test()

    mans, results = run()
    failed = False
    for name, bad in results:
        label = dict((k, v) for k, v, _ in CLAUSES).get(name, "")
        if bad:
            failed = True
            print(f"[FAIL] clause ({name}) — {label}")
            for b in bad:
                print(f"  {b}")
        else:
            print(f"  ok  ({name}) {label}")
    if failed:
        print()
        print("The contract is ARCHITECTURE.md §2 'The `std` / `alloc` contract'.")
        print("Phase-360 W4 exists so these stay invariants rather than measurements.")
        return 1
    print(f"check-feature-contract: OK ({len(mans)} crate(s), 6 clauses)")
    return 0


# ---------------------------------------------------------------------------
# Self-test — every clause must FAIL on a deliberate reintroduction. A gate
# nobody has watched fail is a gate nobody knows the polarity of.
# ---------------------------------------------------------------------------


def self_test():
    import shutil
    import tempfile

    def crate(root, name, manifest, files):
        d = os.path.join(root, SCOPE, name)
        os.makedirs(os.path.join(d, "src"), exist_ok=True)
        with open(os.path.join(d, "Cargo.toml"), "w") as fh:
            fh.write(manifest)
        for rel_path, body in files.items():
            p = os.path.join(d, rel_path)
            os.makedirs(os.path.dirname(p), exist_ok=True)
            with open(p, "w") as fh:
                fh.write(body)
        return d

    OWNER_SRC = '#![no_std]\n#[cfg(feature = "alloc")]\nmod a {}\n#[global_allocator]\nstatic A: X = X;\n'

    def base(root):
        # A minimal WELL-FORMED tree: the allocator owner, at its real path.
        crate(
            root,
            "platform/nros-platform",
            '[package]\nname = "nros-platform"\n[features]\nalloc = []\n',
            {"src/lib.rs": OWNER_SRC},
        )

    def run_in(root):
        return dict(run(root)[1])

    failures = []

    def expect(case, root, clause, want_fail):
        got = run_in(root)[clause]
        if want_fail and not got:
            failures.append(f"{case}: clause ({clause}) did NOT fire")
        if not want_fail and got:
            failures.append(f"{case}: clause ({clause}) fired on a clean tree: {got}")

    with tempfile.TemporaryDirectory(dir=os.path.join(ROOT, "tmp")) as tmp:
        # 0. The baseline tree passes every clause.
        r = os.path.join(tmp, "clean")
        base(r)
        for c in ("a/manifest", "a/source", "b", "c", "d", "e"):
            expect("clean", r, c, want_fail=False)

        # a/manifest — `std` declared beside `alloc` without listing it.
        r = os.path.join(tmp, "a1")
        base(r)
        crate(r, "x", '[package]\nname = "x"\n[features]\nstd = []\nalloc = []\n',
              {"src/lib.rs": '#[cfg(feature = "std")]\n#[cfg(feature = "alloc")]\nfn f() {}\n'})
        expect("std-without-alloc", r, "a/manifest", want_fail=True)

        # a/manifest — a capability feature enabling the heap.
        r = os.path.join(tmp, "a1b")
        base(r)
        crate(r, "x", '[package]\nname = "x"\n[features]\nstd = ["alloc"]\nalloc = []\nparam-services = ["alloc"]\n',
              {"src/lib.rs": '#[cfg(feature = "std")]\n#[cfg(feature = "alloc")]\nfn f() {}\n'})
        expect("capability-enables-heap", r, "a/manifest", want_fail=True)

        # a/source — the `any(alloc, std)` respelling, in both orders.
        spellings = [
            '#[cfg(any(feature = "alloc", feature = "std"))]',
            '#[cfg(not(any(feature = "std", feature = "alloc")))]',
        ]
        for i, spelling in enumerate(spellings):
            r = os.path.join(tmp, f"a2_{i}")
            base(r)
            crate(r, "x", '[package]\nname = "x"\n[features]\nstd = ["alloc"]\nalloc = []\n',
                  {"src/lib.rs": f'{spelling}\nfn f() {{}}\n#[cfg(feature = "std")]\nfn g() {{}}\n'})
            expect(f"any-spelling-{i}", r, "a/source", want_fail=True)

        # a/source — the same text in a COMMENT is not a hit.
        r = os.path.join(tmp, "a2c")
        base(r)
        crate(r, "x", '[package]\nname = "x"\n[features]\nstd = ["alloc"]\nalloc = []\n',
              {"src/lib.rs": '// was any(feature = "alloc", feature = "std") — see W2.a\n'
                             '#[cfg(feature = "alloc")]\n#[cfg(feature = "std")]\nfn f() {}\n'})
        expect("any-in-comment", r, "a/source", want_fail=False)

        # b — a no_std crate defaulting to std.
        r = os.path.join(tmp, "b")
        base(r)
        crate(r, "x", '[package]\nname = "x"\n[features]\ndefault = ["std"]\nstd = ["alloc"]\nalloc = []\n',
              {"src/lib.rs": '#![no_std]\n#[cfg(feature = "std")]\n#[cfg(feature = "alloc")]\nfn f() {}\n'})
        expect("no_std-defaults-std", r, "b", want_fail=True)

        # b — a HOSTED crate may default to std.
        r = os.path.join(tmp, "b2")
        base(r)
        crate(r, "x", '[package]\nname = "x"\n[features]\ndefault = ["std"]\nstd = ["alloc"]\nalloc = []\n',
              {"src/lib.rs": '#[cfg(feature = "std")]\n#[cfg(feature = "alloc")]\nfn f() {}\n'})
        expect("hosted-defaults-std", r, "b", want_fail=False)

        # c — a declared feature with no cfg site anywhere.
        r = os.path.join(tmp, "c")
        base(r)
        crate(r, "x", '[package]\nname = "x"\n[features]\nalloc = []\n', {"src/lib.rs": "fn f() {}\n"})
        expect("dead-declaration", r, "c", want_fail=True)

        # c — the W2.b case: used ONLY from tests/, via an inner `#![cfg]`.
        r = os.path.join(tmp, "c2")
        base(r)
        crate(r, "x", '[package]\nname = "x"\n[features]\nalloc = []\n',
              {"src/lib.rs": "fn f() {}\n",
               "tests/t.rs": '#![cfg(feature = "alloc")]\nfn t() {}\n'})
        expect("used-only-from-tests", r, "c", want_fail=False)

        # d — a default feature no dep-site can reach.
        r = os.path.join(tmp, "d")
        base(r)
        crate(r, "x", '[package]\nname = "x"\n[features]\ndefault = ["markers"]\nmarkers = []\n',
              {"src/lib.rs": '#[cfg(feature = "markers")]\nfn f() {}\n'})
        crate(r, "y", '[package]\nname = "y"\n[dependencies]\nx = { path = "../x", default-features = false }\n',
              {"src/lib.rs": "fn f() {}\n"})
        expect("unreachable-default", r, "d", want_fail=True)

        # d — one dep-site naming it is enough.
        r = os.path.join(tmp, "d2")
        base(r)
        crate(r, "x", '[package]\nname = "x"\n[features]\ndefault = ["markers"]\nmarkers = []\n',
              {"src/lib.rs": '#[cfg(feature = "markers")]\nfn f() {}\n'})
        crate(r, "y", '[package]\nname = "y"\n[dependencies]\n'
                      'x = { path = "../x", default-features = false, features = ["markers"] }\n',
              {"src/lib.rs": "fn f() {}\n"})
        expect("default-requested-explicitly", r, "d", want_fail=False)

        # d — reached by FORWARDING (`dep/feature` in the consumer's own feature
        # table) rather than by `features = [...]` on the dep line. This is how
        # `nros-board-nuttx-qemu` reaches `image-runtime`, and reporting it as
        # unreachable was this clause's first false positive.
        r = os.path.join(tmp, "d3")
        base(r)
        crate(r, "x", '[package]\nname = "x"\n[features]\ndefault = ["markers"]\nmarkers = []\n',
              {"src/lib.rs": '#[cfg(feature = "markers")]\nfn f() {}\n'})
        crate(r, "y", '[package]\nname = "y"\n[features]\nmarkers = ["x/markers"]\n'
                      '[dependencies]\nx = { path = "../x", default-features = false }\n',
              {"src/lib.rs": "fn f() {}\n"})
        expect("default-reached-by-forwarding", r, "d", want_fail=False)

        # d — the OPTIONAL-dep spelling `dep?/feature` counts too.
        r = os.path.join(tmp, "d4")
        base(r)
        crate(r, "x", '[package]\nname = "x"\n[features]\ndefault = ["markers"]\nmarkers = []\n',
              {"src/lib.rs": '#[cfg(feature = "markers")]\nfn f() {}\n'})
        crate(r, "y", '[package]\nname = "y"\n[features]\nmarkers = ["x?/markers"]\n'
                      '[dependencies]\nx = { path = "../x", default-features = false, optional = true }\n',
              {"src/lib.rs": "fn f() {}\n"})
        expect("default-reached-by-optional-forwarding", r, "d", want_fail=False)

        # d — forwarding a DIFFERENT feature does not launder the unreachable one.
        r = os.path.join(tmp, "d5")
        base(r)
        crate(r, "x", '[package]\nname = "x"\n[features]\ndefault = ["markers"]\nmarkers = []\nother = []\n',
              {"src/lib.rs": '#[cfg(feature = "markers")]\nfn f() {}\n#[cfg(feature = "other")]\nfn g() {}\n'})
        crate(r, "y", '[package]\nname = "y"\n[features]\nother = ["x/other"]\n'
                      '[dependencies]\nx = { path = "../x", default-features = false }\n',
              {"src/lib.rs": "fn f() {}\n"})
        expect("forwarding-a-different-feature", r, "d", want_fail=True)

        # e — a second allocator outside the owner.
        r = os.path.join(tmp, "e")
        base(r)
        crate(r, "x", '[package]\nname = "x"\n[features]\nalloc = []\n',
              {"src/lib.rs": '#[cfg(feature = "alloc")]\nfn f() {}\n#[global_allocator]\nstatic B: X = X;\n'})
        expect("second-allocator", r, "e", want_fail=True)

        # e — a test target may install its own (loan_e2e's counting allocator).
        r = os.path.join(tmp, "e2")
        base(r)
        crate(r, "x", '[package]\nname = "x"\n[features]\nalloc = []\n',
              {"src/lib.rs": '#[cfg(feature = "alloc")]\nfn f() {}\n',
               "tests/t.rs": "#[global_allocator]\nstatic B: X = X;\n"})
        expect("allocator-in-a-test", r, "e", want_fail=False)

        # e — the owner going missing is itself a failure.
        r = os.path.join(tmp, "e3")
        os.makedirs(os.path.join(r, SCOPE), exist_ok=True)
        crate(r, "x", '[package]\nname = "x"\n[features]\nalloc = []\n',
              {"src/lib.rs": '#[cfg(feature = "alloc")]\nfn f() {}\n'})
        expect("owner-missing", r, "e", want_fail=True)

    if failures:
        for f in failures:
            sys.stderr.write(f"self-test: {f}\n")
        return 2
    print("check-feature-contract self-test: OK (17 cases, every clause fires and holds)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
