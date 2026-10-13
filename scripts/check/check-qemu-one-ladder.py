#!/usr/bin/env python3
"""check-qemu-one-ladder — issue 1851.

Which QEMU system binary runs is ONE ladder: `QEMU_SYSTEM_<ARCH>`, the
project-local `build/qemu/bin`, the `nros setup` store at the PINNED version,
then `$PATH`. The tests ask `nros_tests::qemu::qemu_system_path` (or
`qemu_system_cmd`); shell and `just` ask `nros_qemu_bin` /
`nros_qemu_report` (`scripts/lib/qemu-bin.sh`).

Before this, each consumer searched its own subset — `just freertos` and
`just nuttx` only `build/qemu`, the RISC-V and ESP32 recipes only `$PATH`, the
Rust RISC-V launchers only `$PATH` — so a host whose QEMU sat in the store
(what `nros setup` produces) passed the ARM tests while the recipes ran the
distro's 6.2 or nothing.

This refuses a QEMU launched or probed by bare NAME in code:

  * shell / `just`: a `qemu-system-<arch>` token outside a comment and outside
    a string literal (`command -v qemu-system-arm`, a bare launch line), unless
    it is the argument of `nros_qemu_bin` / `nros_qemu_report`;
  * Rust: `Command::new("qemu-system-…")`.

A string literal is prose to this gate (an `echo` naming the binary, a
`"$(nros_qemu_bin qemu-system-arm)"` substitution). Buildless, ~0.2 s;
self-tests its matcher on every run and reports stale allowlist entries.

Issue 1884 — the PROBE must ask what the BOOT needs, too. The ESP32 probe
(`esp32::is_qemu_riscv32_available` / `require_qemu_riscv32`) resolves the
Espressif fork's store row and demands its `esp32c3` machine. Two NuttX
rv-virt tests called it, so a stock `qemu-system-riscv32`, which boots
`-M virt` fine, read as missing on every host without the fork. So a Rust
file that calls the ESP32 probe must also boot an ESP32 image
(`start_esp32_qemu`); anything else asks `qemu::qemu_system_has_machine`.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import comments  # noqa: E402
import git_grep  # noqa: E402  issue 1816 — a failed search is not "no match"

SHELL_BARE = re.compile(r"(?<![-/\w.$])qemu-system-[a-z0-9_]+")
SHELL_OK = re.compile(r"\bnros_qemu_(?:bin|report)\s+qemu-system-")
RUST_BARE = re.compile(r'Command::new\(\s*"qemu-system-')
# A `just` expression whose fallback is the bare name — a quoted string to the
# shell-side stripper, so it needs its own shape (what `just freertos` had).
JUST_FALLBACK = re.compile(r'\{\s*"qemu-system-[a-z0-9_]+"\s*\}')

# path -> why it may name a QEMU binary as a bare token.
ALLOWED = {
    "scripts/check/check-qemu-one-ladder.py": "this gate's own patterns and self-tests",
    "scripts/ci/runner-container.sh": (
        "`qemu-system-misc` is an apt PACKAGE name in the image's install list, "
        "not a binary anything runs"
    ),
}

SELF_TESTS = [
    # (text, lang, should_match)
    ("    qemu-system-arm -cpu cortex-m3 -machine mps2-an385", "sh", True),
    ("if command -v qemu-system-riscv64 &>/dev/null; then", "sh", True),
    ('    "${NROS_DEADLINE[@]}" 20 qemu-system-riscv32 -M esp32c3', "just", True),
    ('let mut cmd = Command::new("qemu-system-riscv64");', "rust", True),
    ('    {{ if path_exists(QEMU_BIN) == "true" { QEMU_BIN } else { "qemu-system-arm" } }} -M virt', "just", True),
    ('    "$(nros_qemu_bin qemu-system-riscv64)" -M virt', "sh", False),
    ("    nros_qemu_report qemu-system-arm 7.2 || fail=1", "sh", False),
    ('    echo "  [MISSING] qemu-system-arm (run: nros setup --tool qemu)"', "sh", False),
    ("# qemu-system-arm on PATH is the last rung", "sh", False),
    ('let mut cmd = qemu_system_cmd("qemu-system-riscv64");', "rust", False),
    ('let mut cmd = Command::new("/usr/bin/qemu-system-arm");', "rust", False),
    ("    bin=/opt/qemu/bin/qemu-system-arm", "sh", False),
]


ESP32_PROBE = re.compile(r"\b(?:is_qemu_riscv32_available|require_qemu_riscv32)\s*\(")
ESP32_BOOT = re.compile(r"\bstart_esp32_qemu\s*\(")
ESP32_PROBE_HOME = "packages/testing/nros-tests/src/esp32.rs"

ESP32_SELF_TESTS = [
    # (text, should_be_refused)
    ("if !nros_tests::esp32::is_qemu_riscv32_available() { unmet!() }\n"
     "QemuProcess::start_nuttx_riscv(&t, true)", True),
    ("require_qemu_riscv32();\nlet q = start_esp32_qemu(flash, false);", False),
    ("// esp32::is_qemu_riscv32_available() is ESP32's\n"
     "if !nros_tests::qemu::is_qemu_riscv32_virt_available() {}", False),
]


def esp32_probe_misused(text: str) -> list[int]:
    """Lines calling the ESP32 probe in a file that boots no ESP32 image."""
    code = comments.strip_comments(text, "rust")
    if ESP32_BOOT.search(code):
        return []
    return [n for n, line in enumerate(code.splitlines(), 1) if ESP32_PROBE.search(line)]


def lang_of(path: str) -> str | None:
    if Path(path).name == "justfile":
        return "just"
    return comments.lang_for(path)


def hits(text: str, lang: str) -> list[int]:
    if lang == "rust":
        code = comments.strip_comments(text, lang)
        return [n for n, line in enumerate(code.splitlines(), 1) if RUST_BARE.search(line)]
    if lang not in ("sh", "just"):
        return []
    out = []
    if lang == "just":
        plain = comments.strip_comments(text, lang)
        out += [n for n, line in enumerate(plain.splitlines(), 1) if JUST_FALLBACK.search(line)]
    code = comments.strip_comments(text, lang, strings=True)
    for n, line in enumerate(code.splitlines(), 1):
        # The ladder's own argument is the one place the name may stand bare.
        line = SHELL_OK.sub("nros_qemu_ARG ", line)
        if SHELL_BARE.search(line):
            out.append(n)
    return sorted(set(out))


def self_test() -> int:
    bad = 0
    for text, lang, want in SELF_TESTS:
        got = bool(hits(text, lang))
        if got != want:
            bad += 1
            print(f"  self-test FAILED: {text!r} ({lang}) matched={got}, want {want}")
    for text, want in ESP32_SELF_TESTS:
        got = bool(esp32_probe_misused(text))
        if got != want:
            bad += 1
            print(f"  self-test FAILED (esp32 probe): {text!r} refused={got}, want {want}")
    return bad


def main() -> int:
    if self_test():
        print("check-qemu-one-ladder --self-test: FAILED")
        return 1
    files = git_grep.run(
        ["-lIE", r"qemu-system-[a-z0-9_]+", "--",
         "just", "justfile", "scripts", "packages", ":!*.md", ":!*.py"],
        ROOT,
    ).split()
    found = []
    seen_allowed = set()
    for f in files:
        lang = lang_of(f)
        if lang is None:
            continue
        try:
            text = (ROOT / f).read_text(errors="replace")
        except OSError:
            continue
        lines = hits(text, lang)
        if not lines:
            continue
        if f in ALLOWED:
            seen_allowed.add(f)
            continue
        found += [f"{f}:{n}" for n in lines]
    stale = sorted(set(ALLOWED) - seen_allowed - {"scripts/check/check-qemu-one-ladder.py"})
    misused = []
    for f in git_grep.run(
        ["-lE", r"is_qemu_riscv32_available|require_qemu_riscv32", "--", "packages/*.rs"],
        ROOT,
    ).split():
        if f == ESP32_PROBE_HOME:
            continue
        text = (ROOT / f).read_text(errors="replace")
        misused += [f"{f}:{n}" for n in esp32_probe_misused(text)]
    rc = 0
    if misused:
        rc = 1
        print("check-qemu-one-ladder: the ESP32 QEMU probe guards a boot that is not ESP32 "
              "(issue 1884):\n")
        for h in misused:
            print(f"  {h}")
        print(
            "\nIt demands the Espressif fork's `esp32c3` machine. Ask for the machine the"
            "\nboot passes to -M instead: nros_tests::qemu::qemu_system_has_machine"
            "(\"qemu-system-<arch>\", \"<machine>\")."
        )
    if found:
        rc = 1
        print("check-qemu-one-ladder: a QEMU binary is launched or probed by bare NAME:\n")
        for h in found:
            print(f"  {h}")
        print(
            "\nAsk the one ladder instead (issue 1851) —"
            "\nshell/just: \"$(nros_qemu_bin qemu-system-<arch>)\" or nros_qemu_report"
            " (scripts/lib/qemu-bin.sh);"
            "\nRust: nros_tests::qemu::qemu_system_cmd(\"qemu-system-<arch>\")."
        )
    if stale:
        rc = 1
        print("check-qemu-one-ladder: allowlist entries that no longer name one (delete them):")
        for f in stale:
            print(f"  {f}")
    if rc == 0:
        print(f"check-qemu-one-ladder: OK ({len(SELF_TESTS) + len(ESP32_SELF_TESTS)} self-test cases; "
              f"{len(ALLOWED)} allowlisted)")
    return rc


if __name__ == "__main__":
    sys.exit(main())
