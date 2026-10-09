#!/usr/bin/env python3
"""One `nros_platform_*` provider per linked graph — issue 1779.

The platform ABI is a set of free C symbols bound at link time. Two kinds of
crate compile a port that defines them: `nros-platform-cffi` (its
`posix-c-port` / `c-stub-test` features), and every board build script that
compiles its RTOS port into an `nros_platform_<rtos>` archive. Cargo's feature
unification can put both in one binary — `cargo test --workspace` gave
`nros-board-threadx`'s host test binary the POSIX port AND the ThreadX port,
and lld reported every `nros_platform_*` symbol twice. It was visible only
where the ThreadX submodule was initialised, so the same command was green in a
CI worktree and red on a provisioned checkout.

The fix is a protocol: `nros-platform-cffi` STATES the provider it compiled
(`links = "nros_platform_cffi"` + `cargo:abi_provider=…`), and every other
compile of a port ASKS first through
`nros_board_common::platform_port::defer_to_graph_provider`. This gate keeps
the protocol complete. Every Rust file under `packages/` that compiles an
`nros_platform_*` archive (`.compile("nros_platform_…")`) must call that helper
BEFORE the compile. The only exception is the provider itself, which states
the answer.

One shape is exempt, and it must say so at the site with a reason
(`// platform-port-provider-exempt: <reason>`). A second compile of the SAME
POSIX sources into a demand-driven archive (`nros-rmw-xrce-cffi`) has exactly
the provider's symbol set, so the linker never pulls a second member. What
collides is a whole-archive port or a different one, which was ThreadX's
case. Measured: `cargo build --tests --workspace --no-default-features
--keep-going` with every port's sources present fails on
`nros-board-threadx` alone.

It also checks the provider's half. `nros-platform-cffi` must keep its `links`
key and its `cargo:abi_provider=` line, because without them the helper reads
nothing and every board compiles its port again.

Usage: check-platform-port-single-provider.py [--self-test]
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
import comments  # noqa: E402  the one comment stripper (phase-472 W3)

ROOT = Path(__file__).resolve().parent.parent

PROVIDER_BUILD = "packages/platform/nros-platform-cffi/build.rs"
PROVIDER_MANIFEST = "packages/platform/nros-platform-cffi/Cargo.toml"

COMPILE_RE = re.compile(r'\.compile\(\s*"nros_platform_[a-z0-9_]+"\s*\)')
ASK_RE = re.compile(r"\bplatform_port::defer_to_graph_provider\s*\(")
# A site that compiles the SAME sources the provider does, into a
# demand-driven (not `+whole-archive`) archive, cannot collide: its members
# define exactly the symbols the provider's do, so once one member is pulled
# the other's has nothing left to satisfy. Stated at the site, with a reason.
EXEMPT_RE = re.compile(r"//[ \t]*platform-port-provider-exempt:[ \t]*(\S[^\n]*)")


def tracked_rust() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "packages/**/*.rs"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return [p for p in out.splitlines() if p]


def violations(path: str, text: str) -> list[str]:
    """Findings for one file's text."""
    if path == PROVIDER_BUILD:
        return []
    if EXEMPT_RE.search(text):
        return []
    # Comments blanked (same length), so a port NAMED in a comment is not a
    # compile, and an ask in a comment is not an ask.
    code = comments.strip_comments(text, "rust")
    compiles = [m.start() for m in COMPILE_RE.finditer(code)]
    if not compiles:
        return []
    asks = [m.start() for m in ASK_RE.finditer(code)]
    if not asks:
        line = code.count("\n", 0, compiles[0]) + 1
        return [
            f"{path}:{line}: compiles an nros_platform_* port but never calls "
            "`platform_port::defer_to_graph_provider` — a host build that feature "
            "unification gave the POSIX port links two providers (issue 1779)"
        ]
    if min(asks) > min(compiles):
        line = code.count("\n", 0, compiles[0]) + 1
        return [
            f"{path}:{line}: compiles an nros_platform_* port BEFORE asking "
            "`platform_port::defer_to_graph_provider` (issue 1779)"
        ]
    return []


def provider_findings(build: str, manifest: str) -> list[str]:
    out = []
    if not re.search(r'^\s*links\s*=\s*"nros_platform_cffi"\s*$', manifest, re.M):
        out.append(
            f'{PROVIDER_MANIFEST}: lost `links = "nros_platform_cffi"` — no dependent '
            "build script can read the provider any more (issue 1779)"
        )
    if "cargo:abi_provider=" not in build:
        out.append(
            f"{PROVIDER_BUILD}: no longer emits `cargo:abi_provider=` — every board "
            "port would compile beside the POSIX one again (issue 1779)"
        )
    return out


def self_test() -> int:
    fails = []
    bad = 'fn main() { let mut b = cc::Build::new(); b.compile("nros_platform_threadx"); }'
    if not violations("packages/boards/x/build.rs", bad):
        fails.append("a port compile with no ask was not reported")
    late = (
        'fn main() { b.compile("nros_platform_freertos");\n'
        ' if platform_port::defer_to_graph_provider("x", "y") { return; } }'
    )
    if not violations("packages/boards/x/build.rs", late):
        fails.append("an ask AFTER the compile was not reported")
    good = (
        'fn main() { if nros_board_common::platform_port::defer_to_graph_provider("x", "y")'
        ' { return; }\n b.compile("nros_platform_nuttx"); }'
    )
    if violations("packages/boards/x/build.rs", good):
        fails.append("a compliant script was reported")
    exempt = "// platform-port-provider-exempt: same sources, demand-driven\n" + bad
    if violations("packages/rmw/x/build.rs", exempt):
        fails.append("a reasoned exemption was reported")
    if not violations("packages/rmw/x/build.rs", "// platform-port-provider-exempt:\n" + bad):
        fails.append("an exemption with NO reason was honoured")
    if violations(PROVIDER_BUILD, bad):
        fails.append("the provider itself was reported")
    if not provider_findings('println!("x");', "[package]\n"):
        fails.append("a provider that states nothing was not reported")
    if fails:
        for f in fails:
            print(f"check-platform-port-single-provider SELF-TEST FAILED: {f}", file=sys.stderr)
        return 1
    return 0


def main(argv: list[str]) -> int:
    if self_test() != 0:
        return 1
    if argv[:1] == ["--self-test"]:
        print("check-platform-port-single-provider --self-test: OK")
        return 0
    findings = provider_findings(
        (ROOT / PROVIDER_BUILD).read_text(), (ROOT / PROVIDER_MANIFEST).read_text()
    )
    sites = 0
    for path in tracked_rust():
        text = (ROOT / path).read_text(errors="replace")
        if path != PROVIDER_BUILD and COMPILE_RE.search(comments.strip_comments(text, "rust")):
            sites += 1
        findings += violations(path, text)
    if findings:
        print("check-platform-port-single-provider: FAILED (issue 1779)", file=sys.stderr)
        for f in findings:
            print(f"  {f}", file=sys.stderr)
        return 1
    if sites == 0:
        print(
            "check-platform-port-single-provider: found NO port compile site — the "
            "pattern no longer matches the tree, so this gate is checking nothing",
            file=sys.stderr,
        )
        return 1
    print(
        f"check-platform-port-single-provider: OK ({sites} port compile site(s), each asks "
        "the graph's provider first; nros-platform-cffi states it)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
