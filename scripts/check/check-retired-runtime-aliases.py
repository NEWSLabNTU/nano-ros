#!/usr/bin/env python3
"""check-retired-runtime-aliases — phase-484 W8c's retirements stay retired.

Two shapes were deprecated in release 0.5.0-nros2 and retired after it:

* the runtime env aliases `ZENOH_LOCATOR` / `ZENOH_MODE` (of `NROS_LOCATOR` /
  `NROS_SESSION_MODE`). Both hosted readers now REFUSE a set one, through ONE
  table (`packages/api/nros/src/retired_env.rs`). A new reader anywhere else —
  a launcher's `${ZENOH_LOCATOR:-…}`, a test's `.env("ZENOH_LOCATOR", …)`, a
  bin's `std::env::var("ZENOH_LOCATOR")` — would bring the name back as a
  second spelling of the locator, which is what the retirement removed.
* `nros::main!(board = X)` (Form 2). The macro refuses it with a
  `compile_error!`; this keeps a tracked Rust source from spelling it, outside
  the one verdict fixture whose job is to prove the refusal.

Keys on READ shapes, as `check-retired-store-vars` does, so prose saying the
name is retired is not a finding and `CONFIG_NROS_ZENOH_LOCATOR` (a different
name that contains the word) never matches. Buildless, tracked files only;
self-tests every shape on each run.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

NAMES = r"ZENOH_(?:LOCATOR|MODE)"
READ = re.compile(
    r"|".join(
        [
            rf"\$\{{?{NAMES}\b",  # shell / just / cmake $NAME, ${NAME…}
            rf"ENV\{{{NAMES}\}}",  # cmake $ENV{NAME}
            rf"env::(?:var(?:_os)?|set_var|remove_var)\(\s*\"{NAMES}\"",  # Rust std
            rf"\bvar\(\s*c\"{NAMES}\"",  # nros::host_env's getenv reader
            rf"environ(?:\.get)?\s*[\[(]\s*[\"']{NAMES}[\"']",  # Python
            rf"getenv\(\s*c?[\"']{NAMES}[\"']",  # C / Python / Rust CStr
            rf"set\s+-q\s+{NAMES}\b",  # fish
            rf"\.env\(\s*\"{NAMES}\"",  # Command::env in tests
            rf"EnvGuard::(?:set|unset)\(\s*\"{NAMES}\"",  # test env guards
            rf"(?:^|[\s;]){NAMES}=",  # an assignment that sets it
        ]
    )
)

# Form 2, possibly multi-line: `main!(` then the args up to `board =`, allowing
# one level of nested parens (`args = [("k", "v")]`).
BOARD_ARG = re.compile(r"\bmain!\s*\((?:[^()]|\([^()]*\))*?\bboard\s*=")

# The one refusal table, this gate, and the verdict fixture that proves the
# macro refuses Form 2.
ALLOWED = {
    "packages/api/nros/src/retired_env.rs",
    "scripts/check/check-retired-runtime-aliases.py",
}
BOARD_ARG_ALLOWED = {
    "packages/testing/nros-tests/fixtures/n9_workspace/cases/"
    "main_macro_misuse_board_arg/src/demo_entry/src/main.rs.case",
}

SELF_TESTS = [
    ('nros_router_exec "${ZENOH_LOCATOR:-tcp/127.0.0.1:7447}"', True),
    ('LOCATOR="$ZENOH_MODE"', True),
    ('std::env::var("ZENOH_LOCATOR").unwrap_or_default()', True),
    ('std::env::remove_var("ZENOH_LOCATOR");', True),
    ('let legacy = var(c"ZENOH_MODE");', True),
    ('os.environ.get("ZENOH_LOCATOR", "tcp/127.0.0.1:7447")', True),
    ('getenv("ZENOH_LOCATOR")', True),
    ('.env("ZENOH_LOCATOR", &zenoh_locator)', True),
    ('let _g = EnvGuard::unset("ZENOH_MODE");', True),
    ("ZENOH_LOCATOR=tcp/1.2.3.4:7447 ./talker", True),
    ("# ZENOH_LOCATOR is retired (phase-484 W8c)", False),
    ('CONFIG_NROS_ZENOH_LOCATOR="tcp/10.0.2.2:7456"', False),
    ('-DCONFIG_NROS_ZENOH_LOCATOR="$loc"', False),
    ("pub const DEFAULT_ZENOH_LOCATORS: [&str; 2]", False),
    ('std::env::var("NROS_LOCATOR")', False),
]
BOARD_SELF_TESTS = [
    ("nros::main!(board = ::nros_board_linux::LinuxBoard);", True),
    ("nros::main!(\n    board = X,\n    launch = \"b\",\n);", True),
    ('nros::main!(launch = "b", args = [("k", "v")], board = X);', True),
    ("nros::main!();", False),
    ('nros::main!(launch = "demo_bringup");', False),
    ('nros::main!(model = "b:config/system_model.yaml");', False),
]


def strip_rust_comments(text: str) -> str:
    """Drop `//` comment lines: the macro's own docs record the retired form."""
    return "\n".join(
        line for line in text.splitlines() if not line.lstrip().startswith("//")
    )


