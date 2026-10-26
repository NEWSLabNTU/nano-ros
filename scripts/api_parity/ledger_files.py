"""The api-parity ledger's ON-DISK layout: one file per row (issue 1091).

The ONE reader and the ONE writer of `docs/reference/api-parity-ledger/`.
Every tool that reads the ledger goes through `load`; a second spelling of the
layout would agree with this one until the day it did not.

    docs/reference/api-parity-ledger/
      SCHEMA.md                 what a row means (issue 1095)
      <topic>/                  one directory per stage, named for `topics.NAMES`
        _doc.json               {"_doc": [...]} -- notes specific to the topic
        <row-file>.json         {"key": "<lang>:<item>", "verdict": ..., ...}

WHY ONE FILE PER ROW. The ledger used to be 17 topic shards, each one JSON
object written `sort_keys=True`. Sorting converts NAME correlation into
POSITION correlation: two branches adding rows for adjacent items -- the normal
case, because a feature's items sort together -- inserted at the same base line,
and git cannot merge that. A merge driver cannot fix it (GitHub rebases queue
entries server-side, where drivers do not run -- issue 0884), and the ledger is
authored, so it cannot be generated and ignored either. A new row in its own
new file has no neighbour to collide with, which is why `docs/issues/` filings
never conflicted while the generated index did.

THE DIRECTORY IS THE TOPIC. The shard identity (`_shard`) is what `--check`
uses to refuse a row filed under the wrong stage; it is read off the directory,
so it survives the layout change unmodified.

THE ROW CARRIES ITS OWN KEY, and the file name is DERIVED from it by
`row_filename`, an INJECTIVE escaping: two different keys can never name the
same file, and the loader refuses a file whose name is not its key's. That
last rule is what keeps the duplicate-key protection meaningful: a second row
for one symbol must land on the SAME path, where git reports an add/add
conflict, instead of in a second file that would silently lose one verdict.
A key repeated in two topic directories is the remaining way to write it
twice, and that is a hard error too.
"""

import json
import os

DOC_FILE = "_doc.json"
SUFFIX = ".json"

# Kept verbatim in a file name. Everything else is escaped, so the mapping is
# injective by construction: `.` is the escape for `:` and `~` introduces a hex
# byte, and neither is in this set, so neither can come from a key literally.
_SAFE = frozenset("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-")


class LedgerError(SystemExit):
    """A ledger that cannot be read as one. Exits, like every loader error did."""


def row_filename(key):
    """The file a row lives in, derived from its key and nothing else.

    `:` becomes `.` (so `cpp:rclcpp::Node::now` reads `cpp.rclcpp..Node..now`),
    every other byte outside `[A-Za-z0-9_-]` becomes `~XX` (UTF-8, upper hex).
    Deterministic, filesystem-safe and injective; it is not meant to be read
    back -- the row says its own key.
    """
    out = []
    for ch in key:
        if ch in _SAFE:
            out.append(ch)
        elif ch == ":":
            out.append(".")
        else:
            out.extend("~%02X" % b for b in ch.encode("utf-8"))
    return "".join(out) + SUFFIX


def _canonical(value):
    """`value` with every nested object's keys sorted, so a write is stable."""
    if isinstance(value, dict):
        return {k: _canonical(value[k]) for k in sorted(value)}
    if isinstance(value, list):
        return [_canonical(v) for v in value]
    return value


def render_row(key, row):
    """The exact text of a row file: `key` first, then the fields sorted."""
    body = {"key": key}
    body.update(_canonical({k: v for k, v in row.items() if not k.startswith("_")}))
    return json.dumps(body, indent=2, ensure_ascii=False) + "\n"


def write_row(ledger_dir, topic, key, row):
    """Write one row to `<ledger_dir>/<topic>/<row_filename(key)>`; return the path."""
    topic_dir = os.path.join(ledger_dir, topic)
    os.makedirs(topic_dir, exist_ok=True)
    path = os.path.join(topic_dir, row_filename(key))
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(render_row(key, row))
    return path


def write_rows(ledger_dir, topic, rows, doc=None):
    """Write a whole topic: `{key: row}` plus an optional `_doc` list.

    For the self-tests' planted ledgers, which used to write one shard file.
    """
    for key, row in rows.items():
        write_row(ledger_dir, topic, key, row)
    if doc is not None:
        with open(os.path.join(ledger_dir, topic, DOC_FILE), "w", encoding="utf-8") as fh:
            fh.write(json.dumps({"_doc": doc}, indent=2, ensure_ascii=False) + "\n")


