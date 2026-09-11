#!/usr/bin/env python3
"""A backtick in the RMW ABI headers means THIS IDENTIFIER EXISTS.

Phase-428 W12. Every mechanism this campaign built validates a DECLARATION:
`rmw-abi-shape` compares slot signatures against upstream, `rmw-api-parity`
holds the map to the header, `rmw-vtable-order` holds the positional
initialisers to it. **Nothing read what the header says about itself**, and the
header says a great deal — it is the design record as well as the ABI, and its
prose went stale in exactly the way the parity map did, only with no gate to
notice:

  * `rmw_ret.h`'s file block stated the PRE-W3.d return contract as current and
    was wrong four ways at once, thirteen months after step B;
  * `take_loaned_message` and `take_sequence` each carried a COMPLETE pre-W3.d
    doc block stacked above their current one, which bindgen concatenated into
    `generated.rs`, so the Rust side shipped both the contract and its
    contradiction;
  * five slot names retired by W3.b were still referenced as live —
    try_recv_raw, try_recv_reply_raw and friends — including one, in
    `send_request`'s block, naming the slot that is its own partner.

The part of that which is MECHANICAL is the names. A doc naming a slot that
does not exist is checkable, and this checks it — not by knowing which names
are retired (an authored list of those is the same drift one level up), but by
requiring every backticked identifier to RESOLVE against something the tree or
the recorded upstream snapshot actually contains.

RESOLUTION SOURCES, all derived:

  1. the ABI headers' own CODE — declarations, macros, typedefs, struct
     members, with comments stripped so prose cannot vouch for itself;
  2. anything `git grep -w` finds in tracked NON-MARKDOWN sources (a Rust
     method, a cmake variable, a C symbol in a backend). Markdown is excluded
     deliberately: docs cite each other, and a name that exists only in prose
     is precisely what this looks for;
  3. the recorded upstream snapshot — `rmw-implementation-contract.txt` and
     `rmw-implementation-signatures.txt`, which carry upstream's symbols, its
     return and parameter TYPES and its parameter NAMES. This is what lets the
     headers cite `rmw_take_with_info` or `rosidl_message_type_support_t`
     without a ROS install.

Measured on the tree it was written for: 364 backticked identifiers, 9
unresolved, of which 2 were real defects (`RET_UNSUPPORTED`, an abbreviation of
a constant that does exist, and a hypothetical slot name written as though it
were one).

WHAT THE BASELINE IS FOR, AND WHAT IT IS NOT

The remaining 7 name things that belong to OTHER PROJECTS and appear in neither
the tree nor the recorded snapshot: a zenoh-c type, a micro-ROS entry point, an
Iron-only upstream field, upstream constants and types no contract symbol takes.
They are correct citations that this repo has no offline way to resolve, so they
sit in a RATCHET baseline — the shape `check-prose-issue-refs` already uses for
a correct reference the tree cannot check. The baseline is EXEMPTIONS, never the
subject list: the subject is every backticked identifier in every ABI header,
derived on each run, so a new fiction fails whether or not anyone updates
anything. An entry that stops being cited is reported too, so the file cannot
quietly accumulate.

A RETIRED NAME IS WRITTEN WITHOUT TICKS. Three blocks in these headers name
retired slots on purpose, to record what the prose used to claim. They spell
them bare — try_recv_raw, not `try_recv_raw` — which is what makes the
convention this gate enforces usable rather than a reason for exemptions.

Usage:
    scripts/check-rmw-doc-slot-names.py            # the report
    scripts/check-rmw-doc-slot-names.py --check    # fail on an unresolved name
    scripts/check-rmw-doc-slot-names.py --self-test
    scripts/check-rmw-doc-slot-names.py --write-baseline
"""

import argparse
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
HDR_DIR = os.path.join(ROOT, "packages", "core", "nros-rmw-abi", "include", "nros")
UPSTREAM = (
    os.path.join(ROOT, "docs", "reference", "rmw-implementation-contract.txt"),
    os.path.join(ROOT, "docs", "reference", "rmw-implementation-signatures.txt"),
)
BASELINE = os.path.join(ROOT, ".config", "rmw-doc-external-names.txt")

