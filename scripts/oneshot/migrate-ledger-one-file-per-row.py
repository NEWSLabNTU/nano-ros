#!/usr/bin/env python3
"""One-shot migration for issue 1091: the api-parity ledger, one file per row.

Reads the 17 retired topic shards `docs/reference/api-parity-ledger/<topic>.json`
and writes each row to `<topic>/<row_filename(key)>.json` through the layout
module every reader now uses (`scripts/api_parity/ledger_files.py`), each
shard's `_doc` to `<topic>/_doc.json`, then deletes the shard. Run once; it
refuses to run when no shard is left.

The one key two shards both carried (`cpp:spin_once`, in `exec.json` AND
`node.json`, identical text) is written once, to the shard the old loader let
win -- it iterated shards in sorted order and kept the LAST -- so the merged
ledger every tool sees is unchanged. Two rows that DIFFER are refused: that is
a verdict a person has to choose, not one a script may pick.

Prose in a `why` that names a retired shard FILE (`in boot.json`) is rewritten
to name its directory (`in boot/`), because `check-ledger-orphan-refs` resolves
bare filenames and those files no longer exist. Every row whose text changes is
listed, so the change is countable rather than incidental.

usage: python3 scripts/oneshot/migrate-ledger-one-file-per-row.py
"""

import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.join(ROOT, "scripts", "api_parity"))
import ledger_files  # noqa: E402
import topics  # noqa: E402

LEDGER = os.path.join(ROOT, "docs", "reference", "api-parity-ledger")
SHARD_CITE = re.compile(r"\b(%s)\.json\b(:\d+)?" % "|".join(topics.NAMES))


def _cite(m):
    # A LINE in a shard is history -- that file and its numbering are gone, so
    # the sentence says so (which is also what the orphan gate reads for).
    if m.group(2):
        return "line %s of the retired %s.json" % (m.group(2)[1:], m.group(1))
    return m.group(1) + "/"


def rewrite(value, changed):
    if isinstance(value, str):
        new = SHARD_CITE.sub(_cite, value.replace(
            "other.json:218 and :1070", "lines 218 and 1070 of the retired `other` shard file"))
        if new != value:
            changed.append(1)
        return new
    if isinstance(value, dict):
        return {k: rewrite(v, changed) for k, v in value.items()}
    if isinstance(value, list):
        return [rewrite(v, changed) for v in value]
    return value


def main():
    shards = sorted(n for n in os.listdir(LEDGER) if n.endswith(".json"))
    if not shards:
        raise SystemExit("no top-level shard left under %s -- already migrated" % LEDGER)
    rows, docs, seen_in = {}, {}, {}
    for name in shards:
        topic = name[: -len(".json")]
        if topic not in topics.NAMES:
            raise SystemExit("%s is not a topic shard" % name)
        with open(os.path.join(LEDGER, name), encoding="utf-8") as fh:
            data = json.loads(fh.read(), object_pairs_hook=ledger_files._no_duplicate_keys(name))
        for key, value in data.items():
            if key.startswith("_"):
                if key != "_doc":
                    raise SystemExit("%s: unexpected documentation key %r" % (name, key))
                docs[topic] = value
                continue
            if key in rows:
                if rows[key][1] != value:
                    raise SystemExit("%r differs between %s and %s.json" % (
                        key, name, seen_in[key]))
                print("duplicate %s: %s.json and %s carry identical rows; keeping %s "
                      "(the old loader's winner)" % (key, seen_in[key], name, topic))
            rows[key] = (topic, value)
            seen_in[key] = topic

    names = {}
    for key, (topic, _v) in rows.items():
        fn = (topic, ledger_files.row_filename(key))
        if fn in names:
            raise SystemExit("%r and %r map to one file %s/%s" % (names[fn], key, *fn))
        names[fn] = key

    rewritten = []
    for key, (topic, value) in sorted(rows.items()):
        changed = []
        value = rewrite(value, changed)
        if changed:
            rewritten.append(key)
        ledger_files.write_row(LEDGER, topic, key, value)
    for topic, doc in docs.items():
        with open(os.path.join(LEDGER, topic, ledger_files.DOC_FILE), "w", encoding="utf-8") as fh:
            fh.write(json.dumps({"_doc": doc}, indent=2, ensure_ascii=False) + "\n")
    for name in shards:
        os.remove(os.path.join(LEDGER, name))

    print("migrated %d row(s) from %d shard(s) into %d topic dir(s)" % (
        len(rows), len(shards), len(docs)))
    print("%d row(s) had a retired shard filename in their prose, rewritten:" % len(rewritten))
    for key in rewritten:
        print("  " + key)


if __name__ == "__main__":
    main()
