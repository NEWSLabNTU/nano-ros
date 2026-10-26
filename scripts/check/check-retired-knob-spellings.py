#!/usr/bin/env python3
"""check-retired-knob-spellings — no ENV reader or writer names a retired knob spelling.

phase-484 W8 (RFC-0103 D2b). A knob's env spelling is `NROS_<id>`. The 32
older names in `knobs.toml`'s `legacy` column (`ZPICO_MAX_QUERYABLES`,
`ZPICO_SMOLTCP_*`, `ZPICO_READ_TASK_PRIORITY`, ...) were read with a
deprecation for release 0.5.0-nros2 and are now RETIRED: every reader —
`nros_zephyr_build::knob`, `nros-platform-config`'s `knob_env`, cmake's
`_nros_knob_env`, the CLI's `[image.<id>] env` — refuses one that is set,
naming the canonical spelling. The column stays as that error table for
good: an unlisted old name would be ignored, not refused.

Why the rule is "no ENV reader or writer", not "no file names one": the same
`ZPICO_*` words are also the zenoh-pico shim's C MACROS (`-DZPICO_MAX_QUERYABLES=4`,
`queryable_entry_t queryables[ZPICO_MAX_QUERYABLES]`). RFC-0103 makes those
backend-internal names generated C macros, never user inputs, so they stay —
and a ban on the word would fail on 145 lines of `zpico.c`. What must not
survive is any place a retired name is used AS AN ENVIRONMENT NAME, because
each one either reads a knob the reader now refuses or writes one a build
script now refuses:

  Rust     a string literal that IS the name (`knob("L")`, `env::var("L")`,
           `option_env!("L")`, a `Command::env("L", ..)`, an env-map key), or
           starts `"L=` (a diagnostic telling a person to set it), and
           `rerun-if-env-changed=L` / `rustc-env=L`. Not `.define("L", ..)`,
           which emits the C macro, nor `kconfig_key_for("L")`.
  TOML     a key `L = ..` under a table whose last segment is `env` (`[env]`,
           `[image.<id>.env]`), or inside an inline `env = { .. }`.
  CMake    `ENV{L}`, `_nros_knob_env(L` / `_nros_resolve_knob(L` /
           `_nros_resolve_derivable_knob(L`, and an `L=` on a `-E env` line.
  Shell    `export L=`, fish `set -gx L`, and an `L=value` assignment prefix
           (`L=4 cargo build`). Not `-DL=..`. Files: *.sh, *.bash, *.fish,
           justfiles, `.envrc`, `.env*`, `activate*`, Dockerfiles; also the
           `run:` text of workflows and fenced code blocks in Markdown — a
           documented command is an instruction (W8a's rule for open issues).
  YAML     a workflow `env:` row `L: ..`.
  Python   `os.environ[..]` / `.get(..)` / `getenv(..)` / `env["L"] = ..`.

Every form is also refused inside a Rust raw string or a Python string that
holds TOML, through the inline `env = { .. }` rule, which is file-kind blind.

  Messages (issue 2042) — a retired name inside a Rust string literal that is
           PROSE (holds whitespace): "Raise ZPICO_X to keep more", "ZPICO_X={}
           exhausted", a generated doc line "(set via ZPICO_X, default 8)".
           Following such advice turns a warning into a build refusal, and
           the text is what a person reads. Found by a small string lexer,
           because these messages run across `\`-continued lines. Not a
           literal that names the C macro as CODE (`const ZPICO_X`,
           `#define ZPICO_X`, `-DZPICO_X`, `ZPICO_X: usize`).

Exempt: the registry and its generated tables (the error table), its
generator, this gate, history (phase docs, archived issues, the changelog),
the book's retired-names page, vendored trees — any LINE that says the
spelling is "retired" (a refusal test, or prose naming the old name as old),
and the lines between a `nros-c-macro-names: begin` and `... end` comment: a
table of C macro NAMES a build script hands the C compiler
(`nros-zpico-build`'s `ZpicoConfig::defines`), which no line-local rule can
tell from an env read and which RFC-0103 keeps on purpose.

Buildless, tracked files only; self-tests every shape (both directions) on
each run. Sweep command:

    python3 scripts/check/check-retired-knob-spellings.py
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11 — the repo's interpreter is 3.10
    import tomli as tomllib

ROOT = Path(__file__).resolve().parents[2]
REGISTRY = ROOT / "packages/tooling/nros-zephyr-build/knobs.toml"

ALLOWED = {
    "packages/tooling/nros-zephyr-build/knobs.toml",
    "packages/tooling/nros-zephyr-build/src/knob_registry.rs",
    "cmake/NanoRosKnobRegistry.cmake",
    "scripts/gen-knob-registry.py",
    "scripts/check/check-retired-knob-spellings.py",
    "book/src/reference/retired-names.md",
    "book/src/reference/deprecated-names.md",
    "docs/issues/open.md",
}
HISTORY_PREFIXES = (
    "docs/roadmap/",
    "docs/issues/archived/",
    "changelog.d/",
    "third-party/",
)
HISTORY_FILES = {"CHANGELOG.md"}

MACRO_BEGIN = "nros-c-macro-names: begin"
MACRO_END = "nros-c-macro-names: end"

SHELL_SUFFIXES = (".sh", ".bash", ".fish", ".just")
SHELL_NAMES = {"justfile", ".envrc", ".env.example", "Dockerfile"}


def retired_names(registry_text: str | None = None) -> dict[str, str]:
    """retired spelling -> what to use instead, from the registry's error table.

    A renamed knob's `legacy` names map to its canonical spelling. A RETIRED
    knob (a row with `retired`, RFC-0110 D2) has no replacement spelling, so
    every one of its names -- the canonical one too -- maps to a pointer at the
    row's reason."""
    text = registry_text if registry_text is not None else REGISTRY.read_text()
    out = {}
    for row in tomllib.loads(text).get("knob", []):
        canonical = "NROS_" + row["id"]
        if row.get("retired"):
            for s in [canonical] + row.get("legacy", []):
                out[s] = f"nothing (the knob itself is retired; knobs.toml {row['id']} says why)"
            continue
        for old in row.get("legacy", []):
            out[old] = canonical
    return out


