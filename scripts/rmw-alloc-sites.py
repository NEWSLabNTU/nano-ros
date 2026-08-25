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
    "write_typed",
    "write_fibonacci_get_result_response",
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
    ("packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/publisher.cpp", "publisher_publish_raw"): (
        "TWO per publish. `ddsrt_malloc(body_len)` is message-sized and strips the "
        "4-byte CDR encapsulation header; it looks droppable — `dds_istream_init` "
        "takes a `const void *`, so the stream could point at `data + 4` — and it "
        "is NOT. `dds_cdr_alignto` aligns the stream INDEX and reads at "
        "`m_buffer + m_index`, so an 8-aligned index only yields an 8-aligned "
        "address when the BASE is 8-aligned. `ddsrt_malloc` gives that; `data + 4` "
        "gives 4-byte alignment at best, which is an unaligned 64-bit read for any "
        "message with an int64/double. The copy is load-bearing. "
        "`ddsrt_calloc(1, desc->m_size)` is the typed sample: fixed size, known at "
        "create time, and removable by holding one per publisher"
    ),
    ("packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/subscriber.cpp", "subscription_take"): (
        "`ddsrt_calloc(1, desc->m_size)` per take. Fixed size, known at create "
        "time — the one issue 0777 called out as mattering to a real-time budget, "
        "and removable the same way as the publisher's"
    ),
    ("packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/service.cpp", "write_typed"): (
        "TWO per request and per reply — the request/reply analogue of the publish "
        "path, reached from `client_send_request` and `service_send_response`"
    ),
    (
        "packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/service.cpp",
        "write_fibonacci_get_result_response",
    ): ("nested inside `write_typed`, so it is on the same per-reply path"),
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
