"""RFC-0114 D10/D11 — the board support registry's evidence primitives.

One reader of `packages/boards/board-support.toml` for the fields phase-522 W1
added (`runs`, `emulator`, `hardware`, `cadence`, `last_run`, `infra`,
`breadth`, `external`) and the one derivation of a row's STATUS WORD from them.
The status word is never typed in the registry; every view that shows one (the
gate's `--print-status`, and from phase-522 W2 the generated book pages and
`nros board list`) asks `status_word` here, so two views cannot disagree.

The predicates are written against inputs the CALLER measured (which platforms
have Runtime cells, which have any build evidence, which nightly modules the
sweep runs), so this module reads no file except the registry and has a
self-test that needs none.

One evidence field is not typed here at all: `emulator` is read from the board
descriptor's `[board.run]` (RFC-0118 D4, amending RFC-0114 D10; phase-528 W2),
because a board outside the tree has no registry row and still needs to run,
and RFC-0114 D12 types no board fact twice. `descriptor_index` reads the
tracked descriptors, `with_descriptor_facts` fills each row's `emulator` from
its descriptor, and `typed_emulator_errors` refuses a row that still types one
while an in-tree descriptor exists for it. Only a row with no in-tree
descriptor (an `external` board) may type `emulator`.
"""

from __future__ import annotations

import copy
import datetime
import re
import subprocess
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # 3.10 backport, same spelling as the sibling gates
    import tomli as tomllib

RUNS = ("none", "host", "emulator", "hardware")
CADENCES = ("merge", "daily", "weekly", "manual")
EMULATOR_KINDS = ("qemu", "renode", "wokwi", "fvp")
EVIDENCE_FIELDS = ("runs", "emulator", "hardware", "cadence", "last_run", "breadth")
DEFAULT_STALE_DAYS = 120


def load(path: Path) -> dict:
    """The registry as TOML, read by a real TOML parser."""
    with open(path, "rb") as fh:
        return tomllib.load(fh)


def dumps(doc: dict) -> str:
    """A canonical TOML rendering of the registry's data (comments dropped).

    Only the shapes the registry uses: top-level scalars and arrays of tables
    (`[[board]]`, `[[probe]]`, `[[cell]]`) holding strings, integers, booleans and string arrays.
    Anything else raises, so a new shape cannot round-trip by accident.
    """
    def scalar(v):
        if isinstance(v, bool):
            return "true" if v else "false"
        if isinstance(v, int):
            return str(v)
        if isinstance(v, str):
            return '"' + v.replace("\\", "\\\\").replace('"', '\\"') + '"'
        if isinstance(v, list) and all(isinstance(x, str) for x in v):
            return "[" + ", ".join(scalar(x) for x in v) + "]"
        raise TypeError(f"registry value of unsupported shape: {v!r}")

    def is_tables(v):
        return isinstance(v, list) and v and all(isinstance(x, dict) for x in v)

    out = []
    for k, v in doc.items():
        if not is_tables(v):
            out.append(f"{k} = {scalar(v)}")
    for k, v in doc.items():
        if is_tables(v):
            for row in v:
                out.append("")
                out.append(f"[[{k}]]")
                for rk, rv in row.items():
                    out.append(f"{rk} = {scalar(rv)}")
    return "\n".join(out) + "\n"


def round_trip_errors(doc: dict) -> list[str]:
    """`load(dumps(doc)) == doc`, or why not."""
    try:
        again = tomllib.loads(dumps(doc))
    except (TypeError, tomllib.TOMLDecodeError) as exc:
        return [f"registry does not round-trip through the loader: {exc}"]
    if again != doc:
        return ["registry does not round-trip through the loader: the reloaded "
                "data differs from the first read"]
    return []


def row_name(row: dict) -> str:
    return f"{row.get('crate', '<no crate key>')}@{row.get('matrix_platform') or '-'}"


def board_name(row: dict) -> str:
    """The board name a fixture's `NROS_BOARD` uses for this row's package."""
    crate = row.get("crate", "")
    return crate[len("nros-board-"):] if crate.startswith("nros-board-") else crate


def is_stale(row: dict, stale_days: int, today: datetime.date) -> bool:
    if row.get("cadence") != "manual" or not row.get("last_run"):
        return False
    last = datetime.date.fromisoformat(row["last_run"])
    return (today - last).days > stale_days