def compile_rules(names):
    alt = "|".join(sorted(map(re.escape, names), key=len, reverse=True))
    n = rf"(?<![A-Za-z0-9_])(?:{alt})(?![A-Za-z0-9_])"
    nb = rf"(?:{alt})"  # bounded by the surrounding pattern
    inline_env = re.compile(rf"(?<![A-Za-z0-9_])env\s*=\s*\{{[^}}]*?(?<![A-Za-z0-9_]){nb}\s*=")
    rust = [
        re.compile(rf'(?<!define\()(?<!kconfig_key_for\()"{nb}"'),
        re.compile(rf'"{nb}='),
        re.compile(rf"rerun-if-env-changed={n}"),
        re.compile(rf"rustc-env={n}"),
    ]
    cmake = [
        re.compile(rf"ENV\{{{nb}\}}"),
        re.compile(rf"_nros_(?:knob_env|resolve_knob|resolve_derivable_knob)\(\s*{n}"),
        re.compile(rf"-E\s+env\b.*(?<![A-Za-z0-9_-]){nb}="),
    ]
    shell = [
        re.compile(rf"\bexport\s+{nb}="),
        re.compile(rf"\bset\s+-[a-zA-Z]*x[a-zA-Z]*\s+{n}"),
        re.compile(rf"(?:^|[\s;&|(`'\"]){nb}=\S"),
    ]
    yaml = [re.compile(rf"^\s+{nb}:\s")]
    python = [
        re.compile(rf"environ(?:\.get|\.setdefault|\.pop)?\s*[\[(]\s*[\"']{nb}[\"']"),
        re.compile(rf"getenv\(\s*[\"']{nb}[\"']"),
        re.compile(rf"\benv\s*\[\s*[\"']{nb}[\"']\s*\]\s*="),
    ]
    toml_key = re.compile(rf"^\s*\"?{nb}\"?\s*=")
    return {
        "inline_env": inline_env,
        "rust": rust,
        "cmake": cmake,
        "shell": shell,
        "yaml": yaml,
        "python": python,
        "toml_key": toml_key,
    }


