#!/usr/bin/env python3
"""The host board is `linux`, and nothing tracked selects it by another word.

WHAT THE WORDS MEAN (CLAUDE.md "Naming", maintainer ruling 2026-10-14,
phase-500):

    native   every general-purpose host (Linux, macOS, Windows)
    posix    POSIX-compliant systems
    linux    Linux specifically

The host board (`packages/boards/linux/nros-board.toml`) builds on Linux ONLY —
measured: `nros-board-linux::apply_tier_affinity` uses `cpu_set_t` /
`sched_setaffinity`, which libc does not define for apple, windows or freebsd.
So its one true name is `linux`:

  * `native` claims every general-purpose host. phase-500 W1 DROPPED it as a
    board name; the consumers refuse it with a hint naming `linux`
    (`nros_entry_lower::RETIRED_BOARD_KEYS`, `cmake/NanoRosHostBoard.cmake`).
  * `posix` is the PLATFORM (`nros-platform-posix`, whose C sources NuttX also
    compiles), never a board name.

HISTORY. Phase-401 split a descriptor that said `["linux", "native", "posix"]`
(both reaches at once); issue 1109 then ruled `native` the ROLE word a host
image should write. The maintainer rejected that ruling: under the definitions
above `native` is a reach claim too, and a false one for this board.

WHAT IS CHECKED (every population is TRACKED files; CLI test fixtures under a
`tests/` dir are included — the stale `["native", "posix"]` copy lived there,
invisible to the old gate's `tests` exclusion):

  1. Every board descriptor: no `names` entry is `native` or `posix`, and no
     descriptor offers both `linux` and `posix`.
  2. The one host board descriptor (`platform = "posix"`, `entry_kind =
     "hosted-main"`, outside `tests/`) names exactly `["linux"]`.
  3. Every `system.toml` (and `*system.toml.case` fixture): no `[image.*]` or
     `[image_defaults]` `board` is a retired host word.
  4. Every `Cargo.toml`: no `[package.metadata.nros.*] deploy = [...]` list
     holds a retired host word.
  5. Every cmake file: no `DEPLOY` keyword argument is a retired host word.
  6. `nros-sdk-index.toml`: no `[board.native]` / `[board.posix]`, and a
     `[board.linux]` exists (`nros setup linux` once failed for lack of it).

WHAT IT DELIBERATELY DOES NOT CHECK

Prose, and the deprecated CLI ALIASES (`nros new --platform native|posix`,
`nros setup native`), which exist for released book pages and are retired by
phase-500 W5. Image IDS (`[image.native]`) are labels, renamed by W3. The
`native` lane / just module / scope are W2.

Usage:  check-host-platform-vocabulary.py
"""

import os
import re
import subprocess
import sys

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
import file_kinds  # noqa: E402  phase-472 W5 — populations by kind

HOST = "linux"
# Words that must never select the host board. `native` is retired (phase-500
# W1); `posix` is the platform. Each maps to the word to write instead.
NOT_A_HOST_BOARD = {"native": HOST, "posix": HOST}

NAMES_RE = re.compile(r"^\s*names\s*=\s*\[(.*?)\]", re.M | re.S)
DEPLOY_RE = re.compile(r"^\s*DEPLOY\s+([^)\n#]*)", re.M)


def tracked(*patterns):
    out = subprocess.run(
        ["git", "-C", ROOT, "ls-files", "-z", "--", *patterns],
        check=True, capture_output=True).stdout.decode()
    return sorted(p for p in out.split("\0") if p and "/third-party/" not in f"/{p}")


def strip_comments(text, mark="#"):
    return "\n".join(l for l in text.split("\n") if not l.lstrip().startswith(mark))


def descriptor_problems(names):
    """What is wrong with one descriptor's `names` list."""
    out = [f"`{n}` is not a board name (write `{NOT_A_HOST_BOARD[n]}`)"
           for n in names if n in NOT_A_HOST_BOARD]
    if HOST in names and "posix" in names:
        out.append("claims both `linux` and `posix`")
    return out