def stale_from(row: dict, stale_days: int) -> datetime.date | None:
    """The first day a manual row reads "Builds", or None for any other row."""
    if row.get("cadence") != "manual" or not row.get("last_run"):
        return None
    return datetime.date.fromisoformat(row["last_run"]) + datetime.timedelta(
        days=stale_days + 1)


def status_word(row: dict, built: bool, stale_days: int = DEFAULT_STALE_DAYS,
                today: datetime.date | None = None) -> str:
    """RFC-0114 D10's status word for one row.

    `built` is whether the row has any build evidence: a Runtime or BuildOnly
    cell on its platform, or a fixture row whose `NROS_BOARD` names its board.
    A manual row older than `stale_days` on `today` degrades to "Builds" with
    its date — the row is not edited; the derivation reports what the evidence
    is worth that day. `today = None` asks no clock and never degrades: the
    committed views pass None so that a tracked file cannot change on a day
    nobody edited anything (they print the date the degradation starts instead,
    `stale_from`), and every view a reader sees passes the real date: `nros
    board`, `--print-status`, and the rendered book through
    `scripts/mdbook-board-status.py`.
    """
    if row.get("infra") is True:
        return "Infrastructure"
    runs = row.get("runs")
    cadence = row.get("cadence")
    if runs == "none":
        return "Builds" if built else "Not supported"
    if cadence == "manual":
        date = row.get("last_run", "?")
        if today is not None and is_stale(row, stale_days, today):
            return f"Builds (last booted {date})"
        if runs == "hardware":
            return f"Boots on hardware, manual ({date})"
        return f"Boots, manual ({date})"
    where = {"host": "on host", "emulator": "in emulator",
             "hardware": "on hardware"}.get(runs, f"({runs})")
    return f"Verified {where} ({cadence})"


def shape_errors(row: dict, platforms: set[str]) -> list[str]:
    """Field presence and value rules of D10/D11, independent of evidence."""
    where = row_name(row)
    errs = []
    if "infra" in row:
        if row["infra"] is not True:
            errs.append(f"{where}: `infra` is either `true` or absent")
        extra = [k for k in EVIDENCE_FIELDS if k in row]
        if extra:
            errs.append(f"{where}: an infra row carries no evidence field, and this "
                        f"one has {', '.join(extra)}")
        return errs

    runs = row.get("runs")
    if runs not in RUNS:
        errs.append(f"{where}: `runs` must be one of {', '.join(RUNS)} "
                    f"(got {runs!r}); every non-infra row states where its image "
                    "executes, or `none`")
        return errs
    emu = row.get("emulator")
    if emu is not None:
        kind = emu.split(":", 1)[0]
        if ":" not in emu or kind not in EMULATOR_KINDS:
            errs.append(f"{where}: `emulator = {emu!r}` must read <kind>:<machine> "
                        f"with kind one of {', '.join(EMULATOR_KINDS)}")
    if runs == "emulator" and not emu:
        errs.append(f"{where}: runs = \"emulator\" needs `emulator` naming the machine")
    if runs == "hardware" and not row.get("hardware"):
        errs.append(f"{where}: runs = \"hardware\" needs `hardware` naming the board")

    cadence = row.get("cadence")
    if runs == "none":
        if cadence is not None:
            errs.append(f"{where}: runs = \"none\" has no run, so no `cadence`")
    elif cadence not in CADENCES:
        errs.append(f"{where}: `cadence` must be one of {', '.join(CADENCES)} "
                    f"(got {cadence!r})")
    if cadence == "manual":
        try:
            datetime.date.fromisoformat(row.get("last_run", ""))
        except ValueError:
            errs.append(f"{where}: cadence = \"manual\" needs `last_run` as an ISO "
                        f"date (got {row.get('last_run')!r})")
    elif "last_run" in row:
        errs.append(f"{where}: `last_run` is only for cadence = \"manual\"; a lane's "
                    "last run is a question for the lane, not the registry")

    breadth = row.get("breadth")
    if breadth is None and runs == "none":
        # A board nothing boots has no breadth to claim yet; `nros board new`
        # starts a row here. Stating one is still allowed (the twin a row will
        # inherit from once it runs).
        return errs
    m = re.fullmatch(r"inherits (\w+)", breadth or "")
    if breadth != "witness" and not m:
        errs.append(f"{where}: `breadth` must be \"witness\" or \"inherits "
                    f"<PlatformId>\" (got {breadth!r})")
    elif m and m.group(1) not in platforms:
        errs.append(f"{where}: breadth inherits {m.group(1)!r}, which is not a "
                    "PlatformId variant")
    return errs


