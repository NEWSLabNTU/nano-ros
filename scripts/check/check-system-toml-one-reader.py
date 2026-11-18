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
                    # Stop at the enclosing `fn`: a name bound in another
                    # function is another variable.
                    start = max(0, i - WINDOW)
                    for j in range(i - 1, start - 1, -1):
                        if re.search(r"\bfn\s+\w+", lines[j]):
                            start = j
                            break
                    for j in range(start, i):
                        if bind.search(lines[j]) and NAMES_FILE.search(
                            " ".join(lines[j : j + 3])
                        ):
                            hit = True
            if hit:
                out.append(f"{path}:{i + 1}: {l.strip()[:120]}")
    return out


PY_HOME = "scripts/lib/system_toml.py"
PY_READ = re.compile(r"\.read_text\(|(?<![\w.])open\(|tomllib\.loads?\(|tomli\.loads?\(")
PY_WRITE = re.compile(r"""open\(.*,\s*["'][wa]""")
# Case-sensitive, unlike NAMES_FILE: `system_toml` is the reader's module name.
PY_NAMES = re.compile(r"system\.toml")
OWN_TEXT = "# system.toml own text:"


def py_offenders(path: str, text: str) -> list[str]:
    """A Python read of a system.toml around `scripts/lib/system_toml.py`.

    A read is `.read_text(`, `open(` (not for writing) or `tomllib.load(s)`.
    It counts when its line names the file, or names a variable bound to it
    within the 25 lines above. A deliberate read of a file's own text carries
    `# system.toml own text: <reason>` on its line or the line above."""
    lines = text.split("\n")
    out = []
    for i, l in enumerate(lines):
        if not PY_READ.search(l) or PY_WRITE.search(l) or l.lstrip().startswith("#"):
            continue
        if OWN_TEXT in l or (i and OWN_TEXT in lines[i - 1]):
            continue
        hit = bool(PY_NAMES.search(l.split("#", 1)[0]))
        if not hit:
            # The window stops at the enclosing `def`: a name bound in another
            # function is another variable.
            start = max(0, i - WINDOW)
            for j in range(i - 1, start - 1, -1):
                if re.match(r"\s*def ", lines[j]):
                    start = j
                    break
            # Names only, not attributes (`os.path` is not a variable `path`).
            for var in set(re.findall(r"(?<![\w.])([a-z_][a-z0-9_]*)\b", l.split("#", 1)[0])):
                bind = re.compile(rf"^\s*{var}\s*=[^=]")
                if any(bind.search(lines[j]) and PY_NAMES.search(lines[j].split("#", 1)[0])
                       for j in range(start, i)):
                    hit = True
                    break
        if hit:
            out.append(f"{path}:{i + 1}: {l.strip()[:120]}")
    return out


def selftest() -> list[str]:
    bad = []
    direct = 'fn f(d: &Path) { let t = std::fs::read_to_string(d.join("system.toml")); }'
    via = 'fn f(d: &Path) {\n    let p = d.join(SYSTEM_TOML);\n    let t = fs::read_to_string(&p);\n}'
    other = 'fn f(d: &Path) { let t = std::fs::read_to_string(d.join("package.xml")); }'
    other_fn = ('fn a(d: &Path) {\n    let p = d.join("system.toml");\n}\n'
                'fn b(p: &Path) {\n    let t = std::fs::read_to_string(p);\n}')
    test_mod = '#[cfg(test)]\nmod tests {\n    fn f() { std::fs::read_to_string("system.toml"); }\n}'
    ours = 'fn f(d: &Path) { nros_system_include::read_to_string(d.join("system.toml")); }'
    for name, src, want in [
        ("python: a direct read", 'doc = tomllib.loads((d / "system.toml").read_text())', 1),
        ("python: through a bound variable", 'st = d / "system.toml"\ntext = st.read_text()', 1),
        ("python: marked own text", 'st = d / "system.toml"\n'
         '# system.toml own text: it detects an include\ntext = st.read_text()', 0),
        ("python: a write", 'with open(os.path.join(d, "system.toml"), "w") as fh:', 0),
        ("python: another file", 'text = (d / "package.xml").read_text()', 0),
    ]:
        got = len(py_offenders("x.py", src))
        if got != want:
            bad.append(f"selftest: {name}: {got} offender(s), want {want}")
    for name, src, want in [
        ("direct", direct, 1),
        ("through a bound variable", via, 1),
        ("another file", other, 0),
        ("a binding in another fn", other_fn, 0),
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
    sys.path.insert(0, str(ROOT / "scripts" / "lib"))
    import system_toml  # noqa: E402

    problems += [f"python reader: {p}" for p in system_toml.selftest()]
    for f in tracked("packages/**/*.rs"):
        if f.startswith(HOME) or "/tests/" in f or f in ALLOWED:
            continue
        problems += offenders(f, (ROOT / f).read_text(encoding="utf8", errors="replace"))
    for f in tracked("*.py"):
        if f in (PY_HOME, "scripts/check/check-system-toml-one-reader.py") or "third-party/" in f:
            continue
        problems += py_offenders(f, (ROOT / f).read_text(encoding="utf8", errors="replace"))
    if problems:
        print("check-system-toml-one-reader: a system.toml is read around its one reader:")
        for p in problems:
            print(f"  {p}")
        print(
            "  Read what the system means through the one reader, which expands\n"
            "  `include = [..]`: in Rust nros_system_include::read_to_string (or read),\n"
            "  in Python scripts/lib/system_toml.py's load (or text). A tool that edits\n"
            "  the file reads its own text (nros_system_include::read_own_text); a Python\n"
            "  read of its own text says why: `# system.toml own text: <reason>`."
        )
    if problems:
        return 1
    print(
        "check-system-toml-one-reader: OK -- every non-test Rust read of a system.toml goes "
        "through nros_system_include and every Python read through scripts/lib/system_toml.py; "
        "selftest: a direct read and one through a bound variable are refused in both, and "
        "the Python reader agrees with every shared case"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
