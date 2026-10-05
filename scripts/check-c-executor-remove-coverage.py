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

The coverage a Rust table cannot state, read from the C API's own sources.
EVERY `nros_executor_add_*` / `rclc_executor_add_*` FFI is read (issue 1668 —
until then only those whose SECOND parameter was an entity type were, so a verb
that took `*const nros_node_t` there and handed back nothing was invisible),
and each must be one of:

  * KEYED BY AN ENTITY — it takes a `*mut nros_<kind>_t`, and some
    `nros_executor_remove_*` takes that same type (or a row in `UNREMOVABLE`
    names the OPEN issue that tracks the gap);
  * KEYED BY A HANDLE — it writes a `*mut <x>_handle_t` out-parameter, and some
    `nros_executor_remove_*` takes that handle type (shutdown callbacks);
  * NOT AN ENTRY — a row in `NOT_AN_ENTRY` says what it configures instead,
    and its body never touches `handle_count`, which is the statically
    checkable half of "claims no executor handle";
  * SUPERSEDED — a row in `SUPERSEDED` names the sibling that registers the
    same entry removably; that sibling must itself be keyed and removable. The
    old verb stays for ABI (RFC-0054 is additive) and its entries live until
    `rclc_executor_fini` — the row is the record of that, not a pass.

A row in any table that no longer describes the code is stale and fails, so the
tables can only shrink.

The type is the key rather than the verb's name because there are seven
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

# `pub unsafe extern "C" fn <name>(executor: *mut nros_executor_t, ...) -> ...`
FFI = re.compile(
    r'pub unsafe extern "C" fn ((?:nros|rclc)_executor_(add|remove)_[a-z0-9_]+)\s*\(\s*'
    r"executor:\s*\*mut nros_executor_t\s*,?",
    re.S,
)
ENTITY = re.compile(r"^\*mut (nros_[a-z0-9_]+_t)$")
OUT_HANDLE = re.compile(r"^\*mut ([a-z0-9_]+_handle_t)$")
BY_VALUE_HANDLE = re.compile(r"^([a-z0-9_]+_handle_t)$")

# Entity type -> the OPEN issue that tracks its missing remover. A row is a debt
# with an owner, not a blessing; it must name an issue that is still open.
# Empty since issue 1631 added the last four removers; kept, with its stale and
# closed-issue checks, so a NEW add verb without a remover still has to name
# its owner here.
UNREMOVABLE: dict[str, str] = {}

# Add verb -> what it configures INSTEAD of registering an executor entry. The
# gate checks the body never touches `handle_count`.
NOT_AN_ENTRY: dict[str, str] = {
    "nros_executor_add_param_description": "parameter metadata in the node's param store",
    "nros_executor_add_param_description_on": "parameter metadata in the node's param store",
    "nros_executor_add_param_constraint_integer": "a range on a declared parameter",
    "nros_executor_add_param_constraint_integer_on": "a range on a declared parameter",
    "nros_executor_add_param_constraint_double": "a range on a declared parameter",
    "nros_executor_add_param_constraint_double_on": "a range on a declared parameter",
    "nros_executor_add_time_triggered_dispatcher": "the major-frame length of TT dispatch",
}

# Add verb -> the sibling that registers the same entry REMOVABLY.
SUPERSEDED: dict[str, str] = {
    # issue 1668 — direct-arg, no subscription object, nothing handed back.
    "nros_executor_add_subscription_raw_with_info": "nros_executor_add_subscription_with_info",
}


def split_params(text: str, start: int) -> tuple[list[tuple[str, str]], int]:
    """Parse `name: type, ...)` from `start` (just inside the `(`); return the
    params and the index past the closing `)`. Commas inside `<>`/`()` (an
    inline `Option<unsafe extern "C" fn(a, b)>`) do not split."""
    depth = 0
    cur = ""
    params: list[str] = []
    i = start
    while i < len(text):
        c = text[i]
        if c in "(<":
            depth += 1
        elif c in ")>" and not (c == ">" and text[i - 1] == "-"):
            if depth == 0 and c == ")":
                params.append(cur)
                i += 1
                break
            depth -= 1
        if c == "," and depth == 0:
            params.append(cur)
            cur = ""
        else:
            cur += c
        i += 1
    out = []
    for p in params:
        p = " ".join(p.split())
        if ":" in p:
            name, ty = p.split(":", 1)
            out.append((name.strip(), ty.strip()))
    return out, i


