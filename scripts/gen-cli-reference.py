#!/usr/bin/env python3
"""Generate book/src/reference/cli.md from the CLI's own help (RFC-0124 D7).

The page is `nros --help`, then `--help` of every user-tier verb and of every
subcommand each one lists, in the groups and the order `nros --help` prints
them. The text is clap's, verbatim, in a `text` block per command: nothing on
the page is typed by hand except the short header and the "where it is
explained" table, which only links.

User tier only. `nros --help` does not list the build-system tier (`nros dev`)
or the older spellings, and clap does not list a hidden subcommand under its
parent, so walking what each help lists reaches exactly the user tier.
Extension commands (`nros px4`, `nros router`) are packages, not built-ins:
their help is theirs, and their pages document them.

The help is rendered with a scrubbed environment (no `NROS_*`, a scratch
`HOME`, no colour) from a scratch directory, so a variable or a workspace on
the generating machine cannot reach the page.

The page is COMMITTED and gated like the board pages. `--check` renders the
help of this checkout's CLI (`NROS_CLI`, or the in-tree build) and refuses
any difference. A CLI whose source stamp is stale is refused for generation
and reported NOT VERIFIED by `--check` (`just check cli-fresh` owns that
failure); no CLI at all is NOT VERIFIED too, never a quiet pass.

Run:  python3 scripts/gen-cli-reference.py [--check | --self-test]
Gate: just check cli-reference
"""

from __future__ import annotations

import os
import pathlib
import re
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent
sys.path.insert(0, str(HERE / "lib"))
import book_pages  # noqa: E402

SEEDED = "reference/cli.md"
NAME = "cli-reference"
MAX_DEPTH = 4

# clap prints commands under "Commands:" or under a `help_heading` group
# ("Build:", "Run on a target:"); arguments and options are not verbs. The
# same reading `check-user-surface-ids` walks the help with.
GROUP_HEADER = re.compile(r"^([A-Z][\w ,/-]*):\s*$")
NOT_VERBS = {"Options", "Arguments", "Quick start"}
COMMAND_ROW = re.compile(r"^  ([a-z][a-z0-9-]*)(?:,\s*[a-z0-9-]+)*(?:\s|$)")

# Where each verb is explained as a task. Links only: the facts are the help's.
EXPLAINED = [
    ("nros setup", "Install", "../start/install.md"),
    ("nros setup, pin", "Install options, pinning, working offline", "../guide/build/install-options.md"),
    ("nros new", "Run a talker on an emulated board", "../tutorial/run-a-talker.md"),
    ("nros build, list", "Organize a larger workspace", "../guide/build/organize-a-larger-workspace.md"),
    ("nros configure, config", "Configure an image", "../guide/build/configure-an-image.md"),
    ("nros run, flash, monitor, bench", "Deploy to production", "../guide/deploy/deploy-to-production.md"),
    ("nros interface", "Message generation", "message-generation.md"),
    ("nros doctor", "Troubleshoot", "../guide/deploy/troubleshoot.md"),
    ("nros locate", "Build inside an existing CMake or vendor tree",
     "../guide/build/existing-cmake-or-vendor-tree.md"),
    ("nros toolchain", "Move a ROS 2 project over", "../guide/port/move-a-ros2-project.md"),
]


def groups(help_text: str) -> list[tuple[str, list[str]]]:
    """[(group heading, [verb, ...])] in the order the help lists them."""
    out: list[tuple[str, list[str]]] = []
    inside, usage_seen = False, False
    for line in help_text.splitlines():
        if line.startswith("Usage:"):
            usage_seen = True
            continue
        if not usage_seen:
            continue
        m = GROUP_HEADER.match(line)
        if m:
            inside = m.group(1) not in NOT_VERBS
            if inside:
                out.append((m.group(1), []))
            continue
        if inside:
            if line and not line.startswith(" "):
                inside = False
                continue
            m = COMMAND_ROW.match(line)
            if m and m.group(1) != "help" and m.group(1) not in out[-1][1]:
                out[-1][1].append(m.group(1))
    return [(g, vs) for g, vs in out if vs]


def subcommands(help_text: str) -> list[str]:
    return [v for _, vs in groups(help_text) for v in vs]


class Cli:
    def __init__(self, path: str):
        self.path = path
        self.tmp = tempfile.mkdtemp(prefix="nros-help-")

    def help(self, args: list[str]) -> str:
        env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "HOME": self.tmp,
               "NO_COLOR": "1", "CLICOLOR": "0", "LANG": "C", "LC_ALL": "C"}
        r = subprocess.run([self.path, *args, "--help"], cwd=self.tmp, env=env,
                           capture_output=True, text=True, timeout=60)
        if r.returncode != 0:
            sys.exit(f"gen-cli-reference: `nros {' '.join(args)} --help` exited {r.returncode}:\n"
                     f"{r.stderr.strip()}")
        return r.stdout.rstrip() + "\n"


