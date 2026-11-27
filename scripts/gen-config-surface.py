#!/usr/bin/env python3
"""Generate the configuration reference: every knob a project can state.

phase-569 W2c (RFC-0133 D2). The page lists every PUBLIC knob of the knob
registry -- core's `packages/tooling/nros-zephyr-build/knobs.toml` and the
in-tree packages' `nros-knobs.toml` -- with the one place a project states it
(`[image.<id>.knobs.<group>] <name>`), the values it takes, its default and
its help. It never prints an environment variable, a Kconfig symbol or a `-D`
define: those carry a decision between build layers and are not where a user
makes it.

## Where the facts come from

The page is rendered from `nros knob list --json`, the CLI that reads the
registry compiled into it, so the page, `nros knob list` and `nros knob show`
say the same thing in the same words (the CLI rewrites a help text's carrier
spellings into knob names, and renders the conditional defaults; doing that a
second time here would be a second implementation of both). The page is
COMMITTED, like the CLI reference, and needs no CLI to read.

## The gate (`--check`)

Three comparisons, each between two readers that could disagree:

  rows      the registry's public in-tree rows, read here from the TOML by
            `scripts/gen-knob-registry.py`'s loader, against the knobs the
            committed page lists. Needs no CLI, so it runs in every lane: a
            row that gains or loses `class = "public"` fails until the page
            is regenerated.
  cli       the page against a fresh render from this checkout's CLI. A
            missing or stale CLI is NOT VERIFIED (`just check cli-fresh` owns
            that), never a quiet pass.
  kconfig   every `config NROS_*` in zephyr/Kconfig is classified, and none is
            dead. A registry knob's Kconfig symbol takes the row's `class`
            (the symbol carries that knob). Any other symbol is classified
            from evidence: a prompt makes it public, a reader with no prompt
            makes it derived, and a symbol nothing reads is dead.

Run:  python3 scripts/gen-config-surface.py [--check | --stdout | --self-test]
Gate: just check config-surface
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import tempfile
import textwrap

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
import git_grep  # noqa: E402  issue 1816 -- a failed search is not "no match"
import book_pages  # noqa: E402  where the page lives is the page map's answer (RFC-0123 D7)

PAGE = "reference/configuration-surface.md"
NAME = "config-surface"
OUT = str(book_pages.page_file(PAGE))
KCONFIG = os.path.join(ROOT, "zephyr", "Kconfig")
CMAKE_FORWARD = os.path.join(ROOT, "zephyr", "cmake", "nros_cargo_build.cmake")
REGISTRY = os.path.join(ROOT, "packages", "tooling", "nros-zephyr-build", "knobs.toml")

# The CLI names the provenance of a package knob: `built in` for an in-tree
# package, a provider-scan root for anything the workspace found. The page is
# the SDK's reference, so it lists core and the built-in packages only.
BUILT_IN = (None, "built in")

# A knob heading on the page. The rows check reads the page back through it.
HEADING = re.compile(r"^### `([a-z0-9_]+\.[a-z0-9_.]+)`$", re.M)


# --------------------------------------------------------------------------
# The page
# --------------------------------------------------------------------------

def owners():
    """Group word -> what the group is, from the registries' `[owners]`."""
    try:
        import tomllib
    except ModuleNotFoundError:  # Python < 3.11 -- the repo's interpreter is 3.10
        import tomli as tomllib

    out = {}
    with open(REGISTRY, "rb") as fh:
        out.update(tomllib.load(fh).get("owners", {}))
    return out


def default_lines(d):
    """The default in words, as `nros knob show` prints it."""
    src = d.get("from")
    if src == "ladder":
        return ["set per platform and board; `nros knob list --platform <p>` shows "
                "the value one platform gets, and which rung set it"]
    if src == "build":
        return ["chosen by the build"]
    choices = d.get("choices") or []
    if len(choices) == 1:
        return [f"`{choices[0]['value']}`"]
    out = ["the first that applies:"]
    for c in choices:
        if c.get("when"):
            out.append(f"  - `{c['value']}` if {c['when']}")
        else:
            out.append(f"  - `{c['value']}` otherwise")
    return out


def default_cell(d):
    src = d.get("from")
    if src == "ladder":
        return "per platform"
    if src == "build":
        return "chosen by the build"
    choices = d.get("choices") or []
    if len(choices) == 1:
        return f"`{choices[0]['value']}`"
    return f"`{choices[-1]['value']}` (varies)" if choices else "-"


def cell(text):
    return text.replace("|", "\\|")


