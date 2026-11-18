#!/usr/bin/env python3
"""check-changelog-issue-links: release notes carry no issue reference.

A changelog fragment named `NNNN.<type>.md` carries the docs/issues id NNNN,
not a GitHub issue number. towncrier renders that number through
`[tool.towncrier] issue_format`, and the old format turned it into a link to
`github.com/NEWSLabNTU/nano-ros/issues/NNNN`: an unrelated GitHub item (the
0.6.0 notes linked `2041` to a PR about something else). A bare `#NNNN` is no
better, since a reader cannot look the id up. So the format must render
nothing, and CHANGELOG.md must hold no such link.

Three readings:

* the config: `issue_format`, formatted with an issue number, is empty;
* CHANGELOG.md: no `github.com/.../issues/` link;
* the render: where towncrier is installed (`just dev-tools --install`), one
  numbered fragment is built with this repo's `[tool.towncrier]` and the
  output must hold its text and neither the number nor an issues link.

towncrier references an issue only through `issue_format.format(issue=...)`,
and skips it when that is empty, so the config reading decides the render on
a host without towncrier. That host prints which readings ran; it is not a
skip.

  check-changelog-issue-links.py             the three readings
  check-changelog-issue-links.py --self-test negative controls
  check-changelog-issue-links.py --gate      self-test, then the readings
"""
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    tomllib = None

ROOT = Path(__file__).resolve().parents[2]
NAME = "check-changelog-issue-links"
ISSUE_LINK = re.compile(r"github\.com/[^/\s]+/[^/\s]+/issues/\d+")
PROBE_ISSUE = "2041"
PROBE_TEXT = "A numbered fragment's change."


def issue_format(pyproject: str) -> str | None:
    if tomllib is not None:
        return tomllib.loads(pyproject)["tool"]["towncrier"].get("issue_format", None)
    m = re.search(r'^issue_format\s*=\s*"((?:[^"\\]|\\.)*)"\s*$', pyproject, re.M)
    return m.group(1) if m else None


def config_problems(pyproject: str) -> list[str]:
    fmt = issue_format(pyproject)
    if fmt is None:
        return ["pyproject.toml: `[tool.towncrier] issue_format` is unset, so towncrier "
                "renders a numbered fragment as `#NNNN`; set it to \"\""]
    rendered = fmt.format(issue=PROBE_ISSUE)
    if rendered:
        return [f"pyproject.toml: `issue_format` renders issue {PROBE_ISSUE} as "
                f"{rendered!r}; a fragment's number is a docs/issues id, so render "
                "nothing (\"\")"]
    return []


def changelog_problems(text: str) -> list[str]:
    return [f"CHANGELOG.md:{n}: links an issue: {m.group(0)}"
            for n, line in enumerate(text.splitlines(), 1)
            for m in ISSUE_LINK.finditer(line)]


def towncrier() -> list[str] | None:
    exe = shutil.which("towncrier")
    if exe:
        return [exe]
    try:
        subprocess.run([sys.executable, "-m", "towncrier", "--version"],
                       capture_output=True, check=True)
    except (OSError, subprocess.CalledProcessError):
        return None
    return [sys.executable, "-m", "towncrier"]


def render_problems(cmd: list[str], pyproject: str) -> list[str]:
    with tempfile.TemporaryDirectory() as tmp:
        t = Path(tmp)
        (t / "pyproject.toml").write_text(pyproject)
        (t / "changelog.d").mkdir()
        (t / "changelog.d" / f"{PROBE_ISSUE}.fix.md").write_text(PROBE_TEXT + "\n")
        (t / "CHANGELOG.md").write_text("<!-- towncrier release notes start -->\n")
        r = subprocess.run(cmd + ["build", "--draft", "--version", "0.0.0"],
                           cwd=t, capture_output=True, text=True)
        out = r.stdout + r.stderr
    if r.returncode != 0:
        return [f"towncrier build --draft failed ({r.returncode}): {out.strip()[:300]}"]
    bad = []
    if PROBE_TEXT not in out:
        bad.append(f"the rendered notes do not hold the fragment's text: {out.strip()[:300]}")
    link = ISSUE_LINK.search(out)
    if link:
        bad.append(f"the rendered notes link an issue: {link.group(0)}")
    if PROBE_ISSUE in out:
        bad.append(f"the rendered notes cite the fragment's number {PROBE_ISSUE}")
    return bad


def readings() -> int:
    pyproject = (ROOT / "pyproject.toml").read_text()
    bad = config_problems(pyproject) + changelog_problems((ROOT / "CHANGELOG.md").read_text())
    ran = "config, CHANGELOG.md"
    cmd = towncrier()
    if cmd is not None:
        bad += render_problems(cmd, pyproject)
        ran += ", towncrier render"
    else:
        ran += "; no towncrier here, so the config reading stands for the render"
    for b in bad:
        print(f"{NAME}: {b}", file=sys.stderr)
    if bad:
        return 1
    print(f"{NAME}: OK ({ran})")
    return 0


OLD = 'issue_format = "[#{issue}](https://github.com/NEWSLabNTU/nano-ros/issues/{issue})"'


def self_test() -> int:
    pyproject = (ROOT / "pyproject.toml").read_text()
    linked = re.sub(r"^issue_format = .*$", lambda _: OLD, pyproject, flags=re.M)
    unset = re.sub(r"^issue_format = .*\n", "", pyproject, flags=re.M)
    cases = [
        ("the old link format", bool(config_problems(linked))),
        ("an unset format", bool(config_problems(unset))),
        ("the repo's format passes", not config_problems(pyproject)),
        ("a CHANGELOG link", bool(changelog_problems(
            "- x. ([#1](https://github.com/NEWSLabNTU/nano-ros/issues/1))"))),
        ("a CHANGELOG without one", not changelog_problems("- x.\n")),
    ]
    cmd = towncrier()
    if cmd is not None:
        cases += [
            ("a render with the old format", bool(render_problems(cmd, linked))),
            ("a render with no format", bool(render_problems(cmd, unset))),
        ]
    failed = [c for c, ok in cases if not ok]
    for c in failed:
        print(f"{NAME} self-test: not caught: {c}", file=sys.stderr)
    if failed:
        return 1
    print(f"{NAME} self-test: OK ({len(cases)} cases)")
    return 0


def main(argv: list[str]) -> int:
    if argv == ["--self-test"]:
        return self_test()
    if argv == ["--gate"]:
        return self_test() or readings()
    if not argv:
        return readings()
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
