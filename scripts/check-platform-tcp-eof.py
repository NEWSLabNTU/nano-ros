#!/usr/bin/env python3
"""Every platform port's TCP read reports end-of-stream as its own result.

Issue 2779. zenoh-pico closes a session, and so reconnects, when a TCP read
returns end-of-stream (`NROS_PLATFORM_NET_SOCKET_EOF`, Rust
`nros_platform_api::NET_SOCKET_EOF`). A port whose read hands a peer's close up
as 0 ("nothing yet") or as an error leaves a client whose router died polling a
dead socket forever (issue 2774's symptom). One port left behind is the whole
bug on that target, so the rule is about EVERY implementation:

* each C definition of `nros_platform_tcp_read` and `nros_platform_tcp_read_exact`
  in a tracked `.c` file names `NROS_PLATFORM_NET_SOCKET_EOF` in its body, or
  calls a helper in the same file that does;
* each Rust `impl ... PlatformTcp for` names `NET_SOCKET_EOF` in its `fn read`
  and its `fn read_exact`;
* each of zenoh-pico's own TCP links that reads with a BSD `recv(` (Zephyr and
  FreeRTOS+lwIP read through these, not through the ports) names
  `_Z_LINK_READ_EOF`.

This is a reach check, not a behaviour test: the POSIX port's behaviour is
asserted by `nros-platform-cffi`'s `c_port_posix_net` test, and each port by
the two-router re-open measurement in issue 2779.

    check-platform-tcp-eof.py [--selftest]
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import check_skip  # noqa: E402
from source_locate import located_or_dest  # noqa: E402

NAME = "platform-tcp-eof"
C_FUNCS = ("nros_platform_tcp_read", "nros_platform_tcp_read_exact")
C_TOKEN = "NROS_PLATFORM_NET_SOCKET_EOF"
RUST_FUNCS = ("read", "read_exact")
RUST_TOKEN = "NET_SOCKET_EOF"
ZP_TOKEN = "_Z_LINK_READ_EOF"


def body_after(text: str, start: int) -> str:
    """The brace-balanced block that opens at the first `{` from `start`."""
    i = text.index("{", start)
    depth = 0
    for j in range(i, len(text)):
        if text[j] == "{":
            depth += 1
        elif text[j] == "}":
            depth -= 1
            if depth == 0:
                return text[i : j + 1]
    return text[i:]


def c_function_bodies(text: str) -> dict:
    """Every `static`/non-static function defined in a C file, by name."""
    out = {}
    for m in re.finditer(
        r"^(?:static\s+)?(?:inline\s+)?[A-Za-z_][\w\s\*]*?\b([A-Za-z_]\w*)\s*\([^;{]*\)\s*\{",
        text,
        re.M,
    ):
        out[m.group(1)] = body_after(text, m.start())
    return out


def c_problems(path: str, text: str) -> list:
    bodies = c_function_bodies(text)
    problems = []
    for name in C_FUNCS:
        if name not in bodies:
            continue
        body = bodies[name]
        ok = C_TOKEN in body
        if not ok:
            # One level of helper, in the same file (ThreadX's blocking recv).
            for callee, cbody in bodies.items():
                if callee != name and re.search(rf"\b{callee}\s*\(", body) and C_TOKEN in cbody:
                    ok = True
                    break
        if not ok:
            problems.append(f"{path}: `{name}` never returns {C_TOKEN}")
    return problems


def rust_problems(path: str, text: str) -> list:
    problems = []
    for m in re.finditer(r"impl\s+[\w:$]*PlatformTcp\s+for\s+[\w:$]+\s*\{", text):
        block = body_after(text, m.start())
        for name in RUST_FUNCS:
            fm = re.search(rf"\bfn\s+{name}\s*\(", block)
            if fm is None:
                continue
            fbody = body_after(block, fm.start())
            # A forwarder to the C port (`nros-platform-cffi`) returns whatever
            # the C read returns; the C side is what is checked.
            if re.search(r"\bnros_platform_tcp_read(_exact)?\s*\(", fbody):
                continue
            if RUST_TOKEN not in fbody:
                problems.append(f"{path}: `PlatformTcp::{name}` never returns {RUST_TOKEN}")
    return problems


def zp_problems(path: str, text: str) -> list:
    """A zenoh-pico TCP link that reads with BSD `recv(` must report EOF."""
    if re.search(r"(?<![\w.>])recv\s*\(", text) and ZP_TOKEN not in text:
        return [f"{path}: reads with BSD recv() but never returns {ZP_TOKEN}"]
    return []


def zp_tcp_dir() -> Path:
    return located_or_dest("zenoh-pico") / "src" / "link" / "transport" / "tcp"


def zp_scan() -> tuple:
    d = zp_tcp_dir()
    if not d.is_dir():
        return [], 0
    problems, seen = [], 0
    for f in sorted(d.glob("tcp_*.c")):
        seen += 1
        problems += zp_problems(f"zenoh-pico {f.name}", f.read_text(errors="replace"))
    return problems, seen


def scan() -> list:
    files = subprocess.run(
        ["git", "ls-files", "*.c", "*.rs"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()
    problems = []
    seen_c = seen_rust = 0
    for rel in files:
        if rel.startswith("third-party/") or "/generated" in rel:
            continue
        text = (ROOT / rel).read_text(errors="replace")
        if rel.endswith(".c") and "nros_platform_tcp_read" in text:
            if any(n in c_function_bodies(text) for n in C_FUNCS):
                seen_c += 1
            problems += c_problems(rel, text)
        elif rel.endswith(".rs") and "PlatformTcp for" in text:
            seen_rust += 1
            problems += rust_problems(rel, text)
    zp, seen_zp = zp_scan()
    problems += zp
    if seen_zp == 0:
        problems.append("ZP_ABSENT")
    if seen_c == 0 or seen_rust == 0:
        problems.append(
            f"found {seen_c} C and {seen_rust} Rust TCP read implementation(s); "
            "the scan no longer reaches the ports it exists for"
        )
    return problems


def selftest() -> int:
    good_c = (
        "size_t nros_platform_tcp_read(const void *s, uint8_t *b, size_t n) {\n"
        "    if (r == 0 && n > 0) return NROS_PLATFORM_NET_SOCKET_EOF;\n    return r;\n}\n"
    )
    bad_c = "size_t nros_platform_tcp_read(const void *s, uint8_t *b, size_t n) {\n    return r;\n}\n"
    helper_c = (
        "static size_t blocking(int fd) {\n    return NROS_PLATFORM_NET_SOCKET_EOF;\n}\n"
        "size_t nros_platform_tcp_read_exact(const void *s, uint8_t *b, size_t n) {\n"
        "    return blocking(1);\n}\n"
    )
    good_rs = (
        "impl $crate::PlatformTcp for crate::P {\n"
        "    fn read(s: *const c_void) -> usize { if x { return $crate::NET_SOCKET_EOF; } 0 }\n"
        "    fn read_exact(s: *const c_void) -> usize { $crate::NET_SOCKET_EOF }\n}\n"
    )
    bad_rs = (
        "impl PlatformTcp for P {\n"
        "    fn read(s: *const c_void) -> usize { 0 }\n"
        "    fn read_exact(s: *const c_void) -> usize { NET_SOCKET_EOF }\n}\n"
    )
    fwd_rs = (
        "impl PlatformTcp for CffiPlatform {\n"
        "    fn read(s: *const c_void) -> usize { unsafe { nros_platform_tcp_read(s) } }\n}\n"
    )
    cases = [
        (zp_problems("z.c", "rb = recv(fd, p, n, 0); if (rb == 0) return _Z_LINK_READ_EOF;"), 0),
        (zp_problems("z.c", "ssize_t rb = recv(fd, p, n, 0); return rb;"), 1),
        (zp_problems("z.c", "rb = FreeRTOS_recv(s, p, n, 0); rb = sock._tcp->recv(p, n);"), 0),
        (rust_problems("fwd.rs", fwd_rs), 0),
        (c_problems("good.c", good_c), 0),
        (c_problems("bad.c", bad_c), 1),
        (c_problems("helper.c", helper_c), 0),
        (rust_problems("good.rs", good_rs), 0),
        (rust_problems("bad.rs", bad_rs), 1),
    ]
    for i, (got, want) in enumerate(cases):
        if len(got) != want:
            print(f"check-platform-tcp-eof selftest: case {i} gave {got}, wanted {want} problem(s)")
            return 1
    print(f"check-platform-tcp-eof selftest: OK ({len(cases)} cases)")
    return 0


def main() -> int:
    if selftest() != 0:
        return 1
    if "--selftest" in sys.argv:
        return 0
    problems = scan()
    if "ZP_ABSENT" in problems:
        problems.remove("ZP_ABSENT")
        if not problems:
            return check_skip.unverified(
                NAME,
                f"zenoh-pico's TCP links are not checked out at {zp_tcp_dir()} "
                "(`nros setup --source zenoh-pico`)",
            )
        problems.append(f"(zenoh-pico's TCP links at {zp_tcp_dir()} were not read: not checked out)")
    if problems:
        print("check-platform-tcp-eof: FAIL — a TCP read hands a peer's close up as 0 or")
        print("an error, so a client whose router died never reopens on that port (issue 2779):")
        for p in problems:
            print(f"  {p}")
        return 1
    print("check-platform-tcp-eof: OK — every port's TCP read reports end-of-stream")
    return 0


if __name__ == "__main__":
    sys.exit(main())
