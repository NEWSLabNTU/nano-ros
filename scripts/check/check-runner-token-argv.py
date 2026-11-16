#!/usr/bin/env python3
"""check-runner-token-argv: no credential on a process's command line.

A command-line argument is in the host's process table for as long as the
process runs, and `ps` shows it to every user on the machine. Two places put
a runner registration token there:

* the `docker run` / `create` line that starts a runner container, whose
  client stays alive for the container's whole life in foreground mode
  (`-e RUNNER_TOKEN=...`); and
* `config.sh --token ...`, which runs INSIDE the container but is still a
  process in the host's table.

The fix is the same for both: the value goes through the environment, never
argv: `--env-file` for the container engine, `ACTIONS_RUNNER_INPUT_TOKEN` for
`config.sh`. This gate refuses either shape coming back, in every tracked shell
script, `just` file, workflow and Markdown code sample.

Continued lines (a trailing backslash) are joined first, so a flag on its own
line belongs to the command it continues. Comment lines are ignored.

Usage: check-runner-token-argv.py [--self-test]
"""
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GLOBS = ["*.sh", "*.just", "justfile", "*.yml", "*.yaml", "*.md", "*.golden"]
SKIP_PREFIXES = ("third-party/",)

ENGINE = r"(?:docker|podman|\$\{?ENGINE\}?|\"\$\{?ENGINE\}?\")"
LAUNCH = re.compile(ENGINE + r"\s+(?:container\s+)?(?:run|create)\b")
SECRET_ENV = re.compile(
    r"(?:^|\s)(?:-e|--env)(?:\s+|=)[\"']?[A-Za-z_]*(?:TOKEN|SECRET|PASSWORD)[A-Za-z_]*=")
CONFIG_SH = re.compile(r"config\.sh\b")
TOKEN_FLAG = re.compile(r"(?:^|\s)--token(?:\s|=)")


def logical_lines(text):
    """Yield (first line number, joined command) with continuations folded."""
    buf, start = [], None
    for n, raw in enumerate(text.splitlines(), 1):
        line = raw.rstrip()
        stripped = line.lstrip()
        if not buf and stripped.startswith("#"):
            continue
        if buf and stripped.startswith("#"):
            # A comment inside a continued command ends nothing in bash only
            # when quoted; treat it as text-free.
            continue
        if start is None:
            start = n
        if line.endswith("\\"):
            buf.append(line[:-1])
            continue
        buf.append(line)
        yield start, " ".join(buf)
        buf, start = [], None
    if buf:
        yield start, " ".join(buf)


def scan_text(text):
    """Return a list of (line, reason) findings."""
    out = []
    for n, cmd in logical_lines(text):
        if LAUNCH.search(cmd) and SECRET_ENV.search(cmd):
            out.append((n, "a credential passed with -e/--env on a container launch "
                           "(use --env-file with a 0600 file, removed after create)"))
        if CONFIG_SH.search(cmd) and TOKEN_FLAG.search(cmd):
            out.append((n, "config.sh --token (pass ACTIONS_RUNNER_INPUT_TOKEN in "
                           "the environment instead)"))
    return out


def tracked_files():
    r = subprocess.run(["git", "-C", str(ROOT), "ls-files", "-z", "--", *GLOBS],
                       capture_output=True, check=True)
    for p in r.stdout.decode().split("\0"):
        if p and not p.startswith(SKIP_PREFIXES):
            yield p


def self_test():
    bad = [
        'docker run -d --name x \\\n    -e RUNNER_TOKEN="$RUNNER_TOKEN" \\\n    img\n',
        '"$ENGINE" create --name x -e GH_TOKEN=abc img\n',
        'podman run --env=MY_SECRET=x img\n',
        './config.sh \\\n  --url u \\\n  --token "$TOKEN" \\\n  --replace\n',
    ]
    good = [
        '"$ENGINE" create --name x \\\n    --env-file "$TOKEN_FILE" \\\n    img\n',
        'ACTIONS_RUNNER_INPUT_TOKEN="$t" ./config.sh --url u --replace\n',
        '# docker run -e RUNNER_TOKEN=x  (a comment)\n',
        'docker run -e NROS_RUNNER_LABELS="$LABELS" img\n',
        'gh api -X POST x --jq .token\n',
    ]
    ok = True
    for s in bad:
        if not scan_text(s):
            print(f"self-test: MISSED {s!r}", file=sys.stderr)
            ok = False
    for s in good:
        if scan_text(s):
            print(f"self-test: FALSE POSITIVE {s!r}", file=sys.stderr)
            ok = False
    return ok


def main():
    if not self_test():
        print("check-runner-token-argv: SELF-TEST FAILED", file=sys.stderr)
        return 1
    if "--self-test" in sys.argv[1:]:
        print("check-runner-token-argv self-test: OK")
        return 0
    findings, examined = [], 0
    for rel in tracked_files():
        path = ROOT / rel
        try:
            text = path.read_text(errors="replace")
        except (IsADirectoryError, FileNotFoundError):
            continue
        examined += 1
        for n, why in scan_text(text):
            findings.append(f"  {rel}:{n}: {why}")
    if findings:
        print("check-runner-token-argv: a credential on a command line is readable "
              "with `ps` by every user on the host:", file=sys.stderr)
        print("\n".join(findings), file=sys.stderr)
        return 1
    print(f"check-runner-token-argv: OK ({examined} file(s); no credential on a "
          "container launch or a config.sh line)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
