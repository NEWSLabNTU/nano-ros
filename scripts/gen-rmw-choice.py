#!/usr/bin/env python3
"""Render the in-tree members of the RMW choices from the backend descriptors
(phase-581 W8, RFC-0140 D6 amending RFC-0071 D18).

    python3 scripts/gen-rmw-choice.py           # write
    python3 scripts/gen-rmw-choice.py --check   # fail if a committed block differs

Input:  packages/rmw/*/*/nros-rmw.toml (each in-tree backend's descriptor) and
        the sibling package.xml (its canonical name, the first
        `<nano_ros_provides kind="rmw" name=.../>`)
Output: zephyr/Kconfig               the members of `choice NROS_RMW_BACKEND`,
                                     between its `rmw choice members` markers
        integrations/nuttx/Kconfig   the members of the NuttX RMW choice, the same

A backend is a member of the ZEPHYR choice when its `[rmw.zephyr]
module_backend` names itself: the nano-ros module builds it (RFC-0071 D18's
in-tree rule). That is the same rule `RmwIndex::zephyr_module_backends` derives
the members from, and `leaf_kconfig`'s unit test
`the_derived_members_are_the_rendered_choice` holds the two equal. A backend
that is its own Zephyr module (`[rmw.zephyr] module`) is out-of-tree by
construction and joins the choice through `<build>/nros/Kconfig.packages`, not
here. A backend is a member of the NUTTX choice when it has a
`[rmw.nuttx.kconfig]` table (an out-of-tree NuttX RMW is not designed, RFC-0140
D6).

The member's definition is the descriptor's `[rmw.<os>.kconfig]` table:

    prompt = "zenoh-pico"            the bool's prompt
    attrs  = ["select X if Y", ...]  attribute lines, verbatim (Zephyr only)
    help   = \"\"\"...\"\"\"              help text, one paragraph per blank line

The reasons for each attribute stay in the descriptor as TOML comments; the
rendering points there. The symbol is `NROS_RMW_<TOKEN>`, the canonical name
upper-cased with every non-alphanumeric as `_` (the `c_define_token`
convention). Members are rendered in canonical-name order; the choice's
header (`prompt`, `default`) stays hand-written outside the markers, and the
check refuses a `default` naming no member.
"""
import re
import sys
from pathlib import Path

try:
    import tomllib  # 3.11+
except ModuleNotFoundError:  # pragma: no cover
    import tomli as tomllib

REPO = Path(__file__).resolve().parents[1]
RMW_ROOT = REPO / "packages" / "rmw"
BEGIN = "# >>> rmw choice members >>>"
END = "# <<< rmw choice members <<<"
TARGETS = {
    "zephyr": REPO / "zephyr" / "Kconfig",
    "nuttx": REPO / "integrations" / "nuttx" / "Kconfig",
}
PROVIDES_RE = re.compile(
    r'<nano_ros_provides\s+kind="rmw"\s+name="([^"]+)"\s*/>')
COMMENT_RE = re.compile(r"<!--.*?-->", re.S)


def token(name):
    return "".join(c.upper() if c.isascii() and c.isalnum() else "_" for c in name)


def canonical(pkg_dir):
    xml = pkg_dir / "package.xml"
    if not xml.is_file():
        return None
    names = PROVIDES_RE.findall(COMMENT_RE.sub("", xml.read_text(encoding="utf-8")))
    return names[0] if names else None


def descriptors(root=RMW_ROOT):
    """[(canonical name, descriptor path relative to the repo, parsed TOML)]."""
    out = []
    for path in sorted(root.glob("*/*/nros-rmw.toml")):
        name = canonical(path.parent)
        if name is None:
            continue
        with path.open("rb") as f:
            out.append((name, path, tomllib.load(f)))
    return out


def rel(p):
    try:
        return str(Path(p).relative_to(REPO))
    except ValueError:
        return str(p)


def members(descs, os_name):
    """[(name, path, kconfig table)] of the `os_name` choice's in-tree members,
    sorted by name; problems as sentences."""
    out, problems = [], []
    for name, path, doc in descs:
        rmw = doc.get("rmw", {})
        table = rmw.get(os_name, {})
        kcfg = table.get("kconfig")
        if os_name == "zephyr":
            member = table.get("module_backend") == name
        else:
            member = kcfg is not None
        if member and kcfg is None:
            problems.append(
                f"{rel(path)}: `{name}` is a member of the Zephyr RMW choice "
                "([rmw.zephyr] module_backend names itself) but has no "
                "[rmw.zephyr.kconfig] table to render it from (prompt, attrs, help)")
            continue
        if not member and kcfg is not None:
            problems.append(
                f"{rel(path)}: [rmw.{os_name}.kconfig] on `{name}`, which is not a "
                f"member of the {os_name} RMW choice")
            continue
        if not member:
            continue
        if not isinstance(kcfg.get("prompt"), str) or not kcfg["prompt"]:
            problems.append(f"{rel(path)}: [rmw.{os_name}.kconfig] has no `prompt`")
            continue
        if os_name == "nuttx" and kcfg.get("attrs"):
            problems.append(f"{rel(path)}: [rmw.nuttx.kconfig] takes no `attrs` "
                            "(the NuttX choice holds menus and choices only)")
            continue
        out.append((name, path, kcfg))
    out.sort(key=lambda m: m[0])
    return out, problems


