#!/usr/bin/env python3
"""Resolve a book probe track from the generated track list — phase-522 W2.

`scripts/probe/probe-tracks.json` is generated from the board registry's
`[[probe]]` entries by `scripts/gen-board-support.py`. This is its one reader
for the shell and the workflow, so the probe harness and `probe-platforms.yml`
cannot list different tracks.

  probe-track.py <track>      print "<chapter path>\\t<variant>\\t<board row>\\t<offline>"
                              for the track (the row is "" for a track that
                              is not a board's, e.g. `workspace`; offline is
                              1 for a track replayed with no network, else 0)
  probe-track.py --list       print every track name, one per line
  probe-track.py --weekly     print the weekly tracks as a JSON array (the
                              `probe-platforms.yml` matrix)
  probe-track.py --twin <track>
                              print a twin track's board facts (phase-524 W4a)
                              as shell assignments `NROS_TWIN_<FIELD>=<quoted>`,
                              list fields space-joined; nothing for a track
                              that is not a twin's

An unknown track exits 2 naming every known one.
"""

import json
import shlex
import sys
from pathlib import Path

TRACKS = Path(__file__).resolve().parent / "probe-tracks.json"


def tracks() -> list[dict]:
    return json.loads(TRACKS.read_text())["tracks"]


def main(argv: list[str]) -> int:
    if len(argv) == 2 and argv[0] == "--twin":
        for t in tracks():
            if t["track"] == argv[1]:
                for k, v in (t.get("twin") or {}).items():
                    v = " ".join(v) if isinstance(v, list) else v
                    print(f"NROS_TWIN_{k.upper()}={shlex.quote(v)}")
                return 0
        print(f"probe: unknown track '{argv[1]}'", file=sys.stderr)
        return 2
    if len(argv) != 1:
        print(__doc__, file=sys.stderr)
        return 2
    arg = argv[0]
    rows = tracks()
    if arg == "--list":
        print("\n".join(t["track"] for t in rows))
        return 0
    if arg == "--weekly":
        print(json.dumps([t["track"] for t in rows if t["weekly"]]))
        return 0
    for t in rows:
        if t["track"] == arg:
            print(f"book/src/{t['chapter']}\t{t['variant']}\t{t.get('row', '')}\t"
                  f"{1 if t.get('offline') else 0}")
            return 0
    print(f"probe: unknown PROBE_PLATFORM '{arg}' (want "
          f"{'|'.join(t['track'] for t in rows)})", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
