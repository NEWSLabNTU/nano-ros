#!/usr/bin/env python3
"""phase-475 W5 — compare a fresh census with the committed admission list.

    lane-census-diff.py <committed.txt> <fresh-admit.txt> <per-target.json>

Prints three things, for the scheduled census to put in its summary:

* NEWLY ADMISSIBLE — passes every run now and is not admitted. A missed
  opportunity; regenerate the list to take it.
* NO LONGER PASSING — admitted, and the census says it would not pass. The
  gate lane itself goes red on these (W4); this names them a day sooner and
  with the census's reason.
* TIMEOUT — a test that hung instead of skipping. Always a defect, and the ONE
  outcome this exits non-zero on: everything else is information, and a census
  that failed on every new red would become a second gate nobody asked for.
"""
import json
import sys


def names(path):
    with open(path, encoding="utf8") as fh:
        return {l.split("#", 1)[0].strip() for l in fh if l.split("#", 1)[0].strip()}


def main() -> int:
    committed, fresh = names(sys.argv[1]), names(sys.argv[2])
    per = json.load(open(sys.argv[3]))
    new = sorted(fresh - committed)
    gone = sorted(committed - fresh)
    hung = sorted(t for t, v in per.items() if v["outcome"] == "TIMEOUT")

    def why(t):
        w = per.get(t, {}).get("why") or {}
        return max(w.items(), key=lambda kv: kv[1])[0] if w else "absent from the run"

    print(f"admitted {len(committed)}; the census would admit {len(fresh)}\n")
    print(f"NEWLY ADMISSIBLE ({len(new)})")
    for t in new:
        print(f"  {t}")
    print(f"\nNO LONGER PASSING ({len(gone)})")
    for t in gone:
        print(f"  {t:44s} {why(t)[:110]}")
    print(f"\nTIMEOUT ({len(hung)})")
    for t in hung:
        print(f"  {t}")
    if new or gone:
        print("\nRegenerate: scripts/test/lane-census.sh <gate-image> --runs 2 "
              "--admit .config/lane-admission/gate.txt")
    return 1 if hung else 0


if __name__ == "__main__":
    sys.exit(main())