def kind_of(rel: str) -> str | None:
    base = os.path.basename(rel)
    if rel.endswith(".rs"):
        return "rust"
    if rel.endswith((".c", ".h", ".cc", ".cpp", ".hpp")):
        return "c"
    if rel.endswith(".toml"):
        return "toml"
    if rel.endswith(".cmake") or base == "CMakeLists.txt":
        return "cmake"
    if rel.endswith((".yml", ".yaml")):
        return "yaml"
    if rel.endswith(".py"):
        return "python"
    if rel.endswith(".md"):
        return "markdown"
    if (
        rel.endswith(SHELL_SUFFIXES)
        or base in SHELL_NAMES
        or base.startswith(("activate", "Dockerfile", ".env"))
    ):
        return "shell"
    return None


def _strip_line_comment(line: str, kind: str) -> str:
    s = line.lstrip()
    if kind == "rust" and s.startswith("//"):
        return ""
    if kind in ("cmake", "shell", "yaml", "python", "toml") and s.startswith("#"):
        return ""
    return line


def scan(text: str, kind: str, rules) -> list[tuple[int, str]]:
    """(line number, line) for every env use of a retired name in `text`."""
    hits = []
    table = ""
    fenced = False
    c_macros = False
    for i, raw in enumerate(text.splitlines(), 1):
        if MACRO_BEGIN in raw:
            c_macros = True
            continue
        if MACRO_END in raw:
            c_macros = False
            continue
        if c_macros:
            continue
        if kind == "markdown":
            if raw.lstrip().startswith("```"):
                fenced = not fenced
                continue
            if not fenced:
                continue
        if "retired" in raw.lower():
            continue
        line = _strip_line_comment(raw, kind)
        if not line.strip():
            continue
        found = bool(rules["inline_env"].search(line))
        if kind == "rust":
            found |= any(r.search(line) for r in rules["rust"])
        elif kind == "toml":
            m = re.match(r"^\s*\[\[?\s*([^\]]+?)\s*\]\]?\s*(?:#.*)?$", line)
            if m:
                table = m.group(1)
            elif table.split(".")[-1].strip('"') == "env":
                found |= bool(rules["toml_key"].search(line))
        elif kind == "cmake":
            found |= any(r.search(line) for r in rules["cmake"])
        elif kind == "shell":
            found |= any(r.search(line) for r in rules["shell"])
        elif kind == "yaml":
            found |= any(r.search(line) for r in rules["yaml"] + rules["shell"])
        elif kind == "python":
            found |= any(r.search(line) for r in rules["python"])
        elif kind == "markdown":
            found |= any(r.search(line) for r in rules["shell"])
        if found:
            hits.append((i, raw))
    return hits



def rust_string_literals(text: str):
    """(start offset, content) of every string literal in Rust source `text`.

    A small lexer, not a regex: a message is routinely split across lines with
    a trailing `\\`, and a line-local rule cannot tell its continuation lines
    from code. Comments are skipped; raw strings (`r"..."`, `r#"..."#`) and
    byte strings are literals too. A `'"'` char literal is not a string.
    """
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if text.startswith("//", i):
            j = text.find("\n", i)
            i = n if j < 0 else j
            continue
        if text.startswith("/*", i):
            j = text.find("*/", i + 2)
            i = n if j < 0 else j + 2
            continue
        if c == "'":
            # A char literal ('"', '\\'') or a lifetime ('a): skip both.
            m = re.match(r"'(?:\\.|[^\\'])'", text[i:i + 4])
            i += len(m.group(0)) if m else 1
            continue
        m = re.match(r'b?r(#*)"', text[i:i + 12]) if c in "br" else None
        if m and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            close = '"' + m.group(1)
            start = i + len(m.group(0))
            j = text.find(close, start)
            j = n if j < 0 else j
            yield start, text[start:j]
            i = j + len(close)
            continue
        if c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            yield i + 1, text[i + 1:j]
            i = j + 1
            continue
        i += 1


# A retired name inside a literal that is generated CODE, not a message to a
# person: a constant or macro definition, a `-D` flag, an index.
_CODE_BEFORE = re.compile(r"(?:\bconst\s+|#\s*(?:define|ifndef|ifdef|if)\s+|defined\(|-D|\[\s*)$")
# ...or as a typed item (`ZPICO_X: usize`), the shape a test asserts on.
_CODE_AFTER = re.compile(r"^\s*:\s*[a-z]")


