#!/usr/bin/env python3
"""Issue 1496 — a `nros_cpp_*_destroy` that releases NOTHING must say so.

Every one of these functions is a null guard plus one `core::ptr::drop_in_place`,
which reads as a release. For three of the nine it is not: `CppActionServer`,
`CppActionClient` and `nros_node::GuardCondition` have no drop glue at all, and
what they appear to release — an action's five RMW entities, its goal table and
result slab; a guard condition's flag and closure entry — lives in an entry in the
executor arena, which is a BUMP ALLOCATOR with no removal path. Issue 1496 is
what that cost: a destructor shaped like a release that releases nothing, and
whose arena entry kept dispatching goals through the destroyed object's storage.

WHY THE CLASSIFICATION IS NOT IN THIS FILE

"Does this `drop_in_place` run anything?" is a question about the TYPE.
Re-deriving drop glue from the struct definition in a checker would be a second,
worse implementation of something the compiler already decides, so the
classification lives in `packages/api/nros-cpp/src/destroy_shape.rs` as
`needs_drop` asserts that FAIL THE BUILD when a type's answer moves.

What a Rust file cannot state is coverage: nothing in that table forces a NEW
`*_destroy` to appear in it, which is exactly the shape issue 0196 keeps
producing. So this gate reads both sides —

  * every `nros_cpp_*destroy*` FFI in the crate has a row;
  * every row names a real function, and the type the row claims is the type
    that function actually drops;
  * a `NO_OP` row's function documents the consequence, naming issue 1496.

A row is not a blessing. It records which of the two a function is; if the answer
is `NO_OP`, the documentation has to carry that to whoever calls it.

Run: python3 scripts/check-cpp-destroy-shape.py
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "scripts" / "lib"))
from population import require_population  # noqa: E402  phase-472 W4
SRC = REPO / "packages" / "api" / "nros-cpp" / "src"
TABLE = SRC / "destroy_shape.rs"

GATE = "check-cpp-destroy-shape"

# An exported destroy entry point. `destroy_polling` is in scope too: it is the
# same verb over storage the caller owns, and it is the CONTRAST that makes the
# rule legible — the polling tiers hold their core inline, so theirs really do
# release.
FFI_FN = re.compile(
    r'^pub unsafe extern "C" fn (nros_cpp_[a-z0-9_]*destroy[a-z0-9_]*)\s*\(', re.M
)
DROP_IN_PLACE = re.compile(r"drop_in_place\(\s*storage as \*mut ([A-Za-z0-9_:]+)")
# `<fn> drops <type> => NO_OP;` in the macro invocation.
ROW = re.compile(
    r"^\s*(nros_cpp_[a-z0-9_]+)\s+drops\s+([A-Za-z0-9_:<>, ]+?)\s*=>\s*([A-Z_]+)\s*;\s*$",
    re.M,
)

# What a `NO_OP` row's doc comment has to carry. The issue id so the next reader
# can find the measurement, and a phrase that states the consequence — one of
# these, because there is no single right wording and demanding a fixed sentence
# is how a gate turns documentation into a password.
NO_OP_PHRASES = (
    "runs no destructor",
    "releases nothing",
    "release nothing",
    "no-op",
)
ISSUE = "1496"


def doc_comment_above(text: str, index: int) -> str:
    """The `///` block immediately above the item starting at `index`.

    Attributes (`#[unsafe(no_mangle)]`) sit between the doc and the `fn`, so they
    are skipped rather than terminating the block.
    """
    lines = text[:index].split("\n")
    out: list[str] = []
    for line in reversed(lines):
        s = line.strip()
        if not s:
            if out:
                break
            continue
        if s.startswith("#["):
            continue
        if s.startswith("///") or s.startswith("//"):
            out.append(s)
            continue
        break
    return "\n".join(reversed(out))


def subjects(sources: dict[str, str]) -> dict[str, dict[str, str]]:
    """Every destroy FFI -> {file, doc, dropped type (last path segment)}."""
    found: dict[str, dict[str, str]] = {}
    for name, text in sorted(sources.items()):
        for m in FFI_FN.finditer(text):
            fn = m.group(1)
            body = text[m.end() : m.end() + 2000]
            drop = DROP_IN_PLACE.search(body)
            found[fn] = {
                "file": name,
                "doc": doc_comment_above(text, m.start()),
                "type": drop.group(1).split("::")[-1] if drop else "",
            }
    return found


def rows(table: str) -> dict[str, tuple[str, str]]:
    """Every table row -> (dropped type's last segment, shape)."""
    return {
        m.group(1): (m.group(2).split("::")[-1].split("<")[0].strip(), m.group(3))
        for m in ROW.finditer(table)
    }


def problems(sources: dict[str, str], table: str) -> list[str]:
    found = subjects(sources)
    declared = rows(table)
    out: list[str] = []

    for fn in sorted(set(found) - set(declared)):
        out.append(
            f"  {found[fn]['file']}: `{fn}` has no row in destroy_shape.rs.\n"
            f"      Add `{fn} drops <Type> => NO_OP|RELEASES;` there. The\n"
            f"      `needs_drop` assert will tell you which; if it is NO_OP, say\n"
            f"      so in the function's doc comment and name issue 1496."
        )
    for fn in sorted(set(declared) - set(found)):
        out.append(
            f"  destroy_shape.rs: row `{fn}` names no FFI function in {SRC.name}/.\n"
            f"      Delete the row with the function, or fix the spelling."
        )

    for fn in sorted(set(found) & set(declared)):
        want_ty, shape = declared[fn]
        got_ty = found[fn]["type"]
        if not got_ty:
            out.append(
                f"  {found[fn]['file']}: `{fn}` has a row but no\n"
                f"      `drop_in_place(storage as *mut …)` this gate can read.\n"
                f"      Keep the one-drop shape, or retire the row."
            )
        elif got_ty != want_ty:
            out.append(
                f"  {found[fn]['file']}: `{fn}` drops `{got_ty}`, and its row in\n"
                f"      destroy_shape.rs says `{want_ty}`. The row's `needs_drop`\n"
                f"      assert is then about a type this function never touches."
            )
        if shape == "NO_OP":
            doc = found[fn]["doc"].lower()
            if ISSUE not in doc or not any(p in doc for p in NO_OP_PHRASES):
                out.append(
                    f"  {found[fn]['file']}: `{fn}` is classified NO_OP and its doc\n"
                    f"      comment does not say so. A caller reads `destroy` as a\n"
                    f"      release; this one is not. Say what is NOT released and\n"
                    f"      name issue {ISSUE} (one of: "
                    + ", ".join(f"`{p}`" for p in NO_OP_PHRASES)
                    + ")."
                )
    return out


GOOD_SRC = '''
/// ABANDON the storage — issue 1496.
///
/// `drop_in_place` runs no destructor here; the state is in the arena.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_thing_destroy(storage: *mut c_void) -> nros_cpp_ret_t {
    unsafe { core::ptr::drop_in_place(storage as *mut CppThing); }
    NROS_CPP_RET_OK
}

/// Destroy a widget.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_widget_destroy(storage: *mut c_void) -> nros_cpp_ret_t {
    unsafe { core::ptr::drop_in_place(storage as *mut crate::w::CppWidget); }
    NROS_CPP_RET_OK
}
'''

GOOD_TABLE = """
destroy_shapes! {
    nros_cpp_thing_destroy drops CppThing => NO_OP;
    nros_cpp_widget_destroy drops crate::w::CppWidget => RELEASES;
}
"""


def self_test() -> None:
    """Negative controls, on the normal path.

    A gate nobody has seen fail is a comment. These four cases are the three
    failures this gate exists to produce, plus the passing shape — run on every
    invocation, because a selftest behind a flag is run once by its author.
    """
    ok = problems({"good.rs": GOOD_SRC}, GOOD_TABLE)
    assert not ok, f"{GATE} selftest: the compliant shape must pass, got {ok}"

    # 1. A destroy with no row at all — the coverage hole a Rust table cannot see.
    missing = problems(
        {"good.rs": GOOD_SRC},
        GOOD_TABLE.replace("    nros_cpp_thing_destroy drops CppThing => NO_OP;\n", ""),
    )
    assert any("has no row" in p for p in missing), (
        f"{GATE} selftest: an unlisted destroy must fail, got {missing}"
    )

    # 2. A NO_OP whose doc does not say so — issue 1496's own starting state.
    silent = problems(
        {"good.rs": GOOD_SRC},
        GOOD_TABLE.replace(
            "nros_cpp_widget_destroy drops crate::w::CppWidget => RELEASES;",
            "nros_cpp_widget_destroy drops crate::w::CppWidget => NO_OP;",
        ),
    )
    assert any("does not say so" in p for p in silent), (
        f"{GATE} selftest: an undocumented NO_OP must fail, got {silent}"
    )

    # 3. A row about a type the function does not drop — the assert would then
    #    measure something unrelated and pass.
    wrong = problems(
        {"good.rs": GOOD_SRC},
        GOOD_TABLE.replace("drops CppThing =>", "drops CppOther =>"),
    )
    assert any("never touches" in p for p in wrong), (
        f"{GATE} selftest: a mismatched row type must fail, got {wrong}"
    )

    # 4. A row naming nothing — the issue-0743 shape, a stale entry going inert.
    stale = problems(
        {"good.rs": GOOD_SRC},
        GOOD_TABLE + "\nnros_cpp_gone_destroy drops CppGone => NO_OP;\n",
    )
    assert any("names no FFI function" in p for p in stale), (
        f"{GATE} selftest: a row for a deleted function must fail, got {stale}"
    )


def tracked_rust_sources() -> list[Path]:
    """Every tracked `.rs` file under `SRC`, by index lookup.

    `git ls-files`, not `rglob` — issue 0844's rule, which
    `check-no-tracked-file-find` enforces: an index lookup instead of a walk,
    measured at 7m36s -> 0.8s for the same 232 paths, and pruning does not help
    because a walk still stats every directory it considers pruning. It also
    keeps build output and untracked scratch files out of the subject, which for
    this gate matters: an untracked copy of a destroy FFI would be classified
    against a table that has no reason to name it.
    """
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

    # phase-472 W4 — `SRC` is tracked; its absence was reported as NOT CHECKED
    # with exit 0. It is the population moving, and the subject count below
    # must be non-zero instead.
    if not TABLE.is_file():
        print(
            f"{GATE}: FAIL — {TABLE.relative_to(REPO)} is missing.\n"
            "  It is the classification every destroy FFI is held to (issue 1496)."
        )
        return 1

    sources = {
        p.name: p.read_text(encoding="utf-8")
        for p in tracked_rust_sources()
        if p.name != TABLE.name
    }
    if not require_population(len(subjects(sources)), "destroy FFI(s)", gate=GATE):
        return 1
    found = problems(sources, TABLE.read_text(encoding="utf-8"))
    if found:
        print(f"{GATE}: FAIL")
        for p in found:
            print(p)
        return 1

    n = len(subjects(sources))
    shapes = rows(TABLE.read_text(encoding="utf-8"))
    noop = sum(1 for _, s in shapes.values() if s == "NO_OP")
    print(f"{GATE}: OK — {n} destroy FFI(s) classified, {noop} documented as no-ops.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
