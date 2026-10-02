#!/usr/bin/env python3
"""Where each RMW backend allocates, and whether it is on the steady-state path.

Issue 0777 found seven declared ABI deviations justified by "no runtime
allocation to pre-size; pools are baked" — a clause true of one backend in five.
The conclusion those deviations reached survived, but the reason did not, and a
reason nobody can re-run is a reason that can be wrong for years.

So this is the re-run. It enumerates every allocation call in the backends'
own sources and classifies it by the function it sits in:

  steady-state — on the publish / take / request / reply path, so it happens per
                 MESSAGE and its cost lands in worst-case latency
  create       — entity or transport setup, so it happens a bounded number of
                 times and its cost lands in startup

What it deliberately does NOT measure: allocations inside the middleware
libraries themselves (Cyclone below `dds_write`, zenoh-pico's `z_malloc`).
Those are real — an image on either calls a general allocator per message
whatever this reports — but they are not nano-ros sites and cannot be fixed
here. The point of the split is which allocations are OURS to remove.

Usage:
    scripts/rmw-alloc-sites.py              # the report
    scripts/rmw-alloc-sites.py --check      # fail on an undeclared steady-state site
    scripts/rmw-alloc-sites.py --self-test
"""

import contextlib
import io
import argparse
import os
import re
import subprocess
import sys
import sys as _w3_sys  # noqa: E402
from pathlib import Path as _W3Path  # noqa: E402
_w3_sys.path.insert(0, str(_W3Path(__file__).resolve().parent / "lib"))
import comments  # noqa: E402  phase-472 W3 — the one comment stripper

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# The allocators themselves: libc, Cyclone's ddsrt, and the platform ABI every
# backend is meant to funnel through (RFC-0034).
BASE_ALLOCATORS = frozenset({
    "ddsrt_malloc", "ddsrt_calloc", "ddsrt_realloc", "ddsrt_strdup",
    "malloc", "calloc", "realloc", "strdup",
    "nros_platform_alloc", "nros_platform_realloc",
})

# Issue 1605: a funnel helper hides its callers. Issue 0832 routed every XRCE
# allocation through `nros_xrce_calloc` (a `static inline` in a header), and a
# call list naming only the allocators above printed NO xrce row at all — seven
# open-path allocations read as "XRCE does not allocate".
#
# So helpers are FOLLOWED, not enumerated: a `static inline` function, or any
# function defined in a HEADER, whose body reaches an allocator (or another
# helper, to a fixed point) is a helper. Each CALL of a helper is a site,
# attributed to the caller; the helper's own body is not, which would count one
# allocation twice. (A header body is never scanned as a site, so without this
# rule a header helper is invisible by construction.)
#
# An out-of-line helper in a `.c` file cannot be told apart from a constructor
# by shape — both are "a function that allocates" — so it is DECLARED here,
# with why its callers, not its body, are the sites.
DECLARED_HELPERS = {
    "z_malloc": (
        "zenoh-pico's allocator hook (zpico-sys/c/zpico/platform_aliases.c), a "
        "forwarder to nros_platform_alloc. Its callers live inside zenoh-pico, "
        "which this report deliberately does not measure"
    ),
    "z_realloc": "zenoh-pico's realloc hook; same reasoning as z_malloc",
}

SOURCE_EXT = (".c", ".cpp", ".cc")
HEADER_EXT = (".h", ".hpp", ".hh")


def call_re(names):
    alts = "|".join(sorted((re.escape(n) for n in names), key=len, reverse=True))
    return re.compile(r"\b(" + alts + r")\s*\(")


ALLOC = call_re(BASE_ALLOCATORS)

# A definition's name is the last identifier before its parameter list, on a
# line that starts in column 0. Cyclone's functions live inside `namespace
# nros_rmw_cyclonedds {`, so brace depth is never 0 at a definition and cannot
# be the test — the column is. The prefix (return type) is OPTIONAL: clang-format
# wraps a long signature as `rmw_ret_t\nxrce_subscription_create(…`, and a
# required prefix left that name unmatched, so its body was attributed to the
# PREVIOUS function — `xrce_topic_callback`, a per-message callback (issue 1605).
HEAD = re.compile(r"^(?:[A-Za-z_][A-Za-z0-9_:<>,\*\s&]*?\b)?([A-Za-z_][A-Za-z0-9_]*)\s*\(")

