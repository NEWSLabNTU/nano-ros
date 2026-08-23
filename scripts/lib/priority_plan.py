"""RFC-0079 priority address plans — ONE reader for both consumers.

`check-tier-priority-plan.py` enforces plans; `dev/priority-collision-report.py`
reports on them. They had a table each for about an hour, which is the second
spelling this codebase keeps paying for — and the plan tables are precisely the
thing whose whole value is being the single place a band is written down.
"""

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from tracked import tracked  # issue 0721: index lookup, not a walk

ROOT = Path(__file__).resolve().parent.parent.parent

TIER_PLAT_RE = re.compile(r"^\[tiers\.([A-Za-z0-9_]+)\.([A-Za-z0-9_]+)\]\s*$")
TIER_RE = re.compile(r"^\[tiers\.([A-Za-z0-9_]+)\]\s*$")
PRIO_RE = re.compile(r"^priority\s*=\s*(-?\d+)")
PAIR_RE = re.compile(r"\[\s*(-?\d+)\s*,\s*(-?\d+)\s*\]")


def load_plans():
    """platform -> plan, from every board descriptor that declares one."""
    plans = {}
    for desc in sorted(tracked(ROOT / "packages/boards", name="nros-board.toml")):
        text = desc.read_text(encoding="utf-8")
        if "[board.priority_plan]" not in text:
            continue
        platform, in_plan = None, False
        plan = {"reserved": {}, "pool": {}, "source": desc.relative_to(ROOT)}
        for raw in text.splitlines():
            line = raw.split("#", 1)[0].strip()
            if not line:
                continue
            if line.startswith("platform =") and platform is None:
                platform = line.split("=", 1)[1].strip().strip('"')
            if line == "[board.priority_plan]":
                in_plan = True
                continue
            if in_plan and line.startswith("["):
                in_plan = False
            if not in_plan:
                continue
            if line.startswith("direction"):
                plan["direction"] = line.split("=", 1)[1].strip().strip('"')
            elif line.startswith("range"):
                m = PAIR_RE.search(line)
                plan["range"] = (int(m.group(1)), int(m.group(2)))
            elif line.startswith("reserved."):
                name = line.split("=", 1)[0].strip().split(".", 1)[1]
                m = PAIR_RE.search(line)
                plan["reserved"][name] = (int(m.group(1)), int(m.group(2)))
            elif line.startswith("pool."):
                name = line.split("=", 1)[0].strip().split(".", 1)[1]
                m = PAIR_RE.search(line)
                plan["pool"][name] = (int(m.group(1)), int(m.group(2)))
        if platform:
            plans[platform] = plan
    return plans



ABOVE_RE = re.compile(r'^above\s*=\s*"([A-Za-z0-9_]+)"')


def scan_pins():
    """-> [(path, tier, platform, priority, above)] over every system.toml.

    `above` is the band this tier DELIBERATELY outranks, declared on the tier
    (RFC-0079 §6) and inherited by each of its per-platform pins:

        [tiers.safety]
        above = "transport"      # states the choice once, for every port

    It sits on the tier rather than the platform table because it is a
    statement about the SYSTEM, not about one kernel's numbering — the same
    reason the timing contract lives there.
    """
    pins = []
    for f in sorted(tracked(ROOT / "examples", name="system.toml")):
        tier = plat = None
        above = {}
        for raw in f.read_text(encoding="utf-8").splitlines():
            line = raw.strip()
            if not line or line.startswith("#"):
                continue
            m = TIER_PLAT_RE.match(line)
            if m:
                tier, plat = m.group(1), m.group(2)
                continue
            m = TIER_RE.match(line)
            if m:
                tier, plat = m.group(1), None
                continue
            if line.startswith("["):
                tier = plat = None
                continue
            m = ABOVE_RE.match(line)
            if m and tier and plat is None:
                above[tier] = m.group(1)
                continue
            m = PRIO_RE.match(line)
            if m and tier and plat:
                pins.append((f.relative_to(ROOT), tier, plat,
                             int(m.group(1)), above.get(tier)))
    return pins
