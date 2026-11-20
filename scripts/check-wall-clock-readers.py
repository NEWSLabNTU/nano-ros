#!/usr/bin/env python3
"""A duration never reads the WALL clock (issue 2760).

`nros_platform_time_now_ns()` is the wall clock: UNIX time, or 0 when the
port has none, and it steps when the host's clock is set. Elapsed time,
timeouts and deadlines read `nros_platform_clock_ns()`, the monotonic one.

Issue 2760 found seven readers timing durations on the wall clock (smoltcp's
`Instant`, its deadline and idle wait, zpico's `smoltcp_clock_now_ms`, the
zenoh shim's deadline QoS). On FreeRTOS and ThreadX every one of them stood
still, and two bare-metal boards returned UPTIME as wall time to keep them
moving: a stamp that looked real and was not.

Which reader is a duration and which a stamp is a question about intent, not
syntax, so this gate does not guess. Every file that reads the wall clock is
named below with WHY it wants the wall clock (a stamp, or a wall-time API).
A file that starts reading it fails until someone writes that reason down,
and an entry whose file no longer reads it fails too, so the list cannot
silently outgrow the code.

What counts as a read: a CALL `nros_platform_time_now_ns()` or a trait call
`::time_now_ns()`, in code (comments and strings stripped). Declarations
(`fn nros_platform_time_now_ns() -> u64;`) and C definitions (`(void)`) are
not reads. bindgen's `generated.rs` is not scanned.

  check-wall-clock-readers.py              the gate
  check-wall-clock-readers.py --self-test  its cases only
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
from comments import lang_for, strip_comments  # noqa: E402

# Every file that reads the wall clock, and why the WALL clock is right there.
WALL_CLOCK_READERS = {
    "packages/api/nros-c/src/platform.rs":
        "the C surface's RCL_SYSTEM_TIME reading, a wall-time API",
    "packages/core/nros-core/src/clock.rs":
        "Clock::system(), a wall-time API",
    "packages/core/nros-node/src/executor/types.rs":
        "the executor's system-time (epoch) source, a wall-time API",
    "packages/platform/nros-platform-cffi/src/lib.rs":
        "the PlatformTime forwarder and export macro, the seam itself",
    "packages/platform/nros-platform-cffi/tests/c_stub_platform.rs":
        "exercises every ABI symbol once",
    "packages/platform/nros-platform-mps2-an385/src/lib.rs":
        "its host test asserts the wall clock reads 0 (no RTC)",
    "packages/platform/nros-platform-stm32f4/src/lib.rs":
        "its host test asserts the wall clock reads 0 (no RTC)",
    "packages/rmw/xrce/nros-rmw-xrce/src/session.c":
        "received_timestamp, a stamp",
    "packages/rmw/xrce/nros-rmw-xrce/tests/received_timestamp.c":
        "tests that stamp",
    "packages/rmw/zenoh/nros-rmw-zenoh/src/shim/mod.rs":
        "received_timestamp, a stamp",
    "packages/rmw/zenoh/nros-rmw-zenoh/tests/zenoh_integration.rs":
        "brackets the stamps under test",
    "packages/rmw/zenoh/zpico-sys/c/zpico/platform_aliases.c":
        "zenoh-pico's z_time_* and time-since-epoch, its wall-time API",
    "packages/rmw/zenoh/zpico-sys/c/zpico/zpico.c":
        "receive stamps and a reply's restamp, stamps",
}

CALL = re.compile(r"(?:(?<![A-Za-z0-9_])nros_platform_time_now_ns|::time_now_ns)\s*\(\s*\)")
DECL = re.compile(r"\bfn\s+(?:nros_platform_)?time_now_ns\s*\(")
SKIP = ("packages/platform/nros-platform-cffi/src/generated.rs",)
EXTS = ("*.rs", "*.c", "*.h", "*.cpp", "*.hpp", "*.cc")


def reads(text: str, lang: str) -> list[int]:
    """1-based line numbers of wall-clock CALLS in code."""
    code = strip_comments(text, lang, strings=True)
    out = []
    for n, line in enumerate(code.splitlines(), 1):
        for m in CALL.finditer(line):
            # `fn nros_platform_time_now_ns() -> u64` declares, it does not call.
            if DECL.search(line[: m.end()]):
                continue
            out.append(n)
            break
    return out


def scan(files: dict[str, str]) -> dict[str, list[int]]:
    found = {}
    for path, text in files.items():
        lang = lang_for(path)
        if lang is None or path in SKIP:
            continue
        lines = reads(text, lang)
        if lines:
            found[path] = lines
    return found


def verdict(found: dict[str, list[int]], allowed: dict[str, str]) -> list[str]:
    problems = []
    for path in sorted(set(found) - set(allowed)):
        lines = ", ".join(str(n) for n in found[path][:5])
        problems.append(
            f"{path}:{lines} reads the WALL clock and is not a named wall-clock reader.\n"
            "      A duration (elapsed time, a timeout, a deadline) reads "
            "nros_platform_clock_ns(), the monotonic clock: the wall clock is 0 "
            "on a port without one and steps when the host's clock is set.\n"
            "      If this is a STAMP or a wall-time API, add the file to "
            "WALL_CLOCK_READERS in scripts/check-wall-clock-readers.py, saying which."
        )
    for path in sorted(set(allowed) - set(found)):
        problems.append(f"{path} is a named wall-clock reader and reads no wall clock any more: delete the entry")
    return problems


def tracked() -> dict[str, str]:
    out = subprocess.run(
        ["git", "ls-files", "-z", "--", *EXTS],
        cwd=ROOT, check=True, capture_output=True,
    ).stdout.decode()
    files = {}
    for path in filter(None, out.split("\0")):
        if not path.startswith(("packages/", "examples/")):
            continue
        p = ROOT / path
        if p.is_file():
            files[path] = p.read_text(errors="replace")
    return files


def self_test() -> int:
    bad = 0

    def case(name, cond):
        nonlocal bad
        if not cond:
            bad += 1
            print(f"  FAIL {name}")

    rs_call = "fn f() -> u64 { unsafe { nros_platform_time_now_ns() / 1_000_000 } }\n"
    rs_decl = 'unsafe extern "C" {\n    fn nros_platform_time_now_ns() -> u64;\n}\n'
    rs_trait = "let t = <P as PlatformTime>::time_now_ns();\n"
    rs_comment = "// was nros_platform_time_now_ns() before 2760\n"
    rs_string = 'let s = "nros_platform_time_now_ns()";\n'
    c_def = "uint64_t nros_platform_time_now_ns(void) { return 0; }\n"
    c_call = "uint64_t ns = nros_platform_time_now_ns();\n"
    mono = "let ns = unsafe { nros_platform_clock_ns() };\n"

    case("a Rust call is a read", reads(rs_call, "rust") == [1])
    case("a declaration is not", reads(rs_decl, "rust") == [])
    case("a trait call is a read", reads(rs_trait, "rust") == [1])
    case("a type-path trait call is a read", reads("let _ = CffiPlatform::time_now_ns();\n", "rust") == [1])
    case("a comment is not", reads(rs_comment, "rust") == [])
    case("a string is not", reads(rs_string, "rust") == [])
    case("a C definition is not", reads(c_def, "c") == [])
    case("a C call is a read", reads(c_call, "c") == [1])
    case("the monotonic clock is not", reads(mono, "rust") == [])

    allowed = {"a.rs": "a stamp"}
    case("a named reader passes", verdict({"a.rs": [1]}, allowed) == [])
    case("an unnamed reader fails", len(verdict({"a.rs": [1], "b.c": [3]}, allowed)) == 1)
    case("a stale entry fails", len(verdict({}, allowed)) == 1)
    # The issue's own regression: smoltcp timing its Instant on the wall clock.
    smoltcp = "smoltcp::time::Instant::from_millis(unsafe { nros_platform_time_now_ns() / 1_000_000 } as i64)\n"
    case("the 2760 shape fails unnamed",
         len(verdict(scan({"packages/drivers/net/nros-smoltcp/src/bridge.rs": smoltcp}), allowed)) == 2)

    if bad:
        print(f"check-wall-clock-readers self-test: {bad} case(s) FAILED")
        return 1
    print("check-wall-clock-readers self-test: OK (13 cases)")
    return 0


def main(argv: list[str]) -> int:
    if self_test():
        return 2
    if argv == ["--self-test"]:
        return 0
    found = scan(tracked())
    problems = verdict(found, WALL_CLOCK_READERS)
    if problems:
        print("check-wall-clock-readers: FAIL")
        for p in problems:
            print(f"  {p}")
        return 1
    print(f"check-wall-clock-readers: OK ({len(found)} file(s) read the wall clock, each named with why)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