# Functions reached per MESSAGE. Everything else is treated as setup.
STEADY = {
    "publisher_publish_raw",
    "publisher_publish_streamed",
    "subscription_take",
    # Issue 0970 — the sertype's serdata constructors. Not named for an RMW
    # entry point because Cyclone calls them, but they are per message on both
    # sides: `from_ser`/`from_ser_iov` build the received sample, `from_sample`
    # builds the published one.
    "serdata_from_ser",
    "serdata_from_ser_iov",
    "serdata_from_sample",
    "serdata_alloc",
    "service_take_request",
    "service_send_response",
    "client_send_request",
    "client_take_response",
    "xrce_publisher_publish_raw",
    "xrce_publisher_publish_streamed",
    "xrce_subscription_take",
}

# Steady-state sites that exist and are accounted for. `--check` fails on a
# steady-state allocation that is NOT here, so a new one has to be argued for
# rather than merging quietly.
#
# Measured 2026-08-26. Cyclone is the whole list; XRCE reached zero when issue
# 0782 landed, and uORB never had one.
DECLARED = {
    # Issues 0969 and 0970 removed the publish and take entries that stood
    # here. `publisher_publish_raw` had TWO per message — a message-sized
    # `ddsrt_malloc` for the body and a `ddsrt_calloc` of the typed sample —
    # and `subscription_take` had the typed sample plus the ostream's
    # growth-by-realloc. None of them exist now: neither direction decodes.
    #
    # What replaced them is ONE allocation per message per direction, below,
    # and the count alone would understate that. Cyclone was ALREADY
    # allocating a serdata and its payload on the receive path, inside
    # libddsc where this scanner cannot see it; the sites below are that same
    # allocation, moved into our sertype. So the honest reading of this table
    # across the two issues is not "3 became 2" but "the typed sample, its
    # per-member allocations, the body copy and the ostream are gone, and what
    # remains is what Cyclone was doing anyway".
    ("packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/nros_sertype.cpp", "serdata_alloc"): (
        "ONE `ddsrt_malloc` per message, in EACH direction: `serdata_from_ser` / "
        "`serdata_from_ser_iov` call it for a received sample, `serdata_from_sample` "
        "for a published one. It is sized by the message and holds the CDR the "
        "serdata carries. Cyclone's own `serdata_default` did exactly this before "
        "and still does for every topic this backend has not taken over, so what "
        "issue 0970 did was move the allocation rather than add one. Removable only "
        "by borrowing the receive buffer instead of owning it — the loan model, "
        "which RFC-0038 records as not porting to a network DDS backend — and on "
        "the publish side not at all, since `dds_write` returns before the network "
        "does and the bytes have to be owned by then"
    ),
}


def tracked(exts):
    listing = subprocess.run(
        ["git", "-C", ROOT, "ls-files", "packages/rmw"],
        capture_output=True, text=True, check=False,
    ).stdout.split()
    return [f for f in listing if f.endswith(exts) and "/tests/" not in f]


def sources():
    return tracked(SOURCE_EXT)


def backend_of(rel):
    return rel.split("/")[2]


def strip_comments(text):
    """Both comment forms, preserving line numbers.

    A block comment is not optional to handle: `xrce/src/publisher.c` explains
    the allocation issue 0782 REMOVED, in prose containing `malloc(total)`, and
    a scan that skips only `//` reports the fix as never having landed.
    """
    # phase-472 W3 — the shared stripper (scripts/lib/comments.py).
    return comments.strip_comments(text, "cpp")


def owners(lines):
    """Per line, (enclosing definition name, whether that definition is inline).

    The definition is the nearest column-0 HEAD at or above the line."""
    out = []
    fn, inline = "<file scope>", False
    for i, ln in enumerate(lines):
        m = HEAD.match(ln)
        if m and ln[:1] not in (" ", "\t", "#", ""):
            fn = m.group(1)
            prev = lines[i - 1] if i else ""
            inline = bool(re.search(r"\binline\b", ln[: m.start(1)] + " " + prev))
        out.append((fn, inline))
    return out