def evidence_errors(row: dict, *, runtime: set[str], nightly: set[str], probe_tracks: set[str]) -> list[str]:
    """D10's gate predicates against the measured evidence."""
    if row.get("infra") is True or row.get("runs") not in RUNS:
        return []
    where = row_name(row)
    plat = row.get("matrix_platform")
    runs, cadence = row["runs"], row.get("cadence")
    errs = []
    # A manual run is recorded by its date, not by a cell: the FVP's runtime
    # cell is `Gated` and the island has no PlatformId at all. Every automated
    # cadence must have a cell that asserts the run.
    if runs != "none" and cadence != "manual" and (not plat or plat not in runtime):
        errs.append(f"{where}: runs = {runs!r} at cadence {cadence!r} needs at least "
                    f"one Runtime cell for its platform ({plat or 'none named'})")
    if runs == "none" and plat and plat in runtime:
        errs.append(f"{where}: runs = \"none\" but platform {plat} has Runtime cells; "
                    "a board those cells boot does run")
    if cadence == "merge" and runs != "host":
        errs.append(f"{where}: cadence = \"merge\" needs a host executable; the merge "
                    "lane is host-only, so a cross-run platform's run is nightly")
    if cadence == "daily":
        tok = row.get("nightly_token")
        if not tok or tok not in nightly:
            errs.append(f"{where}: cadence = \"daily\" needs a `nightly_token` the "
                        f"nightly sweep runs (token {tok!r}, sweep {sorted(nightly)})")
    if cadence == "weekly":
        named = set(row.get("probe") or [])
        if not named or not named <= probe_tracks:
            errs.append(f"{where}: cadence = \"weekly\" needs `probe` naming tracks of "
                        f"the weekly probe workflow (named {sorted(named)}, workflow "
                        f"{sorted(probe_tracks)})")
    return errs


# --------------------------------------------------------------------------
# descriptor-derived facts (RFC-0118 D4, phase-528 W2)

def descriptor_index(root: Path) -> dict:
    """Every tracked `[[board]]` descriptor entry, by name and by directory.

    `{"by_name": {name: (rel, i)}, "by_dir": {dir: [(rel, i), ...]},
    "entries": {(rel, i): entry}}` — `rel` is the descriptor's path, `i` the
    entry's index in its `[[board]]` array. Tracked files only, at any depth
    (a conf bundle sits under its family package).
    """
    tracked = subprocess.run(["git", "ls-files", "-z", "--", "packages/boards/**nros-board.toml"],
                             cwd=root, capture_output=True, text=True, check=True).stdout
    index: dict = {"by_name": {}, "by_dir": {}, "entries": {}}
    for rel in sorted(p for p in tracked.split("\0") if p):
        boards = load(root / rel).get("board", [])
        boards = boards if isinstance(boards, list) else [boards]
        for i, entry in enumerate(boards):
            key = (rel, i)
            index["entries"][key] = entry
            index["by_dir"].setdefault(str(Path(rel).parent), []).append(key)
            for n in entry.get("names") or []:
                index["by_name"].setdefault(n, key)
    return index


def row_descriptor(row: dict, index: dict) -> tuple[str, int] | None:
    """The descriptor entry a row's evidence is for, or None.

    `board_keys` names it when present (its first key; `descriptor_errors`
    holds every key to the same entry). Otherwise the row's package directory
    `packages/boards/<crate>/` holding exactly one `[[board]]`. Infra and
    external rows have none.
    """
    if row.get("infra") or row.get("external"):
        return None
    keys = row.get("board_keys") or []
    if keys:
        return index["by_name"].get(keys[0])
    entries = index["by_dir"].get(f"packages/boards/{row.get('crate', '')}", [])
    return entries[0] if len(entries) == 1 else None


