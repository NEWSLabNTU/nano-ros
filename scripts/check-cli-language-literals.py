#!/usr/bin/env python3
"""No CLI code DECIDES on a language by comparing a string — phase-469.

A language is a TYPE inside the CLI (`nros_lang::Language`). A string compared
against a language spelling is a decision the compiler cannot see: a fourth
language falls off it silently and lands in generated code, which is the shape
of every defect this campaign removed — `is_cpp = lang != "c"` (#1363),
`lang.as_deref() == Some("c")` (phase-469 W2), `match args.language.as_str() {
"c" => …, "cpp" => … }` and `.unwrap_or("rust")` (phase-469 W3), and the two
extension tables that disagreed with cmake's (`ends_with(".cpp") || …`).

WHY THIS IS A RULE AND NOT AN ALLOWLIST

Phase-469 W2 declined a gate here, correctly for the predicate it had: "a
language literal near the word `lang`" matched 53 lines, and reading them showed
the grep could not tell a decision from a toolchain name, a template body or a
test fixture. A gate over that predicate is an exemption list.

The predicate below is narrower and positional: a spelling is flagged only where
it DECIDES something —

  * a match / `matches!` / `if let` PATTERN   (`"c" =>`, `"c" | "cpp"`);
  * an operand of `==` / `!=`, bare or in `Some(…)`;
  * the argument of a comparing method       (`unwrap_or`, `eq_ignore_ascii_case`,
                                              `ends_with`, `starts_with`,
                                              `strip_suffix`, `strip_prefix`).

A spelling on the RIGHT of an arm (`Language::C => "c"`, a producer), inside a
JSON/TOML fixture, in a path (`join("rust")`) or in prose is not in any of those
positions, so it is not flagged — and none needs an entry here. Measured on the
tree this landed with: ZERO hits after the four conversions it was written
beside, with no exemption list at all.

Two things are deliberately scoped, and they are properties of the predicate,
not named sites:

  * DOTTED extensions are checked for the C-FAMILY spellings only. The CLI
    contains no C/C++ source of its own, so a `".cpp"` comparison in it can only
    be deciding a USER source's language — the question
    `Language::of_source` answers. A `".rs"` comparison is, in this tree, the
    CLI asking which of its OWN files cargo compiles (`source_stamp.rs`, which
    runs inside a build script and cannot reach `nros-lang`): a file type, not a
    choice among languages. A bare `"rs"` IS checked.
  * `nros-lang` itself is out of scope — it is where the spellings BELONG.

The spellings are HARVESTED from `nros-lang/src/lib.rs` (every literal on a
non-test line that names `Language::`), never authored here, so a spelling added
to the enumeration's tables is covered without editing this file.

The compiler is still the primary enforcement: a typed `match` makes a fourth
variant a compile error. This gate covers the one thing the compiler cannot —
a decision that never became typed in the first place.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import comments  # noqa: E402
import per_item  # noqa: E402

NROS_LANG = "packages/cli/nros-lang/src/lib.rs"
# Files the scan MUST reach — each held a decision site this gate's rule
# covers. A glob change that drops one fails here instead of going quiet.
MUST_REACH = (
    "packages/cli/nros-cli-core/src/orchestration/planner.rs",
    "packages/cli/nros-cli-core/src/orchestration/workspace.rs",
    "packages/cli/nros-cli-core/src/cmd/codegen.rs",
    "packages/cli/nros-cli-core/src/cmd/generate_px4.rs",
    "packages/cli/nros-cli-core/src/cmd/build.rs",
    "packages/cli/cargo-nano-ros/src/lib.rs",
)


def harvest(text: str) -> tuple[set[str], set[str]]:
    """(every spelling, the C-family subset) from nros-lang's non-test code."""
    code = per_item.rust_cfg_test_blank(comments.strip_comments(text, "rust"))
    spellings: set[str] = set()
    c_family: set[str] = set()
    for line in code.splitlines():
        if "Language::" not in line:
            continue
        lits = re.findall(r'"([^"\\]*)"', line)
        spellings.update(lits)
        if re.search(r"Language::(C|Cpp)\b", line):
            c_family.update(lits)
    # `.C` is matched before case-folding, on a line of its own.
    if re.search(r'ext\s*==\s*"C"', code):
        c_family.add("C")
    return spellings, c_family


def patterns(spellings: set[str], c_family: set[str]) -> list[tuple[str, re.Pattern]]:
    bare = "|".join(re.escape(f'"{s}"') for s in sorted(spellings, key=len, reverse=True))
    dotted = "|".join(re.escape(f'".{s}"') for s in sorted(c_family, key=len, reverse=True))
    lit = f"(?:{bare}|{dotted})"
    return [
        # `"c" =>`, `"c" |`, `Some("c")) =>`, `"c" if …` — a pattern position.
        ("match pattern", re.compile(rf"{lit}\s*\)*\s*(?:=>|\|(?!\|)|\bif\b)")),
        ("match pattern", re.compile(rf"(?<!\|)\|\s*(?:Some\(\s*)?{lit}")),
        ("matches! pattern", re.compile(rf"matches!\([^;]*?,\s*(?:Some\(\s*)?{lit}")),
        ("equality", re.compile(rf"(?:==|!=)\s*&?\s*(?:Some\(\s*)?{lit}")),
        ("equality", re.compile(rf"{lit}\s*\)?\s*(?:==|!=)")),
        (
            "comparing call",
            re.compile(
                rf"\.(?:unwrap_or|eq_ignore_ascii_case|ends_with|starts_with|"
                rf"strip_suffix|strip_prefix)\(\s*{lit}\s*\)"
            ),
        ),
    ]