def find_helpers(texts):
    """The allocation helpers among `texts` ({rel: source}), to a fixed point.

    A helper is a function that reaches an allocator (or a helper) AND is
    either `inline`, defined in a header, or named in DECLARED_HELPERS."""
    parsed = []
    for rel, text in texts.items():
        lines = strip_comments(text).split("\n")
        parsed.append((rel.endswith(HEADER_EXT), lines, owners(lines)))
    helpers = set(DECLARED_HELPERS)
    while True:
        rx = call_re(BASE_ALLOCATORS | helpers)
        grown = set(helpers)
        for is_header, lines, own in parsed:
            for ln, (fn, inline) in zip(lines, own):
                if fn == "<file scope>" or fn in grown:
                    continue
                if (is_header or inline) and any(
                    m.group(1) != fn for m in rx.finditer(ln)
                ):
                    grown.add(fn)
        if grown == helpers:
            return helpers
        helpers = grown


def sites_in(text, helpers=frozenset()):
    """[(line, function, allocator)] for one file's source.

    `allocator` is what the line CALLS — a base allocator or a helper. A line
    inside a helper's own body is not a site: its callers are."""
    lines = strip_comments(text).split("\n")
    rx = call_re(BASE_ALLOCATORS | set(helpers))
    out = []
    for i, (ln, (fn, _inline)) in enumerate(zip(lines, owners(lines))):
        if fn in helpers:
            continue
        for a in rx.finditer(ln):
            out.append((i + 1, fn, a.group(1)))
    return out


def read_texts(rels):
    texts = {}
    for rel in rels:
        try:
            with open(os.path.join(ROOT, rel), encoding="utf-8", errors="replace") as fh:
                texts[rel] = fh.read()
        except OSError:
            continue
    return texts


def scan():
    """(found, helpers, backends) — every site, the helpers followed, and every
    backend directory scanned (so a backend with no site still gets a row)."""
    srcs = read_texts(sources())
    helpers = find_helpers({**read_texts(tracked(HEADER_EXT)), **srcs})
    found = []
    for rel, text in srcs.items():
        for line, fn, alloc in sites_in(text, helpers):
            found.append((rel, line, fn, alloc, fn in STEADY))
    return found, helpers, sorted({backend_of(r) for r in srcs})


