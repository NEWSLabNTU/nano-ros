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

import argparse
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

ALLOC = re.compile(
    r"\b(ddsrt_malloc|ddsrt_calloc|ddsrt_realloc|ddsrt_strdup"
    r"|malloc|calloc|realloc|strdup)\s*\("
)

# A definition's name is the last identifier before its parameter list, on a
# line that starts in column 0. Cyclone's functions live inside `namespace
# nros_rmw_cyclonedds {`, so brace depth is never 0 at a definition and cannot
# be the test — the column is.
HEAD = re.compile(r"^[A-Za-z_][A-Za-z0-9_:<>,\*\s&]*?\b([A-Za-z_][A-Za-z0-9_]*)\s*\(")

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


def sources():
    listing = subprocess.run(
        ["git", "-C", ROOT, "ls-files", "packages/rmw"],
        capture_output=True, text=True, check=False,
    ).stdout.split()
    return [
        f for f in listing
        if f.endswith((".c", ".cpp", ".cc")) and "/tests/" not in f
    ]


def strip_comments(text):
    """Both comment forms, preserving line numbers.

    A block comment is not optional to handle: `xrce/src/publisher.c` explains
    the allocation issue 0782 REMOVED, in prose containing `malloc(total)`, and
    a scan that skips only `//` reports the fix as never having landed.
    """
    text = re.sub(r"/\*.*?\*/", lambda m: re.sub(r"[^\n]", " ", m.group(0)), text, flags=re.S)
    return re.sub(r"(?m)//.*$", "", text)


def sites_in(text):
    """[(line, function, allocator)] for one file's source."""
    lines = strip_comments(text).split("\n")
    out = []
    for i, ln in enumerate(lines):
        for a in ALLOC.finditer(ln):
            fn = "<file scope>"
            for j in range(i, -1, -1):
                m = HEAD.match(lines[j])
                if m and lines[j][:1] not in (" ", "\t", "#", ""):
                    fn = m.group(1)
                    break
            out.append((i + 1, fn, a.group(1)))
    return out


def scan():
    found = []
    for rel in sources():
        try:
            text = open(os.path.join(ROOT, rel), encoding="utf-8", errors="replace").read()
        except OSError:
            continue
        for line, fn, alloc in sites_in(text):
            found.append((rel, line, fn, alloc, fn in STEADY))
    return found


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

    if bad:
        for b in bad:
            sys.stderr.write("rmw-alloc-sites --self-test: " + b + "\n")
        return 2
    print("rmw-alloc-sites --self-test: OK (3 case(s))")
    return 0


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args(argv)

    if args.self_test:
        return self_test()

    found = scan()
    steady = [f for f in found if f[4]]
    setup = [f for f in found if not f[4]]

    by_backend = {}
    for rel, _line, _fn, _alloc, is_steady in found:
        name = rel.split("/")[2]
        s, c = by_backend.get(name, (0, 0))
        by_backend[name] = (s + 1, c) if is_steady else (s, c + 1)

    print("# RMW backend allocation sites\n")
    print(f"{'backend':<22} {'steady-state':>12} {'create/init':>12}")
    for name in sorted(by_backend):
        s, c = by_backend[name]
        print(f"{name:<22} {s:>12} {c:>12}")

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