def scan_rust_prose(text: str, names, lang: str = "rust") -> list[tuple[int, str]]:
    """issue 2042 -- a retired name in a MESSAGE: a Rust string literal that is
    prose (it holds whitespace) and names the spelling as something to set.
    Following such advice turns a runtime warning into a build refusal. A
    literal that IS the name (`.define("ZPICO_X", ..)`, a C macro table) or
    names it as generated code is not a message, and a line that says the
    spelling is retired is exempt as everywhere else.

    phase-488 -- C and C++ sources too (`lang="c"`): the zpico shim's
    `printk("... pool exhausted (ZPICO_MAX_LIVELINESS=%d) ...")` advised the
    retired spelling after 2042's Rust sweep, because the sweep read `.rs`
    only. The lexer is the same (C's literals and comments are Rust's without
    the raw and byte prefixes). A literal on a preprocessor line (`#error
    "ZPICO_X must be >= 1"`) names the C MACRO the compiler checked, which
    RFC-0103 keeps, so it is not a message to set an env name. A bare
    `getenv("ZPICO_X")` is the C env reader, and is a finding."""
    alt = "|".join(sorted(map(re.escape, names), key=len, reverse=True))
    word = re.compile(rf"(?<![A-Za-z0-9_{{]){{0}}(?:{alt})(?![A-Za-z0-9_}}])".replace("{0}", ""))
    hits = []
    lines = text.splitlines()
    for start, body in rust_string_literals(text):
        if lang == "c":
            line_no = text.count("\n", 0, start) + 1
            raw = lines[line_no - 1] if line_no - 1 < len(lines) else ""
            if raw.lstrip().startswith("#"):
                continue
            if body in names and re.search(r"\bgetenv\(\s*$", text[max(0, start - 40):start - 1]):
                hits.append((line_no, raw))
                continue
        if not re.search(r"\s", body):
            continue
        for m in word.finditer(body):
            if _CODE_BEFORE.search(body[: m.start()]) or _CODE_AFTER.match(body[m.end():]):
                continue
            line_no = text.count("\n", 0, start + m.start()) + 1
            raw = lines[line_no - 1] if line_no - 1 < len(lines) else ""
            if "retired" in raw.lower():
                continue
            hits.append((line_no, raw))
    return hits