def self_test() -> int:
    bad = [(t, want) for t, want in SELF_TESTS if bool(READ.search(t)) != want]
    bad += [
        (t, want)
        for t, want in BOARD_SELF_TESTS
        if bool(BOARD_ARG.search(strip_rust_comments(t))) != want
    ]
    # A comment that records the retired form is prose, not a use.
    if BOARD_ARG.search(strip_rust_comments("//! nros::main!(board = X);")):
        bad.append(("//! nros::main!(board = X);", False))
    for t, want in bad:
        print(f"  self-test: {'missed' if want else 'false hit on'}: {t!r}")
    return len(bad)


def main() -> int:
    if n := self_test():
        print(f"check-retired-runtime-aliases --self-test: {n} case(s) FAILED")
        return 1
    files = (
        subprocess.run(["git", "ls-files", "-z"], cwd=ROOT, capture_output=True, check=True)
        .stdout.decode()
        .split("\0")
    )
    findings = []
    for rel in files:
        if not rel or rel.startswith("docs/") or rel.endswith(".md"):
            continue
        p = ROOT / rel
        try:
            text = p.read_text(encoding="utf-8")
        except (UnicodeDecodeError, IsADirectoryError, FileNotFoundError):
            continue
        if rel not in ALLOWED and "ZENOH_" in text:
            for i, line in enumerate(text.splitlines(), 1):
                if READ.search(line):
                    findings.append(f"  {rel}:{i}: {line.strip()[:120]}")
        if (
            rel not in BOARD_ARG_ALLOWED
            and (rel.endswith(".rs") or rel.endswith(".rs.case"))
            and "board" in text
        ):
            code = strip_rust_comments(text)
            if m := BOARD_ARG.search(code):
                findings.append(f"  {rel}: `{m.group(0)[:80]}` — retired Form 2")
    if findings:
        print("check-retired-runtime-aliases: a retired name is used again:\n")
        print("\n".join(findings))
        print(
            "\nphase-484 W8c retired ZENOH_LOCATOR / ZENOH_MODE (use NROS_LOCATOR /"
            "\nNROS_SESSION_MODE; the refusal table is packages/api/nros/src/retired_env.rs)"
            "\nand nros::main!(board = X) (write nros::main!() and state the board in"
            "\nsystem.toml, or [package.metadata.nros.board] type in an out-of-tree board)."
        )
        return 1
    print(
        f"check-retired-runtime-aliases: OK "
        f"({len(SELF_TESTS) + len(BOARD_SELF_TESTS) + 1} self-test cases; no reader of "
        f"ZENOH_LOCATOR / ZENOH_MODE outside the refusal table, no main!(board =))"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