def fence(text: str) -> str:
    """A fence longer than any backtick run in the text, so no help line can
    close it."""
    longest = max((len(m) for m in re.findall(r"`+", text)), default=0)
    tick = "`" * max(3, longest + 1)
    return f"{tick}text\n{text}{tick}"


def render(help_of) -> str:
    top = help_of([])
    lines = [
        "# `nros` CLI reference",
        "",
        "<!-- type: reference · level: 2 -->",
        "",
        "<!-- GENERATED by scripts/gen-cli-reference.py from the CLI's --help. Do not edit by hand.",
        "     Regenerate: python3 scripts/gen-cli-reference.py (needs this checkout's nros built)",
        "     Gated by:   just check cli-reference -->",
        "",
        "The help of every user-tier `nros` command, exactly as the CLI prints it:",
        "`nros --help`, then each verb in the groups `nros --help` lists, then each",
        "subcommand a verb lists. The page is generated from the CLI's own help, so",
        "it reads the same as `nros <command> --help` on the release it documents.",
        "`nros help --all` adds the build-system tier, which is not part of this page,",
        "and the extension commands your workspace's packages provide.",
        "",
        "## Where each verb is explained",
        "",
        "| Command | Page |",
        "| --- | --- |",
    ]
    lines += [f"| `{c}` | [{t}]({p}) |" for c, t, p in EXPLAINED]
    lines += ["", "## `nros`", "", fence(top), ""]
    seen: set[tuple[str, ...]] = set()

    def emit(path: tuple[str, ...], level: int) -> None:
        if path in seen or len(path) > MAX_DEPTH:
            return
        seen.add(path)
        text = help_of(list(path))
        lines.extend([f"{'#' * level} `nros {' '.join(path)}`", "", fence(text), ""])
        for sub in subcommands(text):
            emit((*path, sub), min(level + 1, 6))

    for heading, verbs in groups(top):
        lines += [f"## {heading}", ""]
        for v in verbs:
            emit((v,), 3)
    return "\n".join(lines).rstrip() + "\n"


def cli_path() -> str | None:
    from source_locate import nros_cli_bin

    return nros_cli_bin(checkout_only=True)


def stale(cli: str) -> str | None:
    r = subprocess.run([cli, "source-stamp"], capture_output=True, text=True, cwd=ROOT)
    return None if r.returncode == 0 else (r.stdout + r.stderr).strip() or f"exit {r.returncode}"


def self_test() -> None:
    top = ("nros — x.\n\nQuick start:\n  nros new <n>   scaffold\n\nUsage: nros [OPTIONS] [COMMAND]\n\n"
           "Start:\n  new    Scaffold\n  setup  Fetch\n\nOther:\n  version  Print\n  help     Print this\n\n"
           "Options:\n  -h, --help  Print help\n")
    assert groups(top) == [("Start", ["new", "setup"]), ("Other", ["version"])], groups(top)
    sub = "Boards\n\nUsage: nros board <COMMAND>\n\nCommands:\n  list  List\n  show  Show\n  help  h\n"
    helps = {(): top, ("new",): "New\n", ("setup",): "Setup\n", ("version",): "Version\n",
             ("board",): sub}
    page = render(lambda a: helps[tuple(a)])
    assert "## Start\n\n### `nros new`\n\n```text\nNew\n```" in page, page
    assert "## Quick start" not in page and "nros help`" not in page, "a non-verb heading or help became a command"
    # A subcommand nests one level under its verb.
    helps[()] = top.replace("  version  Print\n", "  board  Boards\n")
    helps[("board", "list")], helps[("board", "show")] = "L\n", "S\n"
    page = render(lambda a: helps[tuple(a)])
    assert "### `nros board`" in page and "#### `nros board list`" in page, page
    # A help line holding a fence cannot close the block.
    assert fence("a ``` b\n").startswith("````text\n")


def main() -> int:
    self_test()
    if "--self-test" in sys.argv:
        print("gen-cli-reference self-test: OK")
        return 0
    check = "--check" in sys.argv
    out = book_pages.page_file(SEEDED)
    cli = cli_path()
    why = None if cli else "no in-tree nros CLI (`cd packages/cli && cargo build -p nros-cli`, or NROS_CLI)"
    if cli and not why:
        s = stale(cli)
        if s:
            why = f"the in-tree nros CLI is stale against its sources ({s}); rebuild it"
    if why:
        if not check:
            sys.exit(f"gen-cli-reference: {why}")
        import check_skip

        return check_skip.unverified(NAME, f"{why}, so reference/cli is unread against `nros --help`")
    content = book_pages.relink(render(Cli(cli).help), SEEDED)
    if check:
        on_disk = out.read_text(encoding="utf8") if out.exists() else ""
        if on_disk != content:
            sys.exit(f"gen-cli-reference: STALE — {out.relative_to(ROOT)} differs from `nros --help`. "
                     "Regenerate with python3 scripts/gen-cli-reference.py and commit.")
        print(f"cli-reference OK ({content.count('```text')} command(s), matches {cli})")
        return 0
    out.write_text(content, encoding="utf8")
    print(f"wrote {out.relative_to(ROOT)} from {cli}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