# (kind, text, should it be a finding)
SELF_TESTS = [
    ("rust", 'let q = knob("ZPICO_X").resolve(8);', True),
    ("rust", 'env_usize("ZPICO_X", 4)', True),
    ("rust", 'std::env::var("ZPICO_X")', True),
    ("rust", 'println!("cargo:rerun-if-env-changed=ZPICO_X");', True),
    ("rust", 'panic!("ZPICO_X={v} is below the floor")', True),
    ("rust", 'cmd.env("ZPICO_X", "4");', True),
    ("rust", 'imply("zenoh.x", on, "ZPICO_X");', True),
    ("rust", 'r#"env = { ZPICO_X = "2" }"#', True),
    ("rust", 'build.define("ZPICO_X", Some("4"));', False),
    ("rust", 'kconfig_key_for("ZPICO_X")', False),
    ("rust", "/// `ZPICO_X` sizes the queryable table", False),
    ("rust", 'pub const ZPICO_X: usize = 4;', False),
    ("rust", 'knob("NROS_ZENOH_X")', False),
    ("rust", 'env::set_var("ZPICO_X", "1"); // retired spelling: refusal case', False),
    ("rust", '"-DZPICO_X=4"', False),
    ("rust", '// nros-c-macro-names: begin\n("ZPICO_X", n.to_string()),\n// nros-c-macro-names: end\n', False),
    ("rust", '// nros-c-macro-names: begin\n// nros-c-macro-names: end\nknob("ZPICO_X")\n', True),
    ("toml", "[env]\nZPICO_X = \"4\"\n", True),
    ("toml", "[image.linux.env]\nZPICO_X = \"4\"\n", True),
    ("toml", "[[fixture]]\nenv = { ZPICO_X = \"2\" }\n", True),
    ("toml", "defines_env = { ZPICO_X = { env = \"NROS_ZENOH_X\" } }\n", False),
    ("toml", "[defines]\nZPICO_X = \"4\"\n", False),
    ("toml", "[env]\nNROS_ZENOH_X = \"4\"\n", False),
    ("cmake", 'if(DEFINED ENV{ZPICO_X})', True),
    ("cmake", '_nros_resolve_knob(ZPICO_X "${CONFIG_NROS_X}")', True),
    ("cmake", 'COMMAND ${CMAKE_COMMAND} -E env ZPICO_X=4 cargo build', True),
    ("cmake", 'zephyr_compile_definitions(ZPICO_X=${V})', False),
    ("cmake", '_nros_c_array_pool_floor(_v "${N}" ZPICO_X)', False),
    ("cmake", '# ZPICO_X sizes a C array', False),
    ("shell", 'export ZPICO_X=4', True),
    ("shell", 'ZPICO_X=4 cargo build', True),
    ("shell", 'set -gx ZPICO_X 4', True),
    ("shell", 'cc -DZPICO_X=4 foo.c', False),
    ("shell", 'echo "ZPICO_X sizes it"', False),
    ("yaml", "    env:\n      ZPICO_X: 4\n", True),
    ("yaml", "      run: ZPICO_X=4 just linux", True),
    ("yaml", "      NROS_ZENOH_X: 4", False),
    ("python", 'os.environ["ZPICO_X"] = "4"', True),
    ("python", 'os.environ.get("ZPICO_X")', True),
    ("python", 'env["ZPICO_X"] = "4"', True),
    ("python", 'C_MACROS = {"ZPICO_X"}', False),
    ("markdown", "```bash\nZPICO_X=4 cargo build\n```\n", True),
    ("markdown", "```sh\nexport ZPICO_X=4\n```\n", True),
    ("markdown", "Set `ZPICO_X=4` in prose.\n", False),
    ("markdown", "```bash\n# ZPICO_X=4 is retired; use NROS_ZENOH_X\n```\n", False),
]


# issue 2042 -- (text, should it be a finding) for `scan_rust_prose`.
PROSE_TESTS = [
    ('log_warn!(l, "ring holds {}. Raise ZPICO_X to keep more.", n);', True),
    ('log_warn!(l, "qos: ring holds {}. Raise \\\n    ZPICO_X to keep more.", n);', True),
    ('panic!("rejected: ZPICO_X={} exhausted", v)', True),
    ('"/// Max publishers (set via ZPICO_X, default 8).\\n\\\n pub const ZPICO_X: usize = {};"', True),
    ('"pub const ZPICO_X: usize = {};\\n"', False),
    ('assert!(body.contains("ZPICO_X: usize = 1;"));', False),
    ('build.define("ZPICO_X", Some("4"));', False),
    ('let s = "-DZPICO_X=4 -DZPICO_SMOLTCP_Y=1";', False),
    ('// Raise ZPICO_X to keep more.', False),
    ('log_warn!(l, "Raise NROS_ZENOH_X to keep more.");', False),
    ('log_warn!(l, "ZPICO_X is retired; set NROS_ZENOH_X");', False),
    ('let c = \'"\'; log!("Raise ZPICO_X now");', True),
    ('r#"Raise ZPICO_X to keep more"#', True),
]


# phase-488 -- (text, should it be a finding) for `scan_rust_prose(lang="c")`.
C_PROSE_TESTS = [
    ('printk("pool exhausted (ZPICO_X=%d) for \'%s\'\\n", ZPICO_X, k);', True),
    ('printf("raise ZPICO_X"\n       " to keep more\\n");', True),
    ('const char *v = getenv("ZPICO_X");', True),
    ('#error "ZPICO_X must be >= 1: it sizes a C array"', False),
    ('#  error "ZPICO_X must be >= 1"', False),
    ('static int a[ZPICO_X];', False),
    ('/* raise ZPICO_X to keep more */', False),
    ('printk("pool exhausted (NROS_ZENOH_X=%d)\\n", ZPICO_X);', False),
    ('char c = \'"\'; printk("raise ZPICO_X now");', True),
]


