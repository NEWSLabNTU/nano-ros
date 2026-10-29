#!/usr/bin/env python3
"""A script that runs the PINNED make as a jobserver puts its directory on PATH.

Issue 2119 (issue 1638's class). A fan-out runs pinned make 4.4 by ABSOLUTE
path with `-j`, so its children inherit `MAKEFLAGS=--jobserver-auth=fifo:...`.
Those children find their own make by NAME: a cmake "Unix Makefiles" configure
(`cyclonedds-sys`'s build script) caches whatever `gmake` PATH resolves, then
joins the pool as a client. The system make 4.3 cannot parse a fifo jobserver
and dies with `invalid --jobserver-auth string 'fifo:...'`. In the main
checkout an inherited PATH entry happened to name a make 4.4; issue 1280's
re-rooting drops it in a linked worktree, so the bug surfaced only there, and
four drivers carried the fix as a comment ("the drivers that already prepend")
while a fifth did not.

The rule: in `justfile`, `just/*.just` and `scripts/*.sh`, a file that selects
the pinned make (`sdk-path make` or `nros_pinned_make`) and runs a make
VARIABLE in parallel (`-j`, `--jobserver-*`, a `make_args` array — the same
detector as `check-jobserver-makes-watched`) must set `PATH=` to a value naming
that make's directory, on a line BEFORE the run. Accepted spellings of the
directory: `nros_pinned_tool_path_prefix` (scripts/build/jobserver-pool.sh, the
one helper), `$(nros sdk-path make)/bin`, `$_sdk_make/bin`-style variables
assigned from `sdk-path make`, or a variable assigned `$(dirname "$<make>")`.
A bare `make -j` is out of scope: it IS PATH's make, so its children agree.

`--self-test` (also run on every normal invocation) is the negative control.
"""

import importlib.util
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

_spec = importlib.util.spec_from_file_location(
    "jobserver_makes_watched", os.path.join(ROOT, "scripts", "check-jobserver-makes-watched.py"))
_w = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_w)

SELECTS_PINNED = re.compile(r"sdk-path make\b|nros_pinned_make\b")
# A make run through a variable or the pinned helper — not a bare `make`.
VAR_MAKE = re.compile(r'"\$\{?(?:make_bin|pinned_make|MAKE)\}?"|"\$\(nros_pinned_make\)"')
PATH_SET = re.compile(r'(?:^|[\s;(])(?:export\s+)?PATH="([^"]*)"')
# A variable that holds the pinned make's directory (or its bin/ parent).
DIR_VAR = re.compile(
    r'(?:^|[\s;(])(?:local\s+)?(\w+)="\$\((?:"?\$\w+"?\s+)?(?:sdk-path make|dirname "\$\w+")[^"]*"')
DIRECT = ("nros_pinned_tool_path_prefix", "sdk-path make)/bin")


def parallel_var_makes(text):
    """[line] of every parallel make run through a variable."""
    out = []
    for n, line in _w.logical_lines(text):
        body = line.strip()
        if body.startswith(("echo ", "printf ")):
            continue
        for m in VAR_MAKE.finditer(line):
            tail = line[m.end():]
            e = _w.END.search(tail)
            seg = tail[: e.start()] if e else tail
            if _w.PARALLEL.search(seg):
                out.append(n)
                break
    return out


def path_exports(text):
    """[line] of every PATH= assignment naming the pinned make's directory."""
    dir_vars = set()
    for _, line in _w.logical_lines(text):
        for m in DIR_VAR.finditer(line):
            dir_vars.add(m.group(1))
    out = []
    for n, line in _w.logical_lines(text):
        for m in PATH_SET.finditer(line):
            val = m.group(1)
            if any(d in val for d in DIRECT) or any(
                    re.search(r"\$\{?%s\}?(?:/bin)?:" % re.escape(v), val) for v in dir_vars):
                out.append(n)
    return out


def scan(text):
    """[line] of every pinned parallel make with no PATH export before it."""
    if not SELECTS_PINNED.search(text):
        return []
    exports = path_exports(text)
    return [n for n in parallel_var_makes(text) if not any(e <= n for e in exports)]


