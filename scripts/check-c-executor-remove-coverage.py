#!/usr/bin/env python3
"""Issue 1609 — every entity the C executor can ADD, it can REMOVE (or says why not).

The C twin of `check-cpp-destroy-shape` (issue 1496), for the C surface's own
shape of the same defect. `nros_executor_add_<kind>(executor, entity)` registers
an arena entry; the entity's `fini` is handed only the entity, so it cannot
reach the executor's `handle_count`, its trigger-entity table, or the Rust arena
entry — and for the kinds whose entry's callback `context` IS the C struct
(action server, action client, service client) a `fini`'d entity keeps being
dispatched into. The answer is an executor-side removal verb in rclc's shape,
`nros_executor_remove_<kind>(executor, entity)`, and issue 1609 added the first
two.

WHAT THIS GATE HOLDS

The coverage a Rust table cannot state, read from the C API's own sources:

  * every entity TYPE some `nros_executor_add_*` / `rclc_executor_add_*` FFI
    registers (its second parameter, `*mut nros_<kind>_t`) has a
    `nros_executor_remove_*` FFI taking that same type, OR a row in
    `UNREMOVABLE` naming the OPEN issue that tracks the gap;
  * an `UNREMOVABLE` row whose type HAS a remover, or that no add registers,
    is stale and fails — so the table can only shrink as removers land.

The type is the key rather than the verb's name because there are six
`add_subscription*` spellings and one entity; a remover is owed per entity.

Run: python3 scripts/check-c-executor-remove-coverage.py
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "scripts" / "lib"))
from population import require_population  # noqa: E402  phase-472 W4

SRC = REPO / "packages" / "api" / "nros-c" / "src"
ISSUES = REPO / "docs" / "issues"
GATE = "check-c-executor-remove-coverage"

# `pub unsafe extern "C" fn <name>(executor: *mut nros_executor_t, <x>: *mut <T>`
FFI = re.compile(
    r'pub unsafe extern "C" fn ((?:nros|rclc)_executor_(add|remove)_[a-z0-9_]+)\s*\(\s*'
    r"executor:\s*\*mut nros_executor_t,\s*[a-z_]+:\s*\*mut (nros_[a-z0-9_]+_t)\b",
    re.S,
)

# Entity type -> the OPEN issue that tracks its missing remover. A row is a debt
# with an owner, not a blessing; it must name an issue that is still open.
# Empty since issue 1631 added the last four removers; kept, with its stale and
# closed-issue checks, so a NEW add verb without a remover still has to name
# its owner here.
UNREMOVABLE: dict[str, str] = {}


def scan(sources: dict[str, str]) -> tuple[dict[str, list[str]], dict[str, list[str]]]:
    """(entity type -> add FFIs, entity type -> remove FFIs)."""
    adds: dict[str, list[str]] = {}
    removes: dict[str, list[str]] = {}
    for _, text in sorted(sources.items()):
        for m in FFI.finditer(text):
            fn, verb, ty = m.group(1), m.group(2), m.group(3)
            (adds if verb == "add" else removes).setdefault(ty, []).append(fn)
    return adds, removes


def open_issue_ids(issues_dir: Path) -> set[str]:
    return {
        p.name[:4]
        for p in issues_dir.glob("[0-9][0-9][0-9][0-9]-*.md")
        if re.search(r"^status:\s*open\s*$", p.read_text(encoding="utf-8"), re.M)
    }


def problems(
    sources: dict[str, str], table: dict[str, str], open_ids: set[str]
) -> list[str]:
    adds, removes = scan(sources)
    out: list[str] = []
    for ty in sorted(adds):
        if ty in removes:
            continue
        if ty not in table:
            out.append(
                f"  `{ty}` is registered by {', '.join(sorted(adds[ty]))}\n"
                f"      and NO `nros_executor_remove_*` takes it. Its `fini` cannot\n"
                f"      reach the executor, so the arena entry, `handle_count` and the\n"
                f"      trigger table outlive it (issue 1609). Add the remover, or a\n"
                f"      row in UNREMOVABLE naming the open issue that tracks it."
            )
        elif table[ty] not in open_ids:
            out.append(
                f"  UNREMOVABLE row `{ty}` names issue {table[ty]}, which is not an\n"
                f"      OPEN issue in docs/issues/. A debt row needs a live owner."
            )
    for ty in sorted(table):
        if ty in removes:
            out.append(
                f"  UNREMOVABLE row `{ty}` is stale: {', '.join(sorted(removes[ty]))}\n"
                f"      removes it now. Delete the row."
            )
        elif ty not in adds:
            out.append(
                f"  UNREMOVABLE row `{ty}` names a type no `*_executor_add_*` registers.\n"
                f"      Delete the row, or fix the spelling."
            )
    return out


GOOD = """
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_add_widget(
    executor: *mut nros_executor_t,
    widget: *mut nros_widget_t,
) -> nros_ret_t { 0 }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_add_widget_sized(executor: *mut nros_executor_t, widget: *mut nros_widget_t, n: usize) -> nros_ret_t { 0 }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_remove_widget(
    executor: *mut nros_executor_t,
    widget: *mut nros_widget_t,
) -> nros_ret_t { 0 }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_executor_add_gadget(
    executor: *mut nros_executor_t,
    gadget: *mut nros_gadget_t,
) -> nros_ret_t { 0 }
"""


def self_test() -> None:
    """Negative controls, on the normal path — a gate nobody has seen fail is a
    comment."""
    table = {"nros_gadget_t": "9999"}
    ok = problems({"a.rs": GOOD}, table, {"9999"})
    assert not ok, f"{GATE} selftest: the compliant shape must pass, got {ok}"

    # 1. An add with no remover and no row — issue 1609's own starting state.
    bare = problems({"a.rs": GOOD}, {}, {"9999"})
    assert any("NO `nros_executor_remove_*`" in p for p in bare), (
        f"{GATE} selftest: an unremovable type with no row must fail, got {bare}"
    )

    # 2. A row whose remover has since landed — the table must shrink.
    stale = problems({"a.rs": GOOD}, {**table, "nros_widget_t": "9999"}, {"9999"})
    assert any("is stale" in p for p in stale), (
        f"{GATE} selftest: a row for a removable type must fail, got {stale}"
    )

    # 3. A row naming a closed issue — a debt with no owner.
    closed = problems({"a.rs": GOOD}, table, set())
    assert any("not an\n      OPEN issue" in p for p in closed), (
        f"{GATE} selftest: a row naming a closed issue must fail, got {closed}"
    )

    # 4. A row naming a type nothing registers — the inert-entry shape (0743).
    ghost = problems({"a.rs": GOOD}, {**table, "nros_ghost_t": "9999"}, {"9999"})
    assert any("no `*_executor_add_*` registers" in p for p in ghost), (
        f"{GATE} selftest: a row for an unregistered type must fail, got {ghost}"
    )

    # 5. The multi-line signature is read (rustfmt splits every real one).
    adds, removes = scan({"a.rs": GOOD})
    assert adds.get("nros_widget_t") and removes.get("nros_widget_t"), (
        f"{GATE} selftest: the split-signature form must parse, got {adds} / {removes}"
    )


def tracked_rust_sources() -> list[Path]:
    """Every tracked `.rs` under `SRC`, by index lookup (issue 0844's rule)."""
    r = subprocess.run(
        ["git", "-C", str(REPO), "ls-files", "--", SRC.relative_to(REPO).as_posix()],
        capture_output=True,
        text=True,
        check=False,
    )
    if r.returncode != 0:
        sys.exit(f"{GATE}: `git ls-files` failed:\n  {r.stderr.strip()}")
    return sorted(
        REPO / rel
        for rel in (x.strip() for x in r.stdout.splitlines())
        if rel.endswith(".rs")
    )


def main() -> int:
    self_test()
    sources = {p.name: p.read_text(encoding="utf-8") for p in tracked_rust_sources()}
    adds, removes = scan(sources)
    if not require_population(len(adds), "registered entity type(s)", gate=GATE):
        return 1
    found = problems(sources, UNREMOVABLE, open_issue_ids(ISSUES))
    if found:
        print(f"{GATE}: FAIL")
        for p in found:
            print(p)
        return 1
    print(
        f"{GATE}: OK — {len(adds)} registered entity type(s): "
        f"{len(removes)} removable, {len(UNREMOVABLE)} tracked as debt."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
