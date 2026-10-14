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
    """retired spelling -> canonical spelling, from the registry's error table."""
    text = registry_text if registry_text is not None else REGISTRY.read_text()
    out = {}
    for row in tomllib.loads(text).get("knob", []):
        for old in row.get("legacy", []):
            out[old] = "NROS_" + row["id"]
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
    ("toml", "[image.native.env]\nZPICO_X = \"4\"\n", True),
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


def self_test() -> int:
    rules = compile_rules(["ZPICO_X", "ZPICO_SMOLTCP_Y"])
    bad = 0
    for kind, text, want in SELF_TESTS:
        got = bool(scan(text, kind, rules))
        if got != want:
            bad += 1
            print(f"  self-test: {'missed' if want else 'false hit on'} ({kind}): {text!r}")
    # The registry is the input, so its own parse is a case too.
    table = retired_names('[[knob]]\nid = "ZENOH_X"\nlegacy = ["ZPICO_X"]\n')
    if table != {"ZPICO_X": "NROS_ZENOH_X"}:
        bad += 1
        print(f"  self-test: registry parse gave {table!r}")
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
        for i, line in scan(text, kind, rules):
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
    print(f"check-retired-knob-spellings: OK — {len(names)} retired spelling(s), no env reader"
          f" or writer in {scanned} tracked file(s) (self-test: {len(SELF_TESTS)} cases)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
