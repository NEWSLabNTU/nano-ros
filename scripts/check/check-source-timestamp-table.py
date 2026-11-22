#!/usr/bin/env python3
"""The per-backend source-stamp table cites a measurement for every claim.

`docs/reference/source-timestamps.md` says, per (RMW backend, platform), where a
sample's source timestamp comes from and whether it follows the image's
selected wall clock (RFC-0141 D7). Read from code, such a table is a guess
that looks like a fact: the RFC said XRCE stamps "through the agent's time
sync", and a built image measured 0 on every sample. So each row must either
name the test that read a stamp back, or say `not measured` and claim nothing.

Rules, over the rows between the table's begin/end markers:

1. Each row has five cells: backend, platform, source, follows, evidence.
   `backend` is a registered RMW backend (`packages/rmw/<b>/*/nros-rmw.toml`).
   `follows` is one of `yes`, `no`, `no stamp`, `unknown`. No (backend,
   platform) appears twice.
2. Evidence is either `not measured: <reason>` (a non-empty reason), and then
   `follows` must be `unknown`, or `` `<binary>::<function>` ``, and then:
   - `packages/testing/nros-tests/tests/<binary>.rs` exists;
   - it defines `fn <function>` under a `#[test]` or `#[rstest]` attribute;
   - the function's text names the row's backend (so a zenoh cell cannot be
     cited for a Cyclone row);
   - `follows` is not `unknown`.
3. Every registered backend has a row, and at least one MEASURED row, unless
   it is in `NO_MEASURABLE_SURFACE` below, which states why no cell can read
   a stamp back.

Run: python3 scripts/check/check-source-timestamp-table.py [--self-test]
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
TABLE = "docs/reference/source-timestamps.md"
TESTS = "packages/testing/nros-tests/tests"
BEGIN = "<!-- source-timestamp-table:begin -->"
END = "<!-- source-timestamp-table:end -->"
FOLLOWS = {"yes", "no", "no stamp", "unknown"}

# A backend no cell can read a source stamp from, and why. Its rows must still
# exist and say `not measured`; rule 3 only waives the measured row.
NO_MEASURABLE_SURFACE = {
    "uorb": "it runs only inside PX4, and its vtable has no take-with-info entry, "
    "so no MessageInfo reaches a subscriber to read a stamp from",
}

CITE = re.compile(r"^`([A-Za-z0-9_]+)::([A-Za-z0-9_]+)`$")
NOT_MEASURED = re.compile(r"^not measured:\s*(\S.*)$")
FN = re.compile(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z0-9_]+)\s*\(", re.M)
TEST_ATTR = re.compile(r"#\[(?:test|rstest|tokio::test)\b")


def registered_backends(repo: Path) -> set[str]:
    # Each backend declares itself in one descriptor (RFC-0071 D1); its
    # directory under packages/rmw/ is its name.
    return {p.relative_to(repo).parts[2] for p in repo.glob("packages/rmw/*/*/nros-rmw.toml")}


def table_rows(text: str) -> tuple[list[list[str]], list[str]]:
    errors: list[str] = []
    if BEGIN not in text or END not in text:
        return [], [f"{TABLE}: no `{BEGIN}` ... `{END}` block"]
    body = text.split(BEGIN, 1)[1].split(END, 1)[0]
    rows = []
    for line in body.splitlines():
        line = line.strip()
        if not line.startswith("|"):
            continue
        cells = [c.strip() for c in line.strip("|").split("|")]
        if cells and cells[0] in ("backend", "---"):
            continue
        rows.append(cells)
    if not rows:
        errors.append(f"{TABLE}: the table has no rows")
    return rows, errors


def test_functions(path: Path) -> dict[str, str]:
    """`fn name` -> its text (attributes above it through the next fn), test fns only."""
    text = path.read_text()
    matches = list(FN.finditer(text))
    out = {}
    for i, m in enumerate(matches):
        start = matches[i - 1].end() if i else 0
        head = text[start : m.start()]
        # Only the attribute block directly above this fn: after the previous
        # item's closing brace.
        head = head.rsplit("}", 1)[-1]
        if not TEST_ATTR.search(head):
            continue
        end = matches[i + 1].start() if i + 1 < len(matches) else len(text)
        out[m.group(1)] = head + text[m.start() : end]
    return out


def check(text: str, backends: set[str], tests_dir: Path) -> list[str]:
    rows, errors = table_rows(text)
    seen: set[tuple[str, str]] = set()
    measured: set[str] = set()
    with_rows: set[str] = set()
    cache: dict[str, dict[str, str] | None] = {}
    for cells in rows:
        where = f"{TABLE}: row `| {' | '.join(cells)} |`"
        if len(cells) != 5:
            errors.append(f"{where}: has {len(cells)} cells, wants 5")
            continue
        backend, platform, _source, follows, evidence = cells
        if backend not in backends:
            errors.append(f"{where}: `{backend}` is no registered backend {sorted(backends)}")
            continue
        with_rows.add(backend)
        if (backend, platform) in seen:
            errors.append(f"{where}: ({backend}, {platform}) appears twice")
        seen.add((backend, platform))
        if follows not in FOLLOWS:
            errors.append(f"{where}: follows `{follows}` is not one of {sorted(FOLLOWS)}")
        if NOT_MEASURED.match(evidence):
            if follows != "unknown":
                errors.append(
                    f"{where}: not measured, yet it claims follows `{follows}`; "
                    "an unmeasured row says `unknown`"
                )
            continue
        cite = CITE.match(evidence)
        if not cite:
            errors.append(
                f"{where}: evidence must be `not measured: <reason>` or "
                "`<test binary>::<test function>` in backticks"
            )
            continue
        binary, func = cite.groups()
        if binary not in cache:
            path = tests_dir / f"{binary}.rs"
            cache[binary] = test_functions(path) if path.is_file() else None
        fns = cache[binary]
        if fns is None:
            errors.append(f"{where}: cites `{TESTS}/{binary}.rs`, which does not exist")
            continue
        if func not in fns:
            errors.append(f"{where}: `{TESTS}/{binary}.rs` has no test function `{func}`")
            continue
        if backend.lower() not in fns[func].lower():
            errors.append(
                f"{where}: `{binary}::{func}` never names `{backend}`, so it cannot be "
                "the measurement of this row"
            )
            continue
        if follows == "unknown":
            errors.append(f"{where}: cites a measurement, yet says follows `unknown`")
            continue
        measured.add(backend)
    for b in sorted(backends - with_rows):
        errors.append(f"{TABLE}: backend `{b}` has no row")
    for b in sorted((backends & with_rows) - measured):
        if b in NO_MEASURABLE_SURFACE:
            continue
        errors.append(
            f"{TABLE}: backend `{b}` has no measured row; measure one on a built image "
            "(a stamp read back) and cite its test"
        )
    return errors


def self_test() -> int:
    import tempfile

    tests_src = """
