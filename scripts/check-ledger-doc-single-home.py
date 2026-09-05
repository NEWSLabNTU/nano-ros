#!/usr/bin/env python3
"""The api-parity ledger's schema description lives in ONE file.

Issue 1095. `docs/reference/api-parity-ledger/` is 17 JSON shards, and every one
used to carry its own copy of the same `_doc` schema block — what a verdict
means, how a key is spelled, which fields are required. So the schema had 17
homes: changing it edited 17 files, and two concurrent PRs that both touched it
conflicted in up to 17 paths without disagreeing about anything.

That is not hypothetical and it was not cheap. Three phase-417 PRs (#329, #446,
#471) could not rebase because `main` and the branches had each rewritten that
block; resolving the one genuinely mechanical conflict in `scripts/api-parity.py`
only moved each PR to the next ledger shard. And the block GREW under
duplication — 34 lines to 76 — because `their-rename` had to write its new
paragraphs into all seventeen copies.

Same class as issues 0883/0884 one surface over: a file every PR touches becomes
the only conflicting path and the merge queue serialises on it. There the file
was generated, so the fix was to stop tracking it. Here it is hand-written and
mechanically duplicated, so the fix is one home plus a pointer.

WHAT THIS CHECKS

  * every shard's `_doc` names SCHEMA.md, so a reader of the raw JSON is not
    stranded;
  * no shard's `_doc` restates the schema vocabulary — the failure mode is
    someone helpfully pasting the block back in, which reads as an improvement
    and silently restores the 17-way conflict surface.

A shard MAY carry its own notes (qos.json documents its enum decisions). Those
are shard-specific and are exactly what a per-shard `_doc` is for.

Run:  python3 scripts/check-ledger-doc-single-home.py [--self-test]
"""

import glob
import json
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LEDGER = os.path.join(ROOT, "docs", "reference", "api-parity-ledger")
SCHEMA = os.path.join(LEDGER, "SCHEMA.md")

# Phrases that belong to the SCHEMA, not to a shard note. Chosen from the block's
# own vocabulary: a shard restating any of these has copied the schema back.
SCHEMA_MARKERS = (
    "verdict is one of",
    "Key is '<lang>",
    "One row per item where the nano-ros user API",
)


def offenders(doc_lines):
    """Schema markers found in a shard's `_doc`."""
    text = "\n".join(doc_lines)
    return [m for m in SCHEMA_MARKERS if m in text]


def self_test():
    assert offenders(["Schema: see SCHEMA.md", "", "qos note"]) == []
    assert offenders(["verdict is one of:", "  divergence  ..."]) == ["verdict is one of"]
    # A pointer that merely MENTIONS the file is not a restatement.
    assert offenders(["Schema: docs/reference/api-parity-ledger/SCHEMA.md"]) == []
    sys.stdout.write("check-ledger-doc-single-home self-test: OK\n")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return 0
    self_test()

    if not os.path.isfile(SCHEMA):
        sys.stderr.write(
            "error: %s is missing.\n"
            "It is the one home for the ledger schema (issue 1095); without it the\n"
            "shards point at nothing.\n" % os.path.relpath(SCHEMA, ROOT)
        )
        return 1

    shards = sorted(glob.glob(os.path.join(LEDGER, "*.json")))
    if not shards:
        sys.stderr.write(
            "error: no ledger shards found under %s — this gate would pass\n"
            "vacuously.\n" % os.path.relpath(LEDGER, ROOT)
        )
        return 1

    problems = []
    for path in shards:
        name = os.path.basename(path)
        try:
            doc = json.load(open(path, encoding="utf8")).get("_doc", [])
        except (OSError, ValueError) as exc:
            problems.append("%s: cannot read (%s)" % (name, exc))
            continue
        if not isinstance(doc, list):
            doc = [str(doc)]
        found = offenders(doc)
        if found:
            problems.append(
                "%s restates the SCHEMA in its own `_doc` (%s).\n"
                "      The schema has ONE home: SCHEMA.md. A copy here reads as a\n"
                "      helpful addition and silently restores the 17-way conflict\n"
                "      surface issue 1095 removed. Keep only notes specific to this\n"
                "      shard." % (name, ", ".join(repr(f) for f in found))
            )
        elif "SCHEMA.md" not in "\n".join(doc):
            problems.append(
                "%s's `_doc` does not point at SCHEMA.md.\n"
                "      Someone reading the raw JSON has to be told where the schema\n"
                "      lives, or the single home is just a hidden one." % name
            )

    if problems:
        sys.stderr.write("check-ledger-doc-single-home: %d problem(s)\n\n" % len(problems))
        for p in problems:
            sys.stderr.write("  - %s\n\n" % p)
        return 1

    sys.stdout.write(
        "check-ledger-doc-single-home: OK — %d shard(s) point at one SCHEMA.md.\n" % len(shards)
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