def self_test():
    bad = []

    # A block comment describing a removed allocation is not an allocation.
    src = "/* this used to malloc(total) and stage it */\nint f(void) { return 0; }\n"
    if sites_in(src):
        bad.append("a `malloc(` inside a block comment was counted")

    # A namespace-scoped definition is still found by column, not brace depth.
    src = (
        "namespace ns {\n"
        "rmw_ret_t publisher_publish_raw(const rmw_publisher_t* p) {\n"
        "    void* s = ddsrt_calloc(1, n);\n"
        "}\n"
        "}\n"
    )
    got = sites_in(src)
    if got != [(3, "publisher_publish_raw", "ddsrt_calloc")]:
        bad.append(f"namespace-scoped definition not attributed: {got}")

    # An indented call inside a nested block still belongs to the definition.
    src = "void create_thing(void) {\n  if (x) {\n    p = malloc(4);\n  }\n}\n"
    if sites_in(src) != [(3, "create_thing", "malloc")]:
        bad.append(f"nested-block call misattributed: {sites_in(src)}")

    # Issue 1605: a funnel helper in a HEADER hides its callers unless it is
    # followed. The header's body is never a site; each call of it is.
    header = (
        "static inline void* nros_xyz_calloc(size_t n, size_t s) {\n"
        "    void* p;\n"
        "    if ((p = nros_platform_alloc(n * s)) != NULL) { memset(p, 0, n * s); }\n"
        "    return p;\n"
        "}\n"
    )
    src = "int publisher_create(void) {\n    st = nros_xyz_calloc(1, 8);\n}\n"
    helpers = find_helpers({"x/internal.h": header, "x/publisher.c": src})
    if "nros_xyz_calloc" not in helpers:
        bad.append(f"a header `static inline` funnel was not followed: {sorted(helpers)}")
    if sites_in(src, helpers) != [(2, "publisher_create", "nros_xyz_calloc")]:
        bad.append(f"a call of a funnel helper was not counted: {sites_in(src, helpers)}")

    # A helper of a helper is followed too (fixed point), and an inline helper
    # in a SOURCE file contributes its callers, not its own body.
    src = (
        "static inline void* inner(size_t n) {\n    return malloc(n);\n}\n"
        "static inline void* outer(size_t n) {\n    return inner(n);\n}\n"
        "int subscription_take(void) {\n    q = outer(4);\n}\n"
    )
    helpers = find_helpers({"x/a.c": src})
    if sites_in(src, helpers) != [(8, "subscription_take", "outer")]:
        bad.append(f"a chained inline helper was not followed: {sites_in(src, helpers)}")

    # A definition whose return type is wrapped onto the line above still
    # names itself (the name starts in column 0).
    src = (
        "void take_cb(void) {\n}\n"
        "rmw_ret_t\nsubscription_create(const char* t,\n                    int n) {\n"
        "    p = malloc(4);\n}\n"
    )
    if sites_in(src) != [(6, "subscription_create", "malloc")]:
        bad.append(f"a wrapped-signature definition was misattributed: {sites_in(src)}")

    # A non-inline function that allocates is a CONSTRUCTOR, not a helper —
    # its callers are not allocation sites.
    src = "void* make_thing(void) {\n    return malloc(4);\n}\nint f(void) { make_thing(); }\n"
    helpers = find_helpers({"x/b.c": src})
    if "make_thing" in helpers:
        bad.append("an out-of-line constructor was treated as a funnel helper")

    if bad:
        for b in bad:
            sys.stderr.write("rmw-alloc-sites --self-test: " + b + "\n")
        return 2
    print("rmw-alloc-sites --self-test: OK (7 case(s))")
    return 0


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args(argv)

    if args.self_test:
        return self_test()
    # Always, not only behind the flag (phase-472 W9): a negative control
    # nobody runs decays into a comment. Quiet on success.
    with contextlib.redirect_stdout(io.StringIO()) as _selftest_out:
        _selftest_rc = self_test()
    if _selftest_rc:
        sys.stdout.write(_selftest_out.getvalue())
        return _selftest_rc

    found, helpers, backends = scan()
    steady = [f for f in found if f[4]]
    setup = [f for f in found if not f[4]]

    # Issue 1605: a row for EVERY backend scanned — an absent row read as
    # "this backend does not allocate", which is a claim, while a 0 / 0 row is
    # a measurement.
    by_backend = {name: (0, 0) for name in backends}
    for rel, _line, _fn, _alloc, is_steady in found:
        name = backend_of(rel)
        s, c = by_backend.get(name, (0, 0))
        by_backend[name] = (s + 1, c) if is_steady else (s, c + 1)

    print("# RMW backend allocation sites\n")
    print(f"{'backend':<22} {'steady-state':>12} {'create/init':>12}")
    for name in sorted(by_backend):
        s, c = by_backend[name]
        print(f"{name:<22} {s:>12} {c:>12}")
    print(f"\nallocation helpers followed (their callers are the sites): "
          f"{', '.join(sorted(helpers)) or '(none)'}")

    print(f"\n## steady-state ({len(steady)}) — per message, so this is latency\n")
    for rel, line, fn, alloc, _ in steady:
        mark = " " if (rel, fn) in DECLARED else "!"
        print(f"{mark} {rel}:{line}\t{alloc}\tin {fn}()")

    print(f"\n## create / init ({len(setup)}) — bounded, so this is startup\n")
    for rel, line, fn, alloc, _ in setup:
        print(f"  {rel}:{line}\t{alloc}\tin {fn}()")

    if args.check:
        undeclared = sorted({(r, f) for r, _l, f, _a, s in found if s and (r, f) not in DECLARED})
        if undeclared:
            sys.stderr.write(
                "\nERROR: allocation on a steady-state path with no declared reason:\n"
            )
            for rel, fn in undeclared:
                sys.stderr.write(f"  {rel}  {fn}()\n")
            sys.stderr.write(
                "Add it to DECLARED with what it costs per message, or move the "
                "allocation to entity creation.\n"
            )
            return 1
        stale = sorted(
            k for k in DECLARED
            if k not in {(r, f) for r, _l, f, _a, s in found if s}
        )
        if stale:
            sys.stderr.write("\nERROR: DECLARED names a steady-state site that is gone:\n")
            for rel, fn in stale:
                sys.stderr.write(f"  {rel}  {fn}()\n")
            sys.stderr.write("Remove the entry — the allocation it explains no longer exists.\n")
            return 1
        print("\nrmw-alloc-sites --check: OK (every steady-state site is declared)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