def descriptor_emulator(index: dict, key: tuple[str, int] | None) -> str | None:
    if key is None:
        return None
    run = index["entries"][key].get("run")
    return run.get("emulator") if isinstance(run, dict) else None


def with_descriptor_facts(doc: dict, index: dict) -> dict:
    """A copy of the registry with each row's `emulator` read from its
    descriptor's `[board.run]`. A row's own typed value (refused by
    `typed_emulator_errors` when a descriptor exists) is kept only for a row
    with no in-tree descriptor."""
    out = copy.deepcopy(doc)
    for row in out.get("board", []):
        key = row_descriptor(row, index)
        if key is None:
            continue
        row.pop("emulator", None)
        emu = descriptor_emulator(index, key)
        if emu is not None:
            row["emulator"] = emu
    return out


def typed_emulator_errors(rows: list[dict], index: dict) -> list[str]:
    """A row types `emulator` only when no in-tree descriptor can state it.

    Run against the RAW registry rows (before `with_descriptor_facts`)."""
    errs = []
    for row in rows:
        where = row_name(row)
        keys = row.get("board_keys") or []
        owners = {index["by_name"][k] for k in keys if k in index["by_name"]}
        if len(owners) > 1:
            errs.append(f"{where}: board_keys {keys} name {len(owners)} different descriptor "
                        "entries; a row's evidence is for one board")
        if "emulator" not in row:
            continue
        key = row_descriptor(row, index)
        if key is None:
            continue
        rel = key[0]
        stated = descriptor_emulator(index, key)
        if stated == row["emulator"]:
            errs.append(f"{where}: `emulator = {row['emulator']!r}` duplicates {rel}'s "
                        "`[board.run] emulator`; delete it from the registry row, which "
                        "reads it from the descriptor (RFC-0118 D4)")
        else:
            errs.append(f"{where}: `emulator = {row['emulator']!r}` is typed in the registry, "
                        f"but the board's descriptor {rel} "
                        + (f"states {stated!r}" if stated else "states none")
                        + ". The emulator is a descriptor fact: state it in that file's "
                        "`[board.run] emulator` and delete it here (RFC-0118 D4)")
    return errs


