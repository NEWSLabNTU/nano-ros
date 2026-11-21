#!/usr/bin/env python3
"""Every host BSD `send(`/`sendto(` in our C passes `MSG_NOSIGNAL`.

Issue 2799. A process that writes to a TCP connection its peer has reset gets
SIGPIPE, and SIGPIPE's default action ends the process with no message. On a
host-sim image the RTOS sockets are the host's, so a client whose router was
killed died the moment zenoh-pico sent its CLOSE down the dead link:
threadx-linux's NSOS shim called `send(sockID, msg, len, flags)` with nothing
masking the signal. The POSIX port and zenoh-pico's own POSIX link already pass
`MSG_NOSIGNAL`; one site without it is the whole bug on that target.

The rule is about every call, not one shim, so it scans every tracked `.c`
under `packages/` for a bare `send(`/`sendto(` call (not a definition, not a
member or prefixed name such as `zsock_send(` or `sock->send(`) and requires
`MSG_NOSIGNAL` in its arguments, or in a flags variable set within a few lines
above. A call that cannot raise SIGPIPE (a datagram socket) states why with a
`nosignal-exempt: <reason>` comment on its line or the line above.

    check-host-send-nosignal.py [--selftest]
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NAME = "host-send-nosignal"
CALL = re.compile(r"(?<![\w.>])(send|sendto)\s*\(")


def args_at(text: str, open_paren: int) -> str:
    """The balanced argument text from the `(` at `open_paren`."""
    depth = 0
    for j in range(open_paren, len(text)):
        if text[j] == "(":
            depth += 1
        elif text[j] == ")":
            depth -= 1
            if depth == 0:
                return text[open_paren + 1 : j]
    return text[open_paren + 1 :]


EXEMPT = "nosignal-exempt:"
NEAR_LINES = 8


def strip_comments(text: str) -> str:
    """Blank comments and string literals, keeping line numbers. An exemption
    comment is kept as a marker the scan can see."""
    def blank(m):
        body = m.group(0)
        keep = EXEMPT if EXEMPT in body else ""
        return keep + re.sub(r"[^\n]", " ", body[len(keep):])
    text = re.sub(r"/\*.*?\*/", blank, text, flags=re.S)
    text = re.sub(r"//[^\n]*", blank, text)
    return re.sub(r'"(?:\\.|[^"\\\n])*"', lambda m: " " * len(m.group(0)), text)


def problems_in(path: str, text: str) -> list:
    text = strip_comments(text)
    out = []
    for m in CALL.finditer(text):
        line_start = text.rfind("\n", 0, m.start()) + 1
        before = text[line_start : m.start()]
        # A declaration or definition (`ssize_t send(int fd, ...)`), not a call.
        if re.search(r"\b(ssize_t|int|INT|size_t)\s*\**\s*$", before):
            continue
        args = args_at(text, m.end() - 1)
        line_no = text.count("\n", 0, m.start())
        lines = text.split("\n")
        near = "\n".join(lines[max(0, line_no - NEAR_LINES) : line_no + 1])
        # A stated exemption on the call's line or the line above.
        if EXEMPT in "\n".join(lines[max(0, line_no - 1) : line_no + 1]):
            continue
        # The flag in the call, or in a flags variable set just above it
        # (`int flags = MSG_NOSIGNAL;` then `send(fd, p, n, flags)`).
        if "MSG_NOSIGNAL" not in args and "MSG_NOSIGNAL" not in near:
            line = text.count("\n", 0, m.start()) + 1
            out.append(f"{path}:{line}: `{m.group(1)}(` without MSG_NOSIGNAL")
    return out


def scan() -> tuple:
    files = subprocess.run(
        ["git", "ls-files", "packages/*.c", "packages/**/*.c"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()
    problems, calls = [], 0
    for rel in sorted(set(files)):
        if "/third-party/" in rel or "/generated" in rel:
            continue
        text = (ROOT / rel).read_text(errors="replace")
        calls += len(CALL.findall(strip_comments(text)))
        problems += problems_in(rel, text)
    return problems, calls


def selftest() -> int:
    cases = [
        ("ssize_t n = send(fd, p, n, flags | MSG_NOSIGNAL);", 0),
        ("ssize_t n = send(fd, p, n, flags);", 1),
        ("r = sendto(fd, p, n, MSG_NOSIGNAL, a, l);", 0),
        ("r = sendto(fd, p, n, 0,\n    a, l);", 1),
        ("r = zsock_send(fd, p, n, 0); r = sock->send(p, n); r = FreeRTOS_send(s, p, n, 0);", 0),
        ("ssize_t send(int fd, const void *b, size_t n, int f) { return 0; }", 0),
        ("/* send(fd, p, n, 0) */ // sendto(fd, p, n, 0, a, l)", 0),
        ('printf("%d send(s) failed", n);', 0),
        ("int flags = MSG_NOSIGNAL;\nssize_t r = send(fd, p, n, flags);", 0),
        ("/* nosignal-exempt: UDP, no SIGPIPE */\nr = sendto(fd, p, n, 0, a, l);", 0),
        ("/* a different note */\nr = sendto(fd, p, n, 0, a, l);", 1),
    ]
    for i, (src, want) in enumerate(cases):
        got = problems_in("t.c", src)
        if len(got) != want:
            print(f"check-{NAME} selftest: case {i} gave {got}, wanted {want} problem(s)")
            return 1
    print(f"check-{NAME} selftest: OK ({len(cases)} cases)")
    return 0


def main() -> int:
    if selftest() != 0:
        return 1
    if "--selftest" in sys.argv:
        return 0
    problems, calls = scan()
    if calls == 0:
        print(f"check-{NAME}: FAIL: found no host send()/sendto() call at all; "
              "the scan no longer reaches the sockets it exists for")
        return 1
    if problems:
        print(f"check-{NAME}: FAIL: a host send to a reset peer raises SIGPIPE, which "
              "ends the process with no message (issue 2799):")
        for p in problems:
            print(f"  {p}")
        return 1
    print(f"check-{NAME}: OK: {calls} host send()/sendto() call(s), each passing MSG_NOSIGNAL "
          "or stating why it cannot raise SIGPIPE")
    return 0


if __name__ == "__main__":
    sys.exit(main())