def render(items, groups):
    """The page, from `nros knob list --json` items (built-in ones only)."""
    by_group = {}
    for i in items:
        by_group.setdefault(i["name"].split(".", 1)[0], []).append(i)
    L = [
        "# Configuration surface",
        "",
        "<!-- type: reference · level: 2 -->",
        "",
        "<!-- GENERATED by scripts/gen-config-surface.py from `nros knob list --json`. Do not edit by hand.",
        "     Regenerate: python3 scripts/gen-config-surface.py (needs this checkout's nros built)",
        "     Gated by:   just check config-surface -->",
        "",
        "Every knob a project can state, with the one place to state it, the",
        "values it takes and its default. A knob is set in the image's",
        "`system.toml`, under `[image.<id>.knobs.<group>]`; a board-scoped knob",
        "can also be set in a board's `nros-board.toml`. `nros knob list` prints",
        "this list for your workspace, including the knobs of packages it",
        "depends on, and `nros knob show <knob>` prints one entry.",
        "",
        "A knob has other spellings inside the build, such as an environment",
        "variable or a Kconfig symbol. Those carry the value between build",
        "layers. They are not places to set it, so this page does not list them.",
        "",
        f"{len(items)} knobs in {len(by_group)} groups.",
        "",
    ]
    for g in sorted(by_group):
        rows = sorted(by_group[g], key=lambda i: i["name"])
        L += [f"## `{g}`", ""]
        if groups.get(g):
            L += [f"Knobs for {groups[g]}.", ""]
        L += ["| Knob | Scope | Default | What it is |", "| --- | --- | --- | --- |"]
        for i in rows:
            anchor = i["name"].replace(".", "")
            L.append(f"| [`{i['name']}`](#{anchor}) | {i['scope']} | "
                     f"{cell(default_cell(i['default']))} | {cell(i['summary'])} |")
        L.append("")
        for i in rows:
            L += [f"### `{i['name']}`", "", i["summary"], ""]
            if i.get("surface"):
                L.append(f"- **Set it in:** `system.toml`, `{i['surface']}`")
            if i.get("board_key"):
                L.append(f"- **Or for a board:** `nros-board.toml`, `[knobs] {i['board_key']}`")
            L.append(f"- **Scope:** {i['scope']}")
            if i.get("package"):
                L.append(f"- **Package:** `{i['package']}`")
            L.append(f"- **Values:** {i['values']}")
            dl = default_lines(i["default"])
            L.append(f"- **Default:** {dl[0]}")
            L += dl[1:]
            L.append("")
            if i.get("help"):
                L += [textwrap.dedent(i["help"]).strip(), ""]
    return "\n".join(L).rstrip() + "\n"


# --------------------------------------------------------------------------
# The CLI
# --------------------------------------------------------------------------

def cli_path():
    from source_locate import nros_cli_bin

    return nros_cli_bin(checkout_only=True)


def stale(cli):
    r = subprocess.run([cli, "source-stamp"], capture_output=True, text=True, cwd=ROOT)
    return None if r.returncode == 0 else (r.stdout + r.stderr).strip() or f"exit {r.returncode}"


def cli_items(cli):
    """`nros knob list --json` from an empty workspace, built-in knobs only.

    A scratch directory and a scrubbed environment, so neither the generating
    machine's workspace nor an `NROS_*` variable can reach the page."""
    tmp = tempfile.mkdtemp(prefix="nros-knobs-")
    env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "HOME": tmp,
           "NO_COLOR": "1", "LANG": "C", "LC_ALL": "C"}
    r = subprocess.run([cli, "knob", "list", "--json", "--workspace", tmp], cwd=tmp, env=env,
                       capture_output=True, text=True, timeout=120)
    if r.returncode != 0:
        sys.exit(f"gen-config-surface: `nros knob list --json` exited {r.returncode}:\n"
                 f"{r.stderr.strip()}")
    doc = json.loads(r.stdout)
    if doc.get("schema") != "nros.knob.list.v1":
        sys.exit(f"gen-config-surface: `nros knob list --json` says schema {doc.get('schema')!r}; "
                 "this generator reads nros.knob.list.v1")
    return [i for i in doc["items"] if i.get("root") in BUILT_IN]


# --------------------------------------------------------------------------
# The rows check: the registry's public rows against the committed page
# --------------------------------------------------------------------------