TICK = re.compile(r"`([A-Za-z_][A-Za-z0-9_]*)`")
IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
BLOCK = re.compile(r"/\*.*?\*/", re.S)


def headers():
    return sorted(f for f in os.listdir(HDR_DIR) if f.endswith(".h"))


def header_text():
    return {f: open(os.path.join(HDR_DIR, f), encoding="utf-8").read() for f in headers()}


def cited(text_by_file):
    """`{name: {header, …}}` — every backticked identifier in header PROSE."""
    out = {}
    for f, t in text_by_file.items():
        for m in BLOCK.finditer(t):
            for tk in TICK.finditer(m.group(0)):
                out.setdefault(tk.group(1), set()).add(f)
    return out


def abi_code_identifiers(text_by_file):
    """Source 1 — identifiers in the headers' own code, comments stripped."""
    got = set()
    for t in text_by_file.values():
        got |= set(IDENT.findall(BLOCK.sub(" ", t)))
    return got


def upstream_identifiers():
    """Source 3 — the recorded contract and signature snapshots."""
    got = set()
    for path in UPSTREAM:
        with open(path, encoding="utf-8") as fh:
            got |= set(IDENT.findall(fh.read()))
    return got


def in_tree(name):
    """Source 2 — tracked non-markdown sources naming this identifier.

    `generated.rs` is excluded because bindgen copies the very prose being
    checked into it: a fictional name would vouch for itself. The ABI headers
    are excluded for the same reason — source 1 reads their CODE, and their
    prose must not count.
    """
    r = subprocess.run(
        ["git", "-C", ROOT, "grep", "-l", "-w", "-F", name, "--",
         "packages", "examples", "cmake", "scripts", "zephyr", "third-party"],
        capture_output=True, text=True, check=False,
    )
    return [
        h for h in r.stdout.split()
        if not h.endswith(".md")
        and "nros-rmw-abi/include" not in h
        and not h.endswith("generated.rs")
    ]


def read_baseline():
    rows = {}
    try:
        fh = open(BASELINE, encoding="utf-8")
    except FileNotFoundError:
        return rows
    with fh:
        for line in fh:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            name, _, why = line.partition("  ")
            rows[name.strip()] = why.strip()
    return rows


def unresolved(text_by_file=None):
    """`sorted [(name, [header, …])]` for every cited name that resolves nowhere."""
    text_by_file = header_text() if text_by_file is None else text_by_file
    known = abi_code_identifiers(text_by_file) | upstream_identifiers()
    out = []
    for name, where in sorted(cited(text_by_file).items()):
        if name in known:
            continue
        if in_tree(name):
            continue
        out.append((name, sorted(where)))
    return out


