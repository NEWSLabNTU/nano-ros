#!/usr/bin/env python3
"""check-system-toml-one-reader: a `system.toml` is read through ONE reader
(issue 2587, RFC-0024 §5).

A `system.toml` may name shared fragments with `include = [..]`. Only
`nros_system_include` expands them, so a Rust read of the file that goes
around it sees only what the file itself states, and a value stated in a
fragment silently goes missing. This gate refuses two things:

1. A Rust `fs::read_to_string(...)` of a `system.toml` outside
   `packages/tooling/nros-system-include`, in non-test code. A read counts
   when its argument names the file (`"system.toml"`, `SYSTEM_TOML`, an
   path variable named for it: `system_toml`,
   `system_path`, ...), or names a variable bound to it within
   the 25 lines above. Read meaning with `nros_system_include::read_to_string`
   (or `read`); a tool that EDITS the file reads its own text with
   `nros_system_include::read_own_text`.
2. A tracked `system.toml` that uses `include`, while the Python check
   scripts still read the file raw (issue 2587 PR 2 migrates them, then this
   half goes).

Reach, stated: a read through a variable bound more than 25 lines above, or
through a helper that takes an unnamed `&Path`, is not seen. Test modules
(`#[cfg(test)]`, `tests/`) read files they wrote themselves and are exempt.

Self-test on every run: synthetic sources a direct read, a read through a
bound variable and a test-module read must be classified as expected.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HOME = "packages/tooling/nros-system-include/"
# Reads of a system.toml's OWN text that are right by construction, each with
# its reason. Kept short: a new entry needs one.
ALLOWED = {
    # `[sources]` / `[tools]` only, which a fragment may not set
    # (nros_system_include refuses it); this crate has no TOML parser.
    "packages/tooling/nros-build-paths/src/lib.rs",
}
READ = re.compile(r"(?<![\w:])(?:std::)?fs::read_to_string\(")
NAMES_FILE = re.compile(r'system\.toml|SYSTEM_TOML', re.I)
WINDOW = 25
# A variable whose NAME says it holds the file.
PATH_VAR = re.compile(r"\b(system|system_toml|sys_toml|system_path|system_toml_path|root_sys)\b")


def _arg(line: str, start: int) -> str:
    """The text of the call's first argument."""
    depth, i = 1, start
    while i < len(line) and depth:
        depth += {"(": 1, ")": -1}.get(line[i], 0)
        i += 1
    return line[start : i - 1]


def offenders(path: str, text: str) -> list[str]:
    lines = text.split("\n")
    in_test = [False] * len(lines)
    t = False
    for i, l in enumerate(lines):
        if l.strip() == "#[cfg(test)]" and not l.startswith(" "):
            t = True
        in_test[i] = t
        if t and l == "}":
            t = False
    out = []
    for i, l in enumerate(lines):
        if in_test[i]:
            continue
        for m in READ.finditer(l):
            arg = _arg(l, m.end()).strip()
            code = re.sub(r'"(?:[^"\\]|\\.)*"', '""', arg)
            hit = bool(NAMES_FILE.search(arg) or PATH_VAR.search(code))
            if not hit:
                var = re.sub(r"^[&*\s]+|\.(as_ref|as_path|clone)\(\)$", "", arg)
                if re.fullmatch(r"[a-z_][a-z0-9_]*", var):
                    bind = re.compile(rf"\blet\s+(mut\s+)?{var}\b[^=]*=")
                    for j in range(max(0, i - WINDOW), i):
                        if bind.search(lines[j]) and NAMES_FILE.search(
                            " ".join(lines[j : j + 3])
                        ):
                            hit = True
            if hit:
                out.append(f"{path}:{i + 1}: {l.strip()[:120]}")
    return out


def selftest() -> list[str]:
    bad = []
    direct = 'fn f(d: &Path) { let t = std::fs::read_to_string(d.join("system.toml")); }'
    via = 'fn f(d: &Path) {\n    let p = d.join(SYSTEM_TOML);\n    let t = fs::read_to_string(&p);\n}'
    other = 'fn f(d: &Path) { let t = std::fs::read_to_string(d.join("package.xml")); }'
    test_mod = '#[cfg(test)]\nmod tests {\n    fn f() { std::fs::read_to_string("system.toml"); }\n}'
    ours = 'fn f(d: &Path) { nros_system_include::read_to_string(d.join("system.toml")); }'
    for name, src, want in [
        ("direct", direct, 1),
        ("through a bound variable", via, 1),
        ("another file", other, 0),
        ("a test module", test_mod, 0),
        ("the one reader", ours, 0),
    ]:
        got = len(offenders("x.rs", src))
        if got != want:
            bad.append(f"selftest: {name}: {got} offender(s), want {want}")
    return bad


def tracked(pattern: str) -> list[str]:
    return subprocess.run(
        ["git", "ls-files", pattern], cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout.split()


def main() -> int:
    problems = selftest()
    for f in tracked("packages/**/*.rs"):
        if f.startswith(HOME) or "/tests/" in f or f in ALLOWED:
            continue
        problems += offenders(f, (ROOT / f).read_text(encoding="utf8", errors="replace"))
    includes = []
    for f in tracked("**/system.toml") + tracked("system.toml"):
        text = (ROOT / f).read_text(encoding="utf8", errors="replace")
        if any(l.split("=", 1)[0].strip() == "include" for l in text.splitlines() if "=" in l):
            includes.append(f)
    if problems:
        print("check-system-toml-one-reader: a system.toml is read around its one reader:")
        for p in problems:
            print(f"  {p}")
        print(
            "  Read what the system means with nros_system_include::read_to_string (or read),\n"
            "  which expands `include = [..]`; a tool that edits the file reads its own text\n"
            "  with nros_system_include::read_own_text."
        )
    if includes:
        print("check-system-toml-one-reader: a tracked system.toml uses `include`, which the")
        print("  Python check scripts cannot expand yet:")
        for f in includes:
            print(f"  {f}")
        print("  State the fragment's keys in the file until those scripts read it expanded.")
    if problems or includes:
        return 1
    print(
        "check-system-toml-one-reader: OK -- every non-test Rust read of a system.toml goes "
        "through nros_system_include, and no tracked system.toml uses `include`; selftest: "
        "a direct read and one through a bound variable are refused"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