def registry_public_keys():
    """Every public in-tree row's knob name, read from the TOML."""
    import importlib.util

    spec = importlib.util.spec_from_file_location(
        "gen_knob_registry", os.path.join(ROOT, "scripts", "gen-knob-registry.py"))
    reg = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(reg)
    rows, errors = reg.load(packages=None)
    if errors:
        sys.exit("gen-config-surface: the knob registry does not load:\n  " + "\n  ".join(errors[:8]))
    keys = set()
    for r in rows:
        if reg.surface(r):
            keys.add(reg.image_key(r))
    return keys, {reg.kconfig_symbol(r): r.get("class") for r in rows}


def rows_problems(page, public):
    on_page = set(HEADING.findall(page))
    out = []
    for k in sorted(public - on_page):
        out.append(f"`{k}` is a public knob in the registry and is not on the page")
    for k in sorted(on_page - public):
        out.append(f"`{k}` is on the page and is not a public knob in the registry")
    return out


# --------------------------------------------------------------------------
# The Kconfig check
# --------------------------------------------------------------------------

# Symbols whose only consumer is USER code, by design. They have no reader in
# this tree and are not dead: the guides teach an application to use them in its
# own `main()`, and Kconfig generating the macro is the whole service.
PROVIDED = {
    "CONFIG_NROS_INIT_DELAY_MS":
        "`book/src/guide/platforms/zephyr.md` (its C++ applications section) shows an application calling "
        "`nros_platform_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS)` from its own `main()`.",
}


def kconfig_symbols():
    """Every `config NROS_*` in zephyr/Kconfig."""
    out = []
    if not os.path.exists(KCONFIG):
        return out
    for line in open(KCONFIG, encoding="utf-8"):
        m = re.match(r"^\s*(?:menu)?config\s+(NROS_\w+)", line)
        if m:
            out.append("CONFIG_" + m.group(1))
    return out


def forwarded_knobs():
    """Kconfig symbols the cmake bridge hands to cargo (issue 0460's list)."""
    if not os.path.exists(CMAKE_FORWARD):
        return set()
    body = open(CMAKE_FORWARD, encoding="utf-8").read()
    return {"CONFIG_" + m for m in re.findall(r"\$\{CONFIG_(NROS_\w+)\}", body)}


def readers(symbols):
    """Symbols read anywhere outside Kconfig and the docs (one `git grep`)."""
    read = set()
    wanted = set(symbols)
    out = git_grep.run(["-n", "-F", "--", "CONFIG_NROS_"], ROOT)
    for line in out.splitlines():
        try:
            path, _, body = line.split(":", 2)
        except ValueError:
            continue
        if path.endswith("Kconfig") or path.startswith(("docs/", "book/")):
            continue
        read.update(s for s in re.findall(r"CONFIG_NROS_\w+", body) if s in wanted)
    return read


def kconfig_internal_refs():
    """Symbols Kconfig itself consumes (`select`, `imply`, `depends on`,
    `default`), and symbols whose effect is a `select` or `imply`."""
    refs, cur = set(), None
    for line in open(KCONFIG, encoding="utf-8"):
        m = re.match(r"^\s*(?:menu)?config\s+(NROS_\w+)", line)
        if m:
            cur = "CONFIG_" + m.group(1)
            continue
        ref = re.match(r"^\s*(?:select|imply|depends on|default)\s+(.+)$", line)
        if ref:
            for name in re.findall(r"\bNROS_\w+", ref.group(1)):
                refs.add("CONFIG_" + name)
            if cur and re.match(r"^\s*(?:select|imply)\s", line):
                refs.add(cur)
    return refs


def prompted():
    out, cur = set(), None
    for line in open(KCONFIG, encoding="utf-8"):
        m = re.match(r"^\s*(?:menu)?config\s+(NROS_\w+)", line)
        if m:
            cur = "CONFIG_" + m.group(1)
            continue
        if cur and (re.match(r'^\s*(bool|int|string|hex)\s+"', line)
                    or re.match(r'^\s*prompt\s+"', line)):
            out.add(cur)
    return out


def classify(sym, row_class, read, has_prompt):
    """`knob:<class>` for a registry knob's symbol, else from evidence."""
    if row_class:
        return f"knob:{row_class}"
    if sym in PROVIDED:
        return "provided"
    if sym not in read:
        return "dead"
    return "public" if sym in has_prompt else "derived"


def kconfig_classes(row_classes):
    syms = kconfig_symbols()
    read = readers(syms) | forwarded_knobs() | kconfig_internal_refs()
    has_prompt = prompted()
    return {s: classify(s, row_classes.get(s[len("CONFIG_"):]), read, has_prompt) for s in syms}


# --------------------------------------------------------------------------