def render(ms, os_name):
    """The marked block, markers included, no trailing newline."""
    lines = [
        BEGIN,
        "# GENERATED by scripts/gen-rmw-choice.py from each in-tree backend's",
        f"# nros-rmw.toml `[rmw.{os_name}.kconfig]` -- do not edit here: edit the",
        "# descriptor (the reasons for each attribute are there) and re-run the",
        "# script. `just check rmw-choice` holds this block equal to the render",
        "# (phase-581 W8, RFC-0140 D6).",
    ]
    if os_name == "zephyr":
        lines += [
            "#",
            "# A backend that is its own Zephyr module (`[rmw.zephyr] module`) joins this",
            "# choice from <build>/nros/Kconfig.packages, which the module hook writes.",
        ]
    lines.append("")
    for name, path, k in ms:
        lines.append(f"# from {rel(path)}")
        lines.append(f"config NROS_RMW_{token(name)}")
        lines.append(f'    bool "{k["prompt"]}"')
        lines += ["    " + a for a in k.get("attrs", [])]
        help_text = (k.get("help") or "").strip("\n")
        if help_text:
            lines.append("    help")
            lines += [("      " + h) if h.strip() else "" for h in help_text.split("\n")]
        lines.append("")
    lines.append(END)
    return "\n".join(lines)


def span(text):
    b = [m.start() for m in re.finditer(r"(?m)^" + re.escape(BEGIN) + r"$", text)]
    e = [m.end() for m in re.finditer(r"(?m)^" + re.escape(END) + r"$", text)]
    if len(b) != 1 or len(e) != 1 or e[0] < b[0]:
        return None
    return b[0], e[0]


def problems_of(text, ms, os_name, where):
    sp = span(text)
    if sp is None:
        return [f"{where}: no single `{BEGIN}` ... `{END}` block"]
    out = []
    if text[sp[0]:sp[1]] != render(ms, os_name):
        out.append(f"{where}: the RMW choice members differ from what the descriptors "
                   "render. Edit the backend's nros-rmw.toml (not the block) and run "
                   "`python3 scripts/gen-rmw-choice.py`.")
    outside = text[:sp[0]] + text[sp[1]:]
    syms = {f"NROS_RMW_{token(n)}" for n, _, _ in ms}
    for sym in sorted(syms):
        if re.search(r"(?m)^\s*config\s+" + re.escape(sym) + r"\s*$", outside):
            out.append(f"{where}: `config {sym}` is also defined outside the generated "
                       "block; a member has one definition, rendered from its descriptor")
    m = re.search(r"(?m)^\s*default\s+(NROS_RMW_[A-Z0-9_]+)\s*$", text[:sp[0]])
    if m and m.group(1) not in syms:
        out.append(f"{where}: the choice's `default {m.group(1)}` names no member")
    return out


def self_test():
    """Negative controls: an edited block, a stray definition, a missing
    marker, a member with no table, a default naming no member."""
    ms = [("zz", REPO / "x/nros-rmw.toml", {"prompt": "Z", "attrs": ["select A"],
                                            "help": "line\n\npara"})]
    good = "choice X\n    default NROS_RMW_ZZ\n\n" + render(ms, "zephyr") + "\n\nendchoice\n"
    assert problems_of(good, ms, "zephyr", "t") == [], problems_of(good, ms, "zephyr", "t")
    caught = 0
    for bad in (good.replace('bool "Z"', 'bool "Y"'),
                good + "config NROS_RMW_ZZ\n    bool\n",
                good.replace(END, ""),
                good.replace("default NROS_RMW_ZZ", "default NROS_RMW_NOPE")):
        caught += bool(problems_of(bad, ms, "zephyr", "t"))
    _, p = members([("zz", REPO / "x/nros-rmw.toml",
                     {"rmw": {"zephyr": {"module_backend": "zz"}}})], "zephyr")
    caught += bool(p)
    assert caught == 5, f"rmw-choice self-test: {caught} of 5 negative controls caught"
    return caught


def main():
    caught = self_test()
    descs = descriptors()
    rendered, problems = {}, []
    for os_name, target in TARGETS.items():
        ms, p = members(descs, os_name)
        problems += p
        rendered[os_name] = ms
    if problems:
        for msg in problems:
            sys.stderr.write(f"rmw-choice: FAILED -- {msg}\n")
        return 1
    if "--check" in sys.argv:
        for os_name, target in TARGETS.items():
            problems += problems_of(target.read_text(), rendered[os_name], os_name, rel(target))
        for msg in problems:
            sys.stderr.write(f"rmw-choice: FAILED -- {msg}\n")
        if problems:
            return 1
        counts = ", ".join(f"{o} {len(m)}" for o, m in rendered.items())
        print(f"rmw-choice: OK -- in-tree RMW choice members rendered from the descriptors "
              f"({counts}), none defined outside the block "
              f"(selftest: {caught} negative controls caught)")
        return 0
    for os_name, target in TARGETS.items():
        text = target.read_text()
        sp = span(text)
        if sp is None:
            sys.stderr.write(f"rmw-choice: FAILED -- {rel(target)} has no single "
                             f"`{BEGIN}` ... `{END}` block to render into\n")
            return 1
        target.write_text(text[:sp[0]] + render(rendered[os_name], os_name) + text[sp[1]:])
        print(f"wrote {rel(target)} ({len(rendered[os_name])} members)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