def topic_dirs(ledger_dir):
    """[(topic, path)] for every topic directory, sorted."""
    if not os.path.isdir(ledger_dir):
        return []
    return [(name, os.path.join(ledger_dir, name))
            for name in sorted(os.listdir(ledger_dir))
            if os.path.isdir(os.path.join(ledger_dir, name))]


def doc_lines(ledger_dir, topic):
    """A topic's `_doc` list, or None when it has no `_doc.json`."""
    path = os.path.join(ledger_dir, topic, DOC_FILE)
    if not os.path.isfile(path):
        return None
    with open(path, encoding="utf-8") as fh:
        doc = json.load(fh).get("_doc", [])
    return doc if isinstance(doc, list) else [str(doc)]


def _no_duplicate_keys(where):
    # A DUPLICATE KEY is silently survivable and therefore dangerous: JSON
    # parsers keep the last occurrence, so a second value discards the first
    # with no error anywhere. The hook runs on EVERY object, nested ones
    # included, and returns a real dict -- an earlier version returned the pair
    # list itself, which detected the duplicates but left every nested object
    # (`rename`, `their_rename`) as a list of tuples for the rest of the tool.
    def hook(pairs):
        seen = set()
        for key, _ in pairs:
            if key in seen:
                raise LedgerError(
                    f"api-parity ledger: {where} defines {key!r} more than once. JSON "
                    f"keeps the LAST, so the other value is being discarded silently.")
            seen.add(key)
        return dict(pairs)
    return hook


def load(ledger_dir):
    """Every row, merged into `{key: row}`, documentation dropped.

    Each row gains `_shard` (its topic directory) and `_file` (its path relative
    to `ledger_dir`, which is what the person fixing it has to open). Refuses,
    naming the file: a shard FILE at the top level (the retired layout -- its
    rows would otherwise be read by nothing), a row file with no `key`, one
    whose name is not `row_filename(key)`, one carrying a `_`-prefixed field
    (those are the loader's), and a key two files both claim.
    """
    merged = {}
    if not os.path.isdir(ledger_dir):
        return merged
    stray = sorted(n for n in os.listdir(ledger_dir)
                   if n.endswith(SUFFIX) and os.path.isfile(os.path.join(ledger_dir, n)))
    if stray:
        raise LedgerError(
            "api-parity ledger: %s at the top of %s. That is the retired one-shard-per-"
            "topic layout (issue 1091); a row now lives in its own file under "
            "<topic>/, named by its key. Move each row with `write_row`." % (
                ", ".join(stray), ledger_dir))
    for topic, topic_dir in topic_dirs(ledger_dir):
        for name in sorted(os.listdir(topic_dir)):
            if not name.endswith(SUFFIX) or name.startswith("_"):
                continue
            rel = "%s/%s" % (topic, name)
            with open(os.path.join(topic_dir, name), encoding="utf-8") as fh:
                row = json.loads(fh.read(), object_pairs_hook=_no_duplicate_keys(rel))
            key = row.pop("key", None) if isinstance(row, dict) else None
            if not isinstance(key, str) or not key:
                raise LedgerError(
                    "api-parity ledger: %s has no \"key\" field. A row file names the "
                    "ledger key it answers, `<lang>:<item>`." % rel)
            want = row_filename(key)
            if name != want:
                raise LedgerError(
                    "api-parity ledger: %s holds %r, whose file is %s/%s. The name is "
                    "derived from the key so that two rows for one key collide in git "
                    "instead of both surviving; rename the file." % (rel, key, topic, want))
            private = sorted(k for k in row if k.startswith("_"))
            if private:
                raise LedgerError(
                    "api-parity ledger: %s carries %s; `_`-prefixed fields are the "
                    "loader's and would be overwritten." % (rel, ", ".join(private)))
            if key in merged:
                raise LedgerError(
                    "api-parity ledger: %r is in both %s and %s. One key, one row -- "
                    "merge them, do not delete either reason, and keep the one in the "
                    "topic `--check` asks for." % (key, merged[key]["_file"], rel))
            row["_shard"] = topic
            row["_file"] = rel
            merged[key] = row
    return merged