#[rstest]
#[case::cyclonedds(Rmw::Cyclonedds)]
fn measures_cyclonedds(#[case] rmw: Rmw) {}

#[test]
fn measures_zenoh() { let _ = "zenoh"; }

fn helper_cyclonedds() {}
"""

    def table(*rows: str) -> str:
        head = "| backend | platform | source stamp from | follows | evidence |\n| --- | --- | --- | --- | --- |\n"
        return f"{BEGIN}\n{head}" + "\n".join(rows) + f"\n{END}\n"

    good = [
        "| zenoh | linux | shim | yes | `t::measures_zenoh` |",
        "| cyclonedds | linux | ddsrt | no | `t::measures_cyclonedds` |",
        "| cyclonedds | zephyr | ddsrt | unknown | not measured: no cell |",
        "| uorb | px4 | none | unknown | not measured: PX4 only |",
    ]
    backends = {"zenoh", "cyclonedds", "uorb"}
    cases = [
        ("the good table", good, []),
        (
            "a cite to a missing test",
            [good[0].replace("measures_zenoh", "gone"), *good[1:]],
            ["has no test function `gone`"],
        ),
        (
            "a cite to a missing file",
            [good[0].replace("t::", "nofile::"), *good[1:]],
            ["does not exist"],
        ),
        (
            "a cite to a non-test helper",
            [good[0], good[1].replace("measures_cyclonedds", "helper_cyclonedds"), *good[2:]],
            ["has no test function `helper_cyclonedds`"],
        ),
        (
            "a backend with only `not measured`",
            [good[0], good[2], good[3]],
            ["`cyclonedds` has no measured row"],
        ),
        (
            "a not-measured row that claims a verdict",
            [*good[:2], good[2].replace("unknown", "yes"), good[3]],
            ["an unmeasured row says `unknown`"],
        ),
        (
            "a cite to a test of another backend",
            [good[0], good[1].replace("measures_cyclonedds", "measures_zenoh"), *good[2:]],
            ["never names `cyclonedds`"],
        ),
        ("a backend with no row", good[:3], ["`uorb` has no row"]),
        (
            "an unregistered backend",
            [*good, "| fastdds | linux | x | no | not measured: none |"],
            ["`fastdds` is no registered backend"],
        ),
        ("a duplicate row", [*good, good[2]], ["appears twice"]),
    ]
    failed = 0
    with tempfile.TemporaryDirectory() as d:
        tests_dir = Path(d)
        (tests_dir / "t.rs").write_text(tests_src)
        for name, rows, want in cases:
            errs = check(table(*rows), backends, tests_dir)
            if not want:
                ok = not errs
            else:
                ok = all(any(w in e for e in errs) for w in want)
            if not ok:
                failed += 1
                print(f"self-test FAIL: {name}: wanted {want or 'no errors'}, got {errs}")
    if failed:
        return 1
    print(f"source-timestamp-table: self-test OK ({len(cases)} cases)")
    return 0


def main(argv: list[str]) -> int:
    if "--self-test" in argv:
        return self_test()
    rc = self_test()
    if rc:
        return rc
    errors = check((REPO / TABLE).read_text(), registered_backends(REPO), REPO / TESTS)
    for e in errors:
        print(f"source-timestamp-table: {e}", file=sys.stderr)
    if errors:
        return 1
    print("source-timestamp-table: OK (every row cites a measurement or says `not measured`)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