def self_test():
    item = {"name": "net.max_sockets", "scope": "board", "class": "public",
            "surface": "[image.<id>.knobs.net] max_sockets", "board_key": "net.max_sockets",
            "values": "an integer from 1 to 64", "summary": "Sockets | slots",
            "default": {"from": "stated", "choices": [{"value": "8", "when": "the image uses X"},
                                                      {"value": "4", "when": None}]},
            "help": "  Line one.\n  Line two.\n"}
    page = render([item], {"net": "the network stack"})
    assert "### `net.max_sockets`" in page, page
    assert "`[image.<id>.knobs.net] max_sockets`" in page, page
    assert "  - `8` if the image uses X" in page and "  - `4` otherwise" in page, page
    assert "Sockets \\| slots" in page, "a `|` in a summary must not split the table row"
    assert "Line one.\nLine two." in page, "help is dedented"
    assert HEADING.findall(page) == ["net.max_sockets"]
    assert rows_problems(page, {"net.max_sockets"}) == []
    assert rows_problems(page, {"net.max_sockets", "log.level"}) == [
        "`log.level` is a public knob in the registry and is not on the page"]
    assert rows_problems(page, set()) == [
        "`net.max_sockets` is on the page and is not a public knob in the registry"]
    # A registry knob's symbol takes the row's class, whatever the evidence says.
    assert classify("CONFIG_NROS_X", "carrier", set(), set()) == "knob:carrier"
    assert classify("CONFIG_NROS_Y", None, set(), {"CONFIG_NROS_Y"}) == "dead"
    assert classify("CONFIG_NROS_Y", None, {"CONFIG_NROS_Y"}, {"CONFIG_NROS_Y"}) == "public"
    assert classify("CONFIG_NROS_Y", None, {"CONFIG_NROS_Y"}, set()) == "derived"


def main():
    self_test()
    if "--self-test" in sys.argv:
        print("gen-config-surface self-test: OK")
        return 0
    check = "--check" in sys.argv
    public, row_classes = registry_public_keys()

    if check:
        failures = []
        dead = sorted(s for s, c in kconfig_classes(row_classes).items() if c == "dead")
        for s in dead:
            failures.append(f"{s} is declared in zephyr/Kconfig and read by nothing: delete it, "
                            "or wire it")
        page = open(OUT, encoding="utf-8").read() if os.path.exists(OUT) else ""
        failures += rows_problems(page, public)
        if failures:
            print("gen-config-surface: FAIL\n  " + "\n  ".join(failures) +
                  "\n  Regenerate the page with python3 scripts/gen-config-surface.py and commit it.",
                  file=sys.stderr)
            return 1
        cli = cli_path()
        why = None if cli else "no in-tree nros CLI (`just setup-cli`, or NROS_CLI)"
        if cli and not why:
            s = stale(cli)
            if s:
                why = f"the in-tree nros CLI is stale against its sources ({s}); rebuild it"
        if why:
            import check_skip

            return check_skip.unverified(
                NAME, f"{why}, so {PAGE} is checked against the registry rows only, "
                      "not against `nros knob list`")
        fresh = book_pages.relink(render(cli_items(cli), owners()), PAGE)
        if fresh != page:
            print(f"gen-config-surface: STALE -- book/src/{PAGE} differs from `nros knob list`.\n"
                  "  Regenerate with python3 scripts/gen-config-surface.py and commit it.",
                  file=sys.stderr)
            return 1
        print(f"config-surface OK -- {len(public)} public knobs, the page and `nros knob list` "
              f"agree; {len(row_classes)} registry rows, no dead Kconfig symbol")
        return 0

    cli = cli_path()
    if not cli:
        sys.exit("gen-config-surface: no in-tree nros CLI (`just setup-cli`, or NROS_CLI)")
    s = stale(cli)
    if s:
        sys.exit(f"gen-config-surface: the in-tree nros CLI is stale against its sources ({s}); "
                 "rebuild it with `just setup-cli`")
    body = book_pages.relink(render(cli_items(cli), owners()), PAGE)
    problems = rows_problems(body, public)
    if problems:
        sys.exit("gen-config-surface: `nros knob list` and the registry rows disagree:\n  " +
                 "\n  ".join(problems) +
                 "\n  The CLI reads the registry compiled into it: regenerate the registry "
                 "(python3 scripts/gen-knob-registry.py) and rebuild the CLI.")
    if "--stdout" in sys.argv:
        sys.stdout.write(body)
        return 0
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w", encoding="utf-8") as fh:
        fh.write(body)
    print(f"wrote book/src/{PAGE} -- {len(public)} public knobs")
    return 0


if __name__ == "__main__":
    sys.exit(main())