def self_test() -> None:
    """Negative controls: each rule fires on a row built to break it."""
    today = datetime.date(2026, 11, 1)
    plats = {"Linux", "ZephyrQemuCortexM", "Fvp"}
    ok = {"crate": "b", "matrix_platform": "Linux", "runs": "host",
          "cadence": "merge", "breadth": "witness"}
    assert not shape_errors(ok, plats)
    assert status_word(ok, True, today=today) == "Verified on host (merge)"

    assert shape_errors({**ok, "runs": "qemu"}, plats), "unknown runs must fail"
    assert shape_errors({"crate": "e", "runs": "emulator", "cadence": "daily",
                         "breadth": "witness"}, plats), "emulator needs a machine"
    assert shape_errors({**ok, "emulator": "qemu"}, plats), "emulator needs kind:machine"
    assert shape_errors({**ok, "last_run": "2026-01-01"}, plats), \
        "last_run is refused off manual"
    assert shape_errors({**ok, "cadence": "manual"}, plats), "manual needs last_run"
    assert shape_errors({**ok, "breadth": "inherits Nowhere"}, plats), \
        "breadth must inherit a real platform"
    assert shape_errors({"crate": "i", "infra": True, "runs": "host"}, plats), \
        "infra carries no evidence field"
    assert not shape_errors({"crate": "n", "runs": "none"}, plats), \
        "a new board (runs none, no breadth) must pass"
    assert shape_errors({**ok, "breadth": None}, plats) or \
        shape_errors({k: v for k, v in ok.items() if k != "breadth"}, plats), \
        "a row that runs must state its breadth"
    assert shape_errors({"crate": "n", "runs": "none", "cadence": "daily",
                         "breadth": "witness"}, plats), "runs none has no cadence"

    ev = dict(runtime={"Linux"}, nightly={"zephyr"}, probe_tracks={"zephyr"})
    assert not evidence_errors(ok, **ev)
    assert evidence_errors({**ok, "matrix_platform": "Fvp"}, **ev), \
        "an automated run needs a Runtime cell"
    assert evidence_errors({**ok, "runs": "none", "cadence": None}, **ev), \
        "runs none with Runtime cells must fail"
    assert evidence_errors({**ok, "runs": "emulator", "emulator": "qemu:x"}, **ev), \
        "merge cadence is host-only"
    assert evidence_errors({**ok, "cadence": "daily", "nightly_token": "nope"}, **ev), \
        "daily needs a swept nightly token"
    assert evidence_errors({**ok, "cadence": "weekly"}, **ev), "weekly needs a probe track"

    fvp = {"crate": "z", "matrix_platform": "Fvp", "runs": "emulator",
           "emulator": "fvp:M", "cadence": "manual", "last_run": "2026-09-06",
           "breadth": "inherits ZephyrQemuCortexM"}
    assert not shape_errors(fvp, plats) and not evidence_errors(fvp, **ev)
    # RFC-0114 evaluation step 5, both directions.
    assert status_word(fvp, True, stale_days=120, today=today) == "Boots, manual (2026-09-06)"
    assert status_word(fvp, True, stale_days=30, today=today) == \
        "Builds (last booted 2026-09-06)"
    hw = {**fvp, "runs": "hardware", "hardware": "h"}
    assert status_word(fvp, True, stale_days=1) == "Boots, manual (2026-09-06)", \
        "with no clock asked, a view never degrades"
    assert stale_from(fvp, 120) == datetime.date(2027, 1, 5)
    assert status_word(fvp, True, stale_days=120, today=datetime.date(2027, 1, 4)) == \
        "Boots, manual (2026-09-06)"
    assert status_word(fvp, True, stale_days=120, today=datetime.date(2027, 1, 5)) == \
        "Builds (last booted 2026-09-06)"
    assert status_word(hw, False, today=today) == "Boots on hardware, manual (2026-09-06)"
    none = {"crate": "n", "runs": "none", "breadth": "witness"}
    assert status_word(none, True) == "Builds"
    assert status_word(none, False) == "Not supported"
    assert status_word({"crate": "i", "infra": True}, False) == "Infrastructure"

    # phase-528 W2 — descriptor-derived `emulator`, each refusal planted.
    index = {"by_name": {"q": ("d/q.toml", 0), "r": ("d/r.toml", 0)},
             "by_dir": {"packages/boards/nros-board-q": [("d/q.toml", 0)]},
             "entries": {("d/q.toml", 0): {"names": ["q"], "run": {"emulator": "qemu:m"}},
                         ("d/r.toml", 0): {"names": ["r"]}}}
    q = {"crate": "nros-board-q", "runs": "emulator", "cadence": "daily",
         "breadth": "witness"}
    derived = with_descriptor_facts({"board": [q]}, index)["board"][0]
    assert derived["emulator"] == "qemu:m", "the row reads the descriptor's emulator"
    assert "emulator" not in q, "the raw row is not mutated"
    assert not typed_emulator_errors([q], index)
    assert typed_emulator_errors([{**q, "emulator": "qemu:m"}], index), \
        "a typed emulator duplicating the descriptor's is refused"
    assert typed_emulator_errors([{**q, "emulator": "qemu:other"}], index), \
        "a typed emulator the descriptor does not state is refused"
    rr = {"crate": "x", "board_keys": ["r"], "runs": "emulator", "emulator": "qemu:m"}
    assert typed_emulator_errors([rr], index), \
        "a typed emulator on a row whose descriptor states none is refused"
    assert shape_errors(with_descriptor_facts({"board": [{**rr, "cadence": "daily",
                                                           "breadth": "witness"}]},
                                              index)["board"][0], plats), \
        "runs = emulator whose descriptor states none needs one"
    ext = {"crate": "e", "external": "elsewhere", "runs": "emulator", "emulator": "qemu:m"}
    assert not typed_emulator_errors([ext], index), "an external row may type it"
    assert typed_emulator_errors([{"crate": "y", "board_keys": ["q", "r"]}], index), \
        "board_keys spanning two descriptors are refused"

    doc = {"manual_stale_days": 120, "board": [ok, {"crate": "i", "infra": True,
                                                    "maintainers": ["a"]}]}
    assert not round_trip_errors(doc)
    assert round_trip_errors({"board": [{"crate": "x", "bad": 1.5}]}), \
        "an unsupported value shape must not round-trip silently"