def scan_text(text: str, pats) -> list[tuple[int, str, str]]:
    code = comments.strip_comments(text, "rust")
    hits = []
    for kind, pat in pats:
        for m in pat.finditer(code):
            line = per_item.line_of(code, m.start())
            hits.append((line, kind, text.splitlines()[line - 1].strip()))
    return sorted(set(hits))


def scope() -> list[str]:
    out = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "--", "packages/cli/*/src/*.rs"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.split()
    return [p for p in out if not p.startswith("packages/cli/nros-lang/")]


def self_test(pats) -> list[str]:
    """Negative control on every run: each decision shape is caught, each
    non-decision shape is not, and a real file mutated back to its pre-phase-469
    shape is caught."""
    errs = []
    must_flag = {
        "match arm": 'match s.as_str() { "c" => a(), _ => b() }',
        "or-pattern": 'match s { "rust" | "rs" => a(), _ => b() }',
        "matches!": 'if matches!(s.as_str(), "cpp" | "cxx") {}',
        "eq": 'let is_cpp = lang != "c";',
        "eq Some": 'if lang.as_deref() == Some("c") {}',
        "eq lhs": 'if "rust" == lang {}',
        "default": 'let l = field.unwrap_or("rust");',
        "case-insensitive": 'if l.eq_ignore_ascii_case("cpp") {}',
        "extension table": 'if s.ends_with(".cpp") || s.ends_with(".cc") {}',
        "extension eq": 'if path.extension().and_then(|e| e.to_str()) == Some("rs") {}',
    }
    must_pass = {
        "producer arm": 'match l { Language::C => "c", Language::Cpp => "cpp" }',
        "json fixture": 'json!({"language": "rust", "lang": "cpp"})',
        "toml fixture": 'r#"language = "rust""#',
        "path": 'let m = ws.join("modules").join("lang").join("rust");',
        "comment": '// it read `lang != "c"` before',
        "own sources": 'rel.ends_with(".rs") || rel.ends_with(".jinja")',
        "logical or": 'let x = a || "c".is_empty();',
    }
    for name, src in must_flag.items():
        if not scan_text(src, pats):
            errs.append(f"selftest: `{name}` was NOT flagged: {src}")
    for name, src in must_pass.items():
        hits = scan_text(src, pats)
        if hits:
            errs.append(f"selftest: `{name}` was flagged ({hits[0][1]}): {src}")
    # Mutation on REAL code: undo the typed dispatch in `cmd/codegen.rs`.
    real = ROOT / "packages/cli/nros-cli-core/src/cmd/codegen.rs"
    text = real.read_text()
    typed = "nros_lang::Language::C => {"
    if typed not in text:
        errs.append(f"selftest: mutation anchor `{typed}` is gone from {real.name}")
    else:
        if scan_text(text, pats):
            errs.append(f"selftest: {real.name} is not clean before mutation")
        if not scan_text(text.replace(typed, '"c" => {', 1), pats):
            errs.append(f"selftest: mutating {real.name} back to `\"c\" =>` was NOT flagged")
    return errs


def main() -> int:
    spellings, c_family = harvest((ROOT / NROS_LANG).read_text())
    if not {"c", "cpp", "rust"} <= spellings or "cpp" not in c_family:
        print(f"check-cli-language-literals: harvest from {NROS_LANG} looks wrong: "
              f"{sorted(spellings)} / C-family {sorted(c_family)}")
        return 1
    pats = patterns(spellings, c_family)

    errs = self_test(pats)
    files = scope()
    missing = [f for f in MUST_REACH if f not in files]
    if missing:
        errs.append(f"scope no longer reaches: {', '.join(missing)}")
    if errs:
        print("check-cli-language-literals: the gate itself is broken:")
        for e in errs:
            print(f"  {e}")
        return 1

    found = []
    for rel in files:
        for line, kind, src in scan_text((ROOT / rel).read_text(), pats):
            found.append(f"  {rel}:{line}: {kind}: {src}")
    if found:
        print(
            "check-cli-language-literals: a language decided by comparing a STRING "
            "(phase-469).\nParse once at the edge (`nros_lang::Language::parse`, "
            "`::of_source`, serde) and match the enum exhaustively — or, for a "
            "file's language, ask `Language::of_source`:"
        )
        print("\n".join(found))
        return 1
    print(
        f"check-cli-language-literals: OK — {len(files)} files, "
        f"{len(spellings)} spellings harvested, no string-typed language decision"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