def self_test() -> int:
    rules = compile_rules(["ZPICO_X", "ZPICO_SMOLTCP_Y"])
    bad = 0
    for kind, text, want in SELF_TESTS:
        got = bool(scan(text, kind, rules))
        if got != want:
            bad += 1
            print(f"  self-test: {'missed' if want else 'false hit on'} ({kind}): {text!r}")
    for text, want in PROSE_TESTS:
        got = bool(scan_rust_prose(text, ["ZPICO_X", "ZPICO_SMOLTCP_Y"]))
        if got != want:
            bad += 1
            print(f"  self-test: {'missed' if want else 'false hit on'} (rust prose): {text!r}")
    for text, want in C_PROSE_TESTS:
        got = bool(scan_rust_prose(text, ["ZPICO_X", "ZPICO_SMOLTCP_Y"], lang="c"))
        if got != want:
            bad += 1
            print(f"  self-test: {'missed' if want else 'false hit on'} (c prose): {text!r}")
    # The registry is the input, so its own parse is a case too.
    table = retired_names('[[knob]]\nid = "ZENOH_X"\nlegacy = ["ZPICO_X"]\n')
    if table != {"ZPICO_X": "NROS_ZENOH_X"}:
        bad += 1
        print(f"  self-test: registry parse gave {table!r}")
    table = retired_names('[[knob]]\nid = "ZENOH_G"\nlegacy = ["ZPICO_G"]\nretired = "x"\n')
    if set(table) != {"NROS_ZENOH_G", "ZPICO_G"} or "NROS_ZENOH_G" in table.values():
        bad += 1
        print(f"  self-test: a retired knob's spellings gave {table!r}")
    return bad


def tracked() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "-z"], cwd=ROOT, capture_output=True, check=True
    ).stdout.decode()
    return [p for p in out.split("\0") if p]


def main() -> int:
    if n := self_test():
        print(f"check-retired-knob-spellings: self-test FAILED ({n} case(s))")
        return 1
    names = retired_names()
    if not names:
        print("check-retired-knob-spellings: FAILED — knobs.toml lists no retired spelling;"
              " if the error table was removed, delete this gate with it")
        return 1
    rules = compile_rules(names)
    word = re.compile("|".join(map(re.escape, names)))
    findings = []
    scanned = 0
    for rel in tracked():
        if rel in ALLOWED or rel in HISTORY_FILES or rel.startswith(HISTORY_PREFIXES):
            continue
        kind = kind_of(rel)
        if kind is None:
            continue
        try:
            text = (ROOT / rel).read_text(encoding="utf-8")
        except (UnicodeDecodeError, IsADirectoryError, FileNotFoundError):
            continue
        scanned += 1
        if not word.search(text):
            continue
        if kind == "c":
            hits = scan_rust_prose(text, names, lang="c")
        else:
            hits = scan(text, kind, rules)
        if kind == "rust":
            seen = {i for i, _ in hits}
            hits += [h for h in scan_rust_prose(text, names) if h[0] not in seen]
        for i, line in sorted(set(hits)):
            old = word.search(line).group(0)
            findings.append(f"  {rel}:{i}: {line.strip()[:110]}\n      -> {old} is retired; use {names[old]}")
    if findings:
        print("check-retired-knob-spellings: FAILED — a retired knob spelling is used as an")
        print("environment name (phase-484 W8, RFC-0103 D2b):\n")
        print("\n".join(findings))
        print(
            "\nEvery reader now REFUSES a retired spelling that is set, so a writer that sets"
            "\none breaks the build and a reader that asks for one reads nothing. Use the"
            "\ncanonical NROS_* name. A C macro (`.define(..)`, `-D..=`, a `zephyr_compile_"
            "\ndefinitions`) keeps its ZPICO_* name; a line that says the spelling is retired"
            "\nis exempt."
        )
        return 1
    print(f"check-retired-knob-spellings: OK — {len(names)} retired spelling(s), no env reader,"
          f" writer or message advising one in {scanned} tracked file(s)"
          f" (self-test: {len(SELF_TESTS) + len(PROSE_TESTS) + len(C_PROSE_TESTS)} cases)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