def body_of(text: str, after: int) -> str:
    """The function body: from the first `{` after the signature to the first
    column-0 `}` (rustfmt puts every top-level fn's closer there; a fn nested
    in a `mod` closes at its own indent, which `\\n\\s*}\\n` would also stop
    at, but the first column-0 one is the conservative bound)."""
    open_ = text.index("{", after)
    m = re.search(r"\n\s*\}\n", text[open_:])
    return text[open_ : open_ + m.end()] if m else text[open_:]


class Ffi:
    def __init__(self, name: str, verb: str, params: list[tuple[str, str]], body: str):
        self.name, self.verb, self.params, self.body = name, verb, params, body

    def entity(self) -> str | None:
        for _, ty in self.params:
            m = ENTITY.match(ty)
            if m and m.group(1) != "nros_executor_t" and not m.group(1).endswith("_handle_t"):
                return m.group(1)
        return None

    def out_handle(self) -> str | None:
        for _, ty in self.params:
            m = OUT_HANDLE.match(ty)
            if m:
                return m.group(1)
        return None

    def by_value_handle(self) -> str | None:
        for _, ty in self.params:
            m = BY_VALUE_HANDLE.match(ty)
            if m:
                return m.group(1)
        return None


def parse(sources: dict[str, str]) -> list[Ffi]:
    out: list[Ffi] = []
    for _, text in sorted(sources.items()):
        for m in FFI.finditer(text):
            params, end = split_params(text, m.end())
            out.append(Ffi(m.group(1), m.group(2), params, body_of(text, end)))
    return out


def scan(sources: dict[str, str]) -> tuple[dict[str, list[str]], dict[str, list[str]]]:
    """(entity type -> add FFIs, entity type -> remove FFIs)."""
    adds: dict[str, list[str]] = {}
    removes: dict[str, list[str]] = {}
    for f in parse(sources):
        ty = f.entity()
        if ty:
            (adds if f.verb == "add" else removes).setdefault(ty, []).append(f.name)
    return adds, removes


def open_issue_ids(issues_dir: Path) -> set[str]:
    return {
        p.name[:4]
        for p in issues_dir.glob("[0-9][0-9][0-9][0-9]-*.md")
        if re.search(r"^status:\s*open\s*$", p.read_text(encoding="utf-8"), re.M)
    }