def self_test():
    sel = 'make_bin="$(nros sdk-path make)/bin/make"\n'
    cases = (
        (sel + '"$make_bin" -j4 -f x.mk\n', 1, "pinned make -j, no export (the 2119 shape)"),
        (sel + 'export PATH="$(nros_pinned_tool_path_prefix "$make_bin"):$PATH"\n'
         '"$make_bin" -j4 -f x.mk\n', 0, "the helper"),
        (sel + 'export PATH="$(nros sdk-path make)/bin:$(nros sdk-path ninja)/bin:$PATH"\n'
         '"$make_bin" "${make_args[@]}" -f x.mk\n', 0, "the literal sdk-path spelling"),
        ('pinned_make="$(nros sdk-path make)/bin/make"\n'
         'pinned_tool_path="$(dirname "$pinned_make")"\nexport PATH="$pinned_tool_path:$PATH"\n'
         'env -u MAKEFLAGS "$pinned_make" -j8 -f x.mk\n', 0, "a dirname variable"),
        (sel + '"$make_bin" -j4 -f x.mk\nexport PATH="$(nros sdk-path make)/bin:$PATH"\n',
         1, "an export AFTER the run"),
        (sel + 'export PATH="/opt/other/bin:$PATH"\n"$make_bin" -j4 -f x.mk\n',
         1, "an export of something else"),
        ('python3 w.py -- env -u MAKEFLAGS \\\n'
         '    PATH="$(nros_pinned_tool_path_prefix "$pinned_make"):$PATH" \\\n'
         '    "$(nros_pinned_make)" -j"$n" -f x.mk\n', 0, "an env PATH= on the run itself"),
        ('"$(nros_pinned_make)" -j"$n" -f x.mk\n', 1, "nros_pinned_make, no export"),
        ("make -j4 -f x.mk\n", 0, "a bare make (PATH's own)"),
        (sel + '"$make_bin" olddefconfig\n', 0, "a serial pinned make"),
    )
    errs = []
    for text, want, what in cases:
        got = len(scan(text))
        if got != want:
            errs.append(f"self-test: {what}: {got} finding(s), want {want}")
    return errs


def tracked():
    out = subprocess.run(
        ["git", "ls-files", "-z", "--", "justfile", "just/*.just", "scripts/*.sh"],
        cwd=ROOT, capture_output=True, check=True).stdout.decode()
    return [p for p in out.split("\0") if p]


def main(argv):
    errs = self_test()
    if errs or "--self-test" in argv:
        for e in errs:
            print(e, file=sys.stderr)
        if errs:
            print("check-pinned-make-path-exported: SELF-TEST FAILED — the scan cannot "
                  "be trusted", file=sys.stderr)
            return 1
        print("check-pinned-make-path-exported: self-test OK")
        return 0
    findings, runners = [], 0
    files = tracked()
    for path in files:
        with open(os.path.join(ROOT, path), encoding="utf-8", errors="replace") as fh:
            text = fh.read()
        if SELECTS_PINNED.search(text) and parallel_var_makes(text):
            runners += 1
        for n in scan(text):
            findings.append(f"  {path}:{n}")
    if findings:
        print("check-pinned-make-path-exported: FAILED (issue 2119) — the pinned make "
              "runs as a jobserver but its directory is not on PATH first, so a child "
              "that finds `gmake`/`make` by name (a cmake \"Unix Makefiles\" configure) "
              "gets the system make 4.3, which dies on `--jobserver-auth=fifo:`:",
              file=sys.stderr)
        print("\n".join(findings), file=sys.stderr)
        print('  Before the run: export PATH="$(nros_pinned_tool_path_prefix "$make_bin"):$PATH"'
              "  (scripts/build/jobserver-pool.sh)", file=sys.stderr)
        return 1
    print(f"check-pinned-make-path-exported: OK — {runners} pinned-jobserver script(s) "
          f"of {len(files)} put the pinned make on PATH (self-test OK)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