def descriptors():
    # Every descriptor, CLI test fixtures included (phase-500 W1: the stale
    # copy lived under `tests/`).
    return file_kinds.files_of_kind("board-descriptor", repo=ROOT)


def check_descriptors():
    problems, hosts = [], []
    for rel in descriptors():
        with open(os.path.join(ROOT, rel), encoding="utf8") as fh:
            text = fh.read()
        for m in NAMES_RE.finditer(strip_comments(text)):
            names = re.findall(r'"([^"]*)"', m.group(1))
            problems += [(rel, p) for p in descriptor_problems(names)]
        if "/tests/" in f"/{rel}":
            continue
        doc = tomllib.loads(text)
        for b in doc.get("board", []):
            if b.get("platform") == "posix" and b.get("entry_kind") == "hosted-main":
                hosts.append((rel, b.get("names", [])))
    if len(hosts) != 1:
        problems.append(("(catalog)", f"expected ONE host board descriptor, found {hosts}"))
    elif hosts[0][1] != [HOST]:
        problems.append((hosts[0][0], f"host board names {hosts[0][1]}, must be [{HOST!r}]"))
    return problems


def selector_problems(doc):
    """(site, word) for every host selector in a parsed system.toml that writes
    a word that is not the host board's name."""
    sites = []
    d = doc.get("image_defaults")
    if isinstance(d, dict) and "board" in d:
        sites.append(("[image_defaults]", d["board"]))
    for iid, body in (doc.get("image") or {}).items():
        if isinstance(body, dict) and "board" in body:
            sites.append((f"[image.{iid}]", body["board"]))
    return [(w, b) for w, b in sites if b in NOT_A_HOST_BOARD]


def deploy_problems(doc):
    """(table, word) for every `[package.metadata.nros.*] deploy` entry that is
    a retired host word."""
    nros = ((doc.get("package") or {}).get("metadata") or {}).get("nros") or {}
    out = []
    for key, body in nros.items():
        if isinstance(body, dict) and isinstance(body.get("deploy"), list):
            out += [(f"[package.metadata.nros.{key}] deploy", w)
                    for w in body["deploy"] if w in NOT_A_HOST_BOARD]
    return out


def cmake_problems(text):
    out = []
    for m in DEPLOY_RE.finditer(strip_comments(text)):
        out += [("DEPLOY", w) for w in m.group(1).split() if w.strip('"') in NOT_A_HOST_BOARD]
    return out


def sdk_index_problems(doc):
    boards = doc.get("board") or {}
    out = [f"[board.{k}] (write [board.{v}])" for k, v in NOT_A_HOST_BOARD.items() if k in boards]
    if HOST not in boards:
        out.append(f"no [board.{HOST}] — `nros setup {HOST}` cannot resolve")
    return out


def check_files():
    problems = []
    for rel in file_kinds.files_of_kind("system-toml", repo=ROOT) + tracked("*system.toml.case"):
        with open(os.path.join(ROOT, rel), "rb") as fh:
            try:
                doc = tomllib.load(fh)
            except tomllib.TOMLDecodeError as e:
                raise SystemExit(f"check-host-platform-vocabulary: {rel}: {e}")
        problems += [(rel, f"{w} board = {b!r}") for w, b in selector_problems(doc)]
    for rel in tracked("Cargo.toml", "*/Cargo.toml"):
        with open(os.path.join(ROOT, rel), "rb") as fh:
            try:
                doc = tomllib.load(fh)
            except tomllib.TOMLDecodeError:
                continue  # not ours to judge; cargo will
        problems += [(rel, f"{w} holds {b!r}") for w, b in deploy_problems(doc)]
    for rel in tracked("*.cmake", "CMakeLists.txt", "*/CMakeLists.txt"):
        with open(os.path.join(ROOT, rel), encoding="utf8", errors="replace") as fh:
            problems += [(rel, f"{k} {w}") for k, w in cmake_problems(fh.read())]
    with open(os.path.join(ROOT, "nros-sdk-index.toml"), "rb") as fh:
        problems += [("nros-sdk-index.toml", p) for p in sdk_index_problems(tomllib.load(fh))]
    return problems