def problems(
    sources: dict[str, str],
    table: dict[str, str],
    open_ids: set[str],
    not_an_entry: dict[str, str] | None = None,
    superseded: dict[str, str] | None = None,
) -> list[str]:
    not_an_entry = NOT_AN_ENTRY if not_an_entry is None else not_an_entry
    superseded = SUPERSEDED if superseded is None else superseded
    ffis = parse(sources)
    adds, removes = scan(sources)
    removed_handles = {
        h for f in ffis if f.verb == "remove" for h in [f.by_value_handle()] if h
    }
    by_name = {f.name: f for f in ffis if f.verb == "add"}
    out: list[str] = []

    def keyed_removable(f: Ffi) -> bool:
        ty = f.entity()
        if ty:
            return ty in removes
        h = f.out_handle()
        return bool(h) and h in removed_handles

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

    # Issue 1668 — the verbs keyed by no entity.
    for f in sorted(by_name.values(), key=lambda f: f.name):
        if f.entity():
            for t in (not_an_entry, superseded):
                if f.name in t:
                    out.append(
                        f"  `{f.name}` takes `{f.entity()}`, so its row in "
                        f"{'NOT_AN_ENTRY' if t is not_an_entry else 'SUPERSEDED'}\n"
                        f"      is stale. Delete the row."
                    )
            continue
        h = f.out_handle()
        if h:
            if h not in removed_handles:
                out.append(
                    f"  `{f.name}` hands back a `{h}` and NO `nros_executor_remove_*`\n"
                    f"      takes one. Add the remover."
                )
            continue
        if f.name in not_an_entry:
            if "handle_count" in f.body:
                out.append(
                    f"  `{f.name}` is listed in NOT_AN_ENTRY ({not_an_entry[f.name]})\n"
                    f"      but its body touches `handle_count` — it claims an executor\n"
                    f"      handle and hands the caller nothing to remove it by."
                )
            continue
        if f.name in superseded:
            sib = by_name.get(superseded[f.name])
            if sib is None:
                out.append(
                    f"  SUPERSEDED row `{f.name}` names `{superseded[f.name]}`, which no\n"
                    f"      `*_executor_add_*` defines."
                )
            elif not keyed_removable(sib):
                out.append(
                    f"  SUPERSEDED row `{f.name}` names `{sib.name}`, which is not\n"
                    f"      itself removable — a sibling that leaks too supersedes nothing."
                )
            continue
        out.append(
            f"  `{f.name}` takes no entity object and hands back no handle, so\n"
            f"      whatever it registers has nothing to be removed by (issue 1668).\n"
            f"      Give it a `*mut nros_<kind>_t` (or an out-handle) with a remover,\n"
            f"      or a row in NOT_AN_ENTRY (it registers no executor entry) or\n"
            f"      SUPERSEDED (a removable sibling registers the same entry)."
        )
    for t, label in ((not_an_entry, "NOT_AN_ENTRY"), (superseded, "SUPERSEDED")):
        for name in sorted(t):
            if name not in by_name:
                out.append(
                    f"  {label} row `{name}` names a verb no `*_executor_add_*` defines.\n"
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_add_hook(
    executor: *mut nros_executor_t,
    callback: Option<unsafe extern "C" fn(a: u8, b: u8)>,
    out_handle: *mut nros_hook_handle_t,
) -> nros_ret_t {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_remove_hook(
    executor: *mut nros_executor_t,
    handle: nros_hook_handle_t,
) -> nros_ret_t {
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_add_knob(
    executor: *mut nros_executor_t,
    value: u32,
) -> nros_ret_t {
    0
}
"""

# The 1668 shape: a node, strings and a callback — a handle is claimed and
# nothing comes back.
LEAKY = """
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_add_widget_direct(
    executor: *mut nros_executor_t,
    node: *const nros_node_t,
    topic: *const c_char,
) -> nros_ret_t {
    (*executor).handle_count += 1;
    0
}
"""


def self_test() -> None:
    """Negative controls, on the normal path — a gate nobody has seen fail is a
    comment."""
    table = {"nros_gadget_t": "9999"}
    knob = {"nros_executor_add_knob": "a setting"}
    ok = problems({"a.rs": GOOD}, table, {"9999"}, knob, {})
    assert not ok, f"{GATE} selftest: the compliant shape must pass, got {ok}"

    # 1. An add with no remover and no row — issue 1609's own starting state.
    bare = problems({"a.rs": GOOD}, {}, {"9999"}, knob, {})
    assert any("NO `nros_executor_remove_*`" in p for p in bare), (
        f"{GATE} selftest: an unremovable type with no row must fail, got {bare}"
    )

    # 2. A row whose remover has since landed — the table must shrink.
    stale = problems({"a.rs": GOOD}, {**table, "nros_widget_t": "9999"}, {"9999"}, knob, {})
    assert any("is stale" in p for p in stale), (
        f"{GATE} selftest: a row for a removable type must fail, got {stale}"
    )

    # 3. A row naming a closed issue — a debt with no owner.
    closed = problems({"a.rs": GOOD}, table, set(), knob, {})
    assert any("not an\n      OPEN issue" in p for p in closed), (
        f"{GATE} selftest: a row naming a closed issue must fail, got {closed}"
    )

    # 4. A row naming a type nothing registers — the inert-entry shape (0743).
    ghost = problems({"a.rs": GOOD}, {**table, "nros_ghost_t": "9999"}, {"9999"}, knob, {})
    assert any("no `*_executor_add_*` registers" in p for p in ghost), (
        f"{GATE} selftest: a row for an unregistered type must fail, got {ghost}"
    )

    # 5. The multi-line signature is read (rustfmt splits every real one).
    adds, removes = scan({"a.rs": GOOD})
    assert adds.get("nros_widget_t") and removes.get("nros_widget_t"), (
        f"{GATE} selftest: the split-signature form must parse, got {adds} / {removes}"
    )

    # 6. Issue 1668 — an add keyed by nothing, with no row, fails. This is the
    #    pre-1668 tree's `nros_executor_add_subscription_raw_with_info`.
    leak = problems({"a.rs": GOOD + LEAKY}, table, {"9999"}, knob, {})
    assert any("nros_executor_add_widget_direct` takes no entity" in p for p in leak), (
        f"{GATE} selftest: a handle-less add with no row must fail, got {leak}"
    )

    # 7. ...and calling it NOT_AN_ENTRY does not launder it: it claims a handle.
    laundered = problems(
        {"a.rs": GOOD + LEAKY}, table, {"9999"},
        {**knob, "nros_executor_add_widget_direct": "x"}, {},
    )
    assert any("touches `handle_count`" in p for p in laundered), (
        f"{GATE} selftest: NOT_AN_ENTRY on a handle-claiming add must fail, got {laundered}"
    )

    # 8. SUPERSEDED by a removable sibling passes; by a leaky one, fails.
    sup = problems(
        {"a.rs": GOOD + LEAKY}, table, {"9999"}, knob,
        {"nros_executor_add_widget_direct": "nros_executor_add_widget"},
    )
    assert not sup, f"{GATE} selftest: superseded by a removable sibling must pass, got {sup}"
    sup_bad = problems(
        {"a.rs": GOOD + LEAKY}, table, {"9999"}, knob,
        {"nros_executor_add_widget_direct": "nros_executor_add_knob"},
    )
    assert any("not\n      itself removable" in p for p in sup_bad), (
        f"{GATE} selftest: superseded by an unremovable sibling must fail, got {sup_bad}"
    )

    # 9. An out-handle whose remover is missing fails.
    no_hook_remover = GOOD.replace("nros_executor_remove_hook", "nros_executor_drop_hook")
    hook = problems({"a.rs": no_hook_remover}, table, {"9999"}, knob, {})
    assert any("hands back a `nros_hook_handle_t`" in p for p in hook), (
        f"{GATE} selftest: an out-handle with no remover must fail, got {hook}"
    )

    # 10. A row naming no verb is stale.
    ghost_verb = problems(
        {"a.rs": GOOD}, table, {"9999"}, {**knob, "nros_executor_add_ghost": "x"}, {}
    )
    assert any("names a verb no" in p for p in ghost_verb), (
        f"{GATE} selftest: a NOT_AN_ENTRY row for a missing verb must fail, got {ghost_verb}"
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
    add_verbs = [f for f in parse(sources) if f.verb == "add"]
    if not require_population(len(adds), "registered entity type(s)", gate=GATE):
        return 1
    found = problems(sources, UNREMOVABLE, open_issue_ids(ISSUES))
    if found:
        print(f"{GATE}: FAIL")
        for p in found:
            print(p)
        return 1
    print(
        f"{GATE}: OK — {len(add_verbs)} add verb(s) read; {len(adds)} registered "
        f"entity type(s): {len(removes)} removable, {len(UNREMOVABLE)} tracked as "
        f"debt; {len(NOT_AN_ENTRY)} register no entry, {len(SUPERSEDED)} superseded "
        f"by a removable sibling."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