def self_test():
    """Negative controls on the NORMAL path, against the real headers.

    Planted text rather than a mutated tree: the rule is about what counts as
    resolution, and every arm of that has to be exercised in both directions.
    """
    bad = []
    real = header_text()

    # A cited name that IS declared in the headers resolves (source 1) — and
    # its declaration, not its prose, is what resolves it.
    code = abi_code_identifiers(real)
    if "take_sequence" not in code:
        bad.append("source 1 does not see a real slot declaration")
    if "try_recv_raw" in code:
        bad.append("source 1 saw a retired name — comments are not being stripped")

    # Source 3 covers upstream symbols, types and parameter names.
    up = upstream_identifiers()
    for sym in ("rmw_take_with_info", "rosidl_message_type_support_t"):
        if sym not in up:
            bad.append(f"source 3 does not resolve upstream `{sym}`")

    # The extractor reads PROSE only, and reads it from every header.
    planted = {
        "probe.h": "/** doc names `a_fiction_name` */\nint a_declaration_name;\n",
    }
    got = cited(planted)
    if set(got) != {"a_fiction_name"}:
        bad.append(f"citation extraction wrong: {sorted(got)}")
    if abi_code_identifiers(planted) & {"a_fiction_name"}:
        bad.append("a name that appears ONLY in prose was read as code")
    if "a_declaration_name" not in abi_code_identifiers(planted):
        bad.append("a declared name was not read as code")

    # An unresolvable name in planted prose is REPORTED — the whole gate, in
    # one assertion, with no tree mutation.
    plant = dict(real)
    plant["probe.h"] = "/** `nros_slot_that_never_existed` */\n"
    names = [n for n, _ in unresolved(plant)]
    if "nros_slot_that_never_existed" not in names:
        bad.append("a fictional name in header prose was NOT reported")
    base = [n for n, _ in unresolved(real)]
    if "nros_slot_that_never_existed" in base:
        bad.append("the real headers cite the probe name — pick another")

    # The baseline is exemptions over a DERIVED subject, so every entry in it
    # must still be cited by some header. One that is not is stale.
    cites = cited(real)
    for name in sorted(read_baseline()):
        if name not in cites:
            bad.append(
                f"baseline names {name}, which no header cites any more — "
                "delete the row"
            )

    if bad:
        for b in bad:
            sys.stderr.write("check-rmw-doc-slot-names --self-test: " + b + "\n")
        return 2
    print(
        f"check-rmw-doc-slot-names --self-test: OK ({len(cites)} cited name(s), "
        "10 case(s))"
    )
    return 0


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--write-baseline", action="store_true")
    args = ap.parse_args(argv)

    if args.self_test:
        return self_test()

    rows = unresolved()
    allowed = read_baseline()

    if args.write_baseline:
        with open(BASELINE, "w", encoding="utf-8") as fh:
            fh.write(
                "# Backticked identifiers in the RMW ABI headers' prose that name\n"
                "# something OUTSIDE this tree and outside the recorded upstream\n"
                "# snapshot, so `check-rmw-doc-slot-names` cannot resolve them\n"
                "# offline. EXEMPTIONS, not the subject list — the subject is every\n"
                "# backticked identifier, derived on each run.\n"
                "#\n"
                "# A retired slot name does NOT belong here: those are written\n"
                "# without code ticks, which is the convention that keeps this file\n"
                "# short. Add a row only for a name another project owns, and say\n"
                "# which project.\n"
                "#\n"
                "# Regenerate: scripts/check-rmw-doc-slot-names.py --write-baseline\n"
            )
            for name, where in rows:
                fh.write(f"{name}  {allowed.get(name, 'TODO: which project owns it')}\n")
        print(f"wrote {BASELINE} ({len(rows)} row(s))")
        return 0

    print(f"# RMW ABI header prose: {len(cited(header_text()))} backticked identifier(s)\n")
    news = [(n, w) for n, w in rows if n not in allowed]
    print(f"  unresolved          {len(rows):>3}")
    print(f"  of those, exempted  {len(rows) - len(news):>3}")
    print(f"  NEW                 {len(news):>3}")
    if rows:
        print("\n## unresolved\n")
        for name, where in rows:
            tag = "" if name in allowed else "   <-- NEW"
            print(f"  {name:<48} {','.join(where)}{tag}")

    if not args.check:
        return 0

    rc = 0
    if news:
        rc = 1
        sys.stderr.write(
            "\nERROR: header prose backticks a name that resolves nowhere:\n"
        )
        for name, where in news:
            sys.stderr.write(f"  {name}  ({', '.join(where)})\n")
        sys.stderr.write(
            "A backtick in these headers means THIS IDENTIFIER EXISTS. If it is a\n"
            "slot that was renamed, use the current name; if it names a shape that\n"
            "does not exist, write it without ticks; if it belongs to another\n"
            "project, add it to .config/rmw-doc-external-names.txt with the project.\n"
        )

    stale = sorted(set(allowed) - {n for n, _ in rows})
    if stale:
        rc = 1
        sys.stderr.write("\nERROR: baseline row for a name nothing cites (or that now resolves):\n")
        for name in stale:
            sys.stderr.write(f"  {name}\n")
        sys.stderr.write("Delete it — an exemption for a citation that is gone.\n")

    if rc == 0:
        print("\ncheck-rmw-doc-slot-names --check: OK (every backticked name resolves)")
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