def self_test():
    """Prove each check can fail. Runs on EVERY invocation — a negative control
    nobody runs decays into a comment (`check-gate-selftests`)."""
    fails = []
    want = {
        ("linux",): [],
        ("native", "linux"): ["`native` is not a board name (write `linux`)"],
        ("native", "posix"): ["`native` is not a board name (write `linux`)",
                              "`posix` is not a board name (write `linux`)"],
        ("linux", "posix"): ["`posix` is not a board name (write `linux`)",
                             "claims both `linux` and `posix`"],
        ("threadx", "threadx-linux"): [],
    }
    for names, exp in want.items():
        got = descriptor_problems(list(names))
        if got != exp:
            fails.append(f"descriptor {names}: expected {exp}, got {got}")
    for body, exp in [
        ('[image.n]\nboard = "linux"\n', []),
        ('[image.native]\nboard = "linux"\n', []),  # an image ID is a label (W3)
        ('[image.n]\nboard = "native"\n', [("[image.n]", "native")]),
        ('[image_defaults]\nboard = "posix"\n', [("[image_defaults]", "posix")]),
        ('[image.z]\nboard = "zephyr"\n', []),
    ]:
        got = selector_problems(tomllib.loads(body))
        if got != exp:
            fails.append(f"selector {body!r}: expected {exp}, got {got}")
    for body, exp in [
        ('[package.metadata.nros.application]\ndeploy = ["linux"]\n', []),
        ('[package.metadata.nros.application]\ndeploy = ["native", "freertos"]\n',
         [("[package.metadata.nros.application] deploy", "native")]),
    ]:
        got = deploy_problems(tomllib.loads(body))
        if got != exp:
            fails.append(f"deploy {body!r}: expected {exp}, got {got}")
    for body, exp in [
        ("nano_ros_entry(NAME a\n    DEPLOY  linux)\n", []),
        ("nano_ros_entry(NAME a\n    DEPLOY  native freertos)\n", [("DEPLOY", "native")]),
        ("# DEPLOY native — a comment\n", []),
    ]:
        got = cmake_problems(body)
        if got != exp:
            fails.append(f"cmake {body!r}: expected {exp}, got {got}")
    for body, exp in [
        ("[board.linux]\n", []),
        ("[board.native]\n[board.posix]\n", ["[board.native] (write [board.linux])",
                                            "[board.posix] (write [board.linux])",
                                            "no [board.linux] — `nros setup linux` cannot resolve"]),
    ]:
        got = sdk_index_problems(tomllib.loads(body))
        if got != exp:
            fails.append(f"sdk index {body!r}: expected {exp}, got {got}")
    if "packages/cli/nros-cli-core/tests/fixtures/board-workspace/packages/boards/linux/nros-board.toml" \
            not in descriptors():
        fails.append("the CLI test-fixture host descriptor is not in the population")
    if fails:
        for f in fails:
            print(f"check-host-platform-vocabulary self-test: FAIL {f}", file=sys.stderr)
        raise SystemExit(1)


def main():
    problems = check_descriptors() + check_files()
    if problems:
        print("check-host-platform-vocabulary: the host board is selected by a word "
              "other than `linux` (phase-500 W1):\n", file=sys.stderr)
        for rel, what in problems:
            print(f"  {rel}: {what}", file=sys.stderr)
        print(
            "\n  native = every general-purpose host (Linux, macOS, Windows)\n"
            "  posix  = POSIX-compliant systems (the PLATFORM's name)\n"
            "  linux  = Linux — the host board, which builds on Linux only\n"
            "  See CLAUDE.md “Naming” and docs/roadmap/phase-500-linux-host-name.md.",
            file=sys.stderr,
        )
        return 1
    print("check-host-platform-vocabulary OK — the host board is `linux` in every "
          "descriptor, selector, deploy list, cmake DEPLOY and sdk-index key.")
    return 0


if __name__ == "__main__":
    self_test()
    sys.exit(main())
