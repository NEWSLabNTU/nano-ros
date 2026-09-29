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

Measured on the tree it was written for, BEFORE the W12 corrections: 364
backticked identifiers, 9 unresolved, of which 2 were real defects —
`RET_UNSUPPORTED`, an abbreviation of a constant that does exist, and a
hypothetical slot name written as though it were one. After them: 362 cited,
7 unresolved, all 7 external.

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

import contextlib
import io
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

# phase-472 W3 — ONE comment stripper. Source 2 used to be a bare `git grep -w`,
# so a name that exists only in a COMMENT somewhere else in the tree resolved:
# prose vouching for prose, one file over from the self-vouching SELF_EXCLUDE
# already refuses. Fourteen cited names resolved that way when this landed.
sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
import comments  # noqa: E402

TICK = re.compile(r"`([A-Za-z_][A-Za-z0-9_]*)`")
IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")


def prose(text):
    """The COMMENT text of a C header (`/* */` and `//`), code blanked."""
    code = comments.strip_comments(text, "c")
    return "".join(o if o != c else (" " if o != "\n" else o) for o, c in zip(text, code))


def headers():
    return sorted(f for f in os.listdir(HDR_DIR) if f.endswith(".h"))


def header_text():
    return {f: open(os.path.join(HDR_DIR, f), encoding="utf-8").read() for f in headers()}


def cited(text_by_file):
    """`{name: {header, …}}` — every backticked identifier in header PROSE."""
    out = {}
    for f, t in text_by_file.items():
        for tk in TICK.finditer(prose(t)):
            out.setdefault(tk.group(1), set()).add(f)
    return out


def abi_code_identifiers(text_by_file):
    """Source 1 — identifiers in the headers' own code, comments stripped."""
    got = set()
    for t in text_by_file.values():
        got |= set(IDENT.findall(comments.strip_comments(t, "c")))
    return got


def upstream_identifiers():
    """Source 3 — the recorded contract and signature snapshots."""
    got = set()
    for path in UPSTREAM:
        with open(path, encoding="utf-8") as fh:
            got |= set(IDENT.findall(fh.read()))
    return got


# Files whose mention of a name must NOT count as resolving it, because each
# of them CONTAINS the prose under test or a name invented to test it:
#
#   * the ABI headers — source 1 reads their CODE, and their prose must not
#     vouch for itself;
#   * `generated.rs` — bindgen copies that same prose into it verbatim, so a
#     fiction would resolve against its own reflection;
#   * THIS SCRIPT — its self-test plants a name that exists nowhere, and the
#     plant is a string literal here. Committing this file therefore made the
#     probe resolve and the self-test stop failing on a fiction it had just
#     been shown. Measured: green run after run while the file was untracked,
#     red on the first run after the commit. A gate that is part of its own
#     corpus is the same defect one level up from the one it checks.
SELF_EXCLUDE = (
    "nros-rmw-abi/include",
    "generated.rs",
    os.path.basename(__file__),
)


_CODE = {}  # path -> stripped text; one strip per file per run


def names_in_code(path, text, name):
    """Does `name` occur, word-bounded, in the CODE of this file?

    Comments never count (phase-472 W3). String literals DO: a slot or type
    name in a `dlsym`-style string or a cmake variable in quotes is still the
    tree naming it. A file of no language `comments` knows does not vouch at
    all — measured when this landed: no cited name resolved ONLY through one,
    so failing closed there costs nothing and cannot be laundered through a
    `.txt` note."""
    lang = comments.lang_for(path)
    if lang is None:
        return False
    code = _CODE.get(path)
    if code is None:
        code = _CODE[path] = comments.strip_comments(text, lang)
    return re.search(r"(?<![A-Za-z0-9_])" + re.escape(name) + r"(?![A-Za-z0-9_])", code) is not None


_GREP = {}  # name -> tracked files `git grep -w` finds it in


def prefetch(names):
    """ONE `git grep` for many names (255 separate ones cost ~9 s)."""
    names = [n for n in dict.fromkeys(names) if n not in _GREP]
    if not names:
        return
    args = ["git", "-C", ROOT, "grep", "-o", "-w", "-F"]
    for n in names:
        args += ["-e", n]
    args += ["--", "packages", "examples", "cmake", "scripts", "zephyr", "third-party"]
    r = subprocess.run(args, capture_output=True, text=True, check=False)
    for n in names:
        _GREP[n] = []
    seen = set()
    for line in r.stdout.splitlines():
        path, _, word = line.rpartition(":")
        if word in _GREP and (word, path) not in seen:
            seen.add((word, path))
            _GREP[word].append(path)


def in_tree(name):
    """Source 2 — tracked non-markdown sources naming this identifier in CODE."""
    if name not in _GREP:
        prefetch([name])
    # Short files first, and stop at the first CODE hit: the caller asks
    # "does anything vouch", and stripping every one of hundreds of hits for a
    # common word is what made this gate take a minute.
    hits = [
        h for h in _GREP[name]
        if not h.endswith(".md") and not any(x in h for x in SELF_EXCLUDE)
        and comments.lang_for(h) is not None
    ]
    hits.sort(key=lambda h: os.path.getsize(os.path.join(ROOT, h)))
    for h in hits:
        with open(os.path.join(ROOT, h), encoding="utf-8", errors="replace") as fh:
            if names_in_code(h, fh.read(), name):
                return [h]
    return []


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
    prefetch([n for n in cited(text_by_file) if n not in known])
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
    comments.self_test()

    # phase-472 W3 — a name only a COMMENT elsewhere mentions does not resolve;
    # the same name in code does. Both directions, on the predicate `in_tree`
    # applies to every hit.
    if names_in_code("probe1.rs", "// w3_probe_name is prose\nfn other() {}\n", "w3_probe_name"):
        bad.append("a name only a // comment mentions was read as code")
    if names_in_code("probe2.c", "/* w3_probe_name */ int other;\n", "w3_probe_name"):
        bad.append("a name only a /* */ comment mentions was read as code")
    if not names_in_code("probe3.c", "int w3_probe_name; // other\n", "w3_probe_name"):
        bad.append("a declared name was not read as code")
    if names_in_code("x.txt", "w3_probe_name\n", "w3_probe_name"):
        bad.append("a file of no known language vouched for a name")
    # ...and header prose in a `//` comment is prose too.
    if set(cited({"p.h": "int x; // names `w3_line_prose`\n"})) != {"w3_line_prose"}:
        bad.append("a backtick in a // comment was not read as a citation")

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
    # one assertion, with no tree mutation. ONE pass, not two: `plant` is
    # `real` plus a file whose only content is a comment, and a comment
    # contributes no code identifiers, so `unresolved(real)` is exactly this
    # minus the probe.
    probe = "nros_slot_that_never_existed"
    if any(probe in t for t in real.values()):
        bad.append("the real headers cite the probe name — pick another")
    plant = dict(real)
    plant["probe.h"] = "/** `%s` */\n" % probe
    names = [n for n, _ in unresolved(plant)]
    if probe not in names:
        bad.append(
            "a fictional name in header prose was NOT reported — something in "
            "the resolution corpus vouches for it (see SELF_EXCLUDE)"
        )

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
        "15 case(s))"
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
    # Always, not only behind the flag (phase-472 W9): a negative control
    # nobody runs decays into a comment. Quiet on success.
    with contextlib.redirect_stdout(io.StringIO()) as _selftest_out:
        _selftest_rc = self_test()
    if _selftest_rc:
        sys.stdout.write(_selftest_out.getvalue())
        return _selftest_rc

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
