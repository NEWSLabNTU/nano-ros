#!/usr/bin/env python3
"""issue 0606 — every `[deploy.*].board` in the tree resolves to ONE descriptor.

`[deploy.<name>].board` carries the DOWNSTREAM ecosystem's board id: Zephyr's
`native_sim/native/64`, PlatformIO's `esp32dev`, NuttX's `qemu-armv7a-nsh`. A
descriptor's `names` is what nano-ros calls the board. Most values are in both,
which is why the gap stayed invisible: `BoardCatalog::resolve_deploy` matched
`names` only, so the values that were NOT there resolved to nothing and
`nros sync` skipped those leaves — reporting a COUNT at the end, never a name.

Three consumers had each grown their own directory fallback before this was
filed. The rule now lives in one place (`resolve_deploy`: names, then the
directory alias, then the platform) and the descriptors carry the downstream
ids they cover. This gate keeps that true: a new deploy value that no
descriptor claims fails HERE, naming it, instead of becoming a silent skip
three layers down.

Buildless: reads the descriptors and every `system.toml` / entry `Cargo.toml`.
"""

import os
import sys
import sys as _sys
from pathlib import Path as _Path
_sys.path.insert(0, str(_Path(__file__).resolve().parent / "lib"))
from tracked import tracked  # issue 0721: index lookup, not a walk


try:
    import tomllib
except ModuleNotFoundError:  # 3.10 backport, as the sibling gates spell it
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def descriptor_paths():
    """Every descriptor under `packages/boards/`, at ANY depth.

    Issue 1517 — this globbed `packages/boards/*/nros-board.toml`, i.e. the
    IMMEDIATE subdirectories, while the authority it speaks for
    (`BoardCatalog::collect_board_dirs`) descends until it finds a directory
    carrying an `nros-board.toml`. So a nested descriptor was invisible here
    and resolvable everywhere else, and this gate FAILED any image row naming
    one — `fvp-aemv8r-smp` lives at
    `packages/boards/nros-board-zephyr/boards/fvp-aemv8r-smp/`, so writing the
    FVP image's board CORRECTLY turned the fast line red, which is part of why
    the wrong value survived. 0196's shape: a reach narrower than the rule.

    Index lookup rather than a recursive glob, for issue 0721's reason —
    `packages/` holds build output, and `packages/boards/**` would descend
    every `target/` on the way to the 14 tracked descriptors.
    """
    return sorted(
        p
        for p in tracked("packages/boards", name="nros-board.toml")
        # A `nros-board.toml` inside a board's own build output is not a
        # descriptor; the index cannot hold one, so this is belt-and-braces
        # for a staging copy someone checked in.
        if "target" not in p.parts and "build" not in p.parts
    )


def framework_id(entry):
    """The framework's own id for a `[[board]]` entry, or None.

    Mirrors `BoardDescriptor::framework_board`: the board-agnostic
    `west_board`, else `[board.zephyr] west_board`. (A `[[board]]` entry keeps
    its sub-tables under itself — `[board.zephyr]` parses as `entry["zephyr"]`.)
    """
    if entry.get("west_board"):
        return entry["west_board"]
    zephyr = entry.get("zephyr")
    if isinstance(zephyr, dict) and zephyr.get("west_board"):
        return zephyr["west_board"]
    return None


def descriptors():
    """`(alias -> {directory}, framework id -> nano-ros id to write instead)`.

    An alias is a `names` entry, the directory itself, or the descriptor's
    framework id — what `BoardDescriptor::answers_to` + `directory_alias`
    resolve. The directory alias mirrors `directory_alias`: the containing
    directory's name with any `nros-board-` prefix stripped. A nested
    descriptor's directory carries no prefix (`boards/fvp-aemv8r-smp/`), so it
    is the name itself — which is what makes it addressable at all.

    Issue 1519 — the framework id was missing here, so this gate was NARROWER
    than `answers_to` (0196's shape): a `[deploy.*].board` naming Zephyr's id
    resolved for `nros` and failed here, which is half of why that id had been
    smuggled into `names`. The second map is the other half: for each
    framework id, the nano-ros name an IMAGE must write instead (`None` when
    the descriptor has no other name, so there is nothing better to offer —
    the same exemption `image::refuse_framework_board` makes).
    """
    out, framework = {}, {}
    for path in descriptor_paths():
        with open(path, "rb") as fh:
            doc = tomllib.load(fh)
        dir_name = os.path.basename(os.path.dirname(path))
        alias = dir_name[len("nros-board-"):] if dir_name.startswith("nros-board-") else dir_name
        for entry in doc.get("board", []):
            names = list(entry.get("names", []))
            fw = framework_id(entry)
            # `answers_to` derives only the ZEPHYR id; the board-agnostic
            # `west_board` reaches `-b` and is never a lookup key.
            zephyr = entry.get("zephyr")
            answers = names + [alias]
            if isinstance(zephyr, dict) and zephyr.get("west_board"):
                answers.append(zephyr["west_board"])
            for name in answers:
                out.setdefault(name, set()).add(alias)
            if fw:
                framework[fw] = next((n for n in names if n != fw), None)
    return out, framework


def deploy_values():
    """`board value -> [where it is declared]`, both site homes."""
    out = {}
    # issue 0721 / 0726 — index, not walk; same reason as the Cargo.toml scan
    # below. This one is the site the WIDENED gate caught after the other was
    # converted, which is the argument for widening it: the file had two
    # recursive globs and fixing the one I had measured would have left the
    # other paying the same cold-walk cost.
    for path in tracked("examples", "packages", name="system.toml"):
        if True:
            try:
                with open(path, "rb") as fh:
                    doc = tomllib.load(fh)
            except Exception:
                continue
            # `[image.*]` as well as `[deploy.*]` (issue 0951). Images are the
            # buildable unit, so most authored board strings live there now —
            # two in this tree (`nuttx-riscv`, `s32z270-freertos`) are named
            # ONLY on an image, and were therefore never verified by the gate
            # whose whole promise is that an unresolvable board fails HERE,
            # named, instead of becoming a silent skip three layers down.
            # `[image_defaults]` counts too: a board declared there is inherited
            # by every image that omits one.
            for table in ("deploy", "image"):
                for name, blk in (doc.get(table) or {}).items():
                    if isinstance(blk, dict) and blk.get("board"):
                        out.setdefault(blk["board"], []).append(
                            f"{os.path.relpath(path, ROOT)} [{table}.{name}]"
                        )
            defaults = doc.get("image_defaults")
            if isinstance(defaults, dict) and defaults.get("board"):
                out.setdefault(defaults["board"], []).append(
                    f"{os.path.relpath(path, ROOT)} [image_defaults]"
                )
    # Manifests carry no board any more. The standalone-leaf arm that read
    # `[package.metadata.nros.entry] deploy` / `[package.metadata.nros.deploy.*]`
    # here went with those keys (phase-445 W5, RFC-0098 D5): every board is an
    # `[image.*]` board in a `system.toml` — a leaf's own or a bringup's — which
    # the loop above reads, and `check-leaf-deployment-spelling` refuses the
    # retired keys wherever they reappear.
    return out


def classify(known, framework, values):
    """`(unknown, ambiguous, framework_on_image)` — the whole rule, pure.

    `values` maps a board string to its `"<file> [<table>.<name>]"` sites.
    The image rule (issue 1519) reads the TABLE from that label, so a
    `[deploy.*]` naming a framework id passes while an `[image.*]` or
    `[image_defaults]` naming the same string does not — the same split
    `image::refuse_framework_board` and `tier_resolver` make.
    """
    unknown, ambiguous, on_image = [], [], []
    for value, wheres in sorted(values.items()):
        dirs = known.get(value)
        if not dirs:
            unknown.append((value, wheres))
            continue
        if len(dirs) > 1:
            ambiguous.append((value, sorted(dirs), wheres))
            continue
        instead = framework.get(value)
        if instead:
            images = [w for w in wheres if "[image." in w or "[image_defaults]" in w]
            if images:
                on_image.append((value, instead, images))
    return unknown, ambiguous, on_image


def selftest():
    """The gate's own red, on every run (`check-gate-selftests`).

    A synthetic catalog with the shape the `zephyr` descriptor has since issue
    1519 — one name plus a `[board.zephyr] west_board` — and one case of each
    verdict, plus the two that must PASS (a deploy naming the framework id, and
    a framework id that is its descriptor's only name). If any comes back
    wrong, the rule is not being applied and the OK below would mean nothing.
    """
    known = {
        "zephyr": {"zephyr"},
        "native_sim/native/64": {"zephyr"},
        "threadx": {"threadx-linux", "threadx-qemu-riscv64"},
        "lonely_fw": {"lonely"},
    }
    framework = {"native_sim/native/64": "zephyr", "lonely_fw": None}
    values = {
        "zephyr": ["a/system.toml [image.zephyr]"],
        "native_sim/native/64": [
            "a/system.toml [deploy.robot]",
            "b/system.toml [image.zephyr]",
            "c/system.toml [image_defaults]",
        ],
        "lonely_fw": ["d/system.toml [image.x]"],
        "threadx": ["e/system.toml [image.t]"],
        "nonesuch": ["f/system.toml [image.n]"],
    }
    unknown, ambiguous, on_image = classify(known, framework, values)
    problems = []
    if [v for v, _ in unknown] != ["nonesuch"]:
        problems.append(f"unknown: {unknown}")
    if [v for v, _, _ in ambiguous] != ["threadx"]:
        problems.append(f"ambiguous: {ambiguous}")
    want = [(
        "native_sim/native/64",
        "zephyr",
        ["b/system.toml [image.zephyr]", "c/system.toml [image_defaults]"],
    )]
    if on_image != want:
        problems.append(f"framework id on an image (a deploy must pass): {on_image}")
    if problems:
        sys.exit(
            "check-deploy-board-resolves: SELFTEST FAILED — the rule no longer "
            "catches what it exists for:\n  " + "\n  ".join(problems)
        )


def main():
    selftest()
    known, framework = descriptors()
    values = deploy_values()
    if not values:
        sys.exit(
            "check-deploy-board-resolves: found no [deploy.*] / [image.*] board "
            "values — wrong root?"
        )

    unknown, ambiguous, on_image = classify(known, framework, values)

    if unknown or ambiguous or on_image:
        sys.stderr.write("check-deploy-board-resolves: FAILED\n")
        for value, wheres in unknown:
            sys.stderr.write(f"  `{value}` — no descriptor claims it\n")
            for w in wheres[:3]:
                sys.stderr.write(f"      {w}\n")
        for value, dirs, wheres in ambiguous:
            sys.stderr.write(f"  `{value}` — claimed by {len(dirs)}: {', '.join(dirs)}\n")
            for w in wheres[:2]:
                sys.stderr.write(f"      {w}\n")
        for value, instead, wheres in on_image:
            sys.stderr.write(
                f"  `{value}` — the FRAMEWORK's board id, on an image; write "
                f'`board = "{instead}"`\n'
            )
            for w in wheres:
                sys.stderr.write(f"      {w}\n")
        sys.stderr.write(
            "\n  The two tables name a board differently, on purpose:\n"
            "  * `[image.*].board` (and `[image_defaults]`) is a NANO-ROS board id, a\n"
            "    descriptor `names` entry such as `zephyr` — never the framework's own\n"
            "    string. The descriptor states that (`[board.zephyr] west_board`) and\n"
            "    `nros build` passes it to `west build -b`; an image naming it is\n"
            "    refused (`ImageBlock::board`, issue 1519).\n"
            "  * `[deploy.*].board` may name the DOWNSTREAM ecosystem's board (Zephyr's\n"
            "    `native_sim/native/64`, PlatformIO's `esp32dev`, NuttX's\n"
            "    `qemu-armv7a-nsh`). It must still resolve to ONE descriptor — a `names`\n"
            "    entry, the directory, or the descriptor's `[board.zephyr] west_board` —\n"
            "    or `nros sync` skips the leaf with a count rather than a name (0606).\n"
        )
        return 1

    print(
        f"deploy boards resolve: OK ({len(values)} distinct value(s), "
        f"{len(known)} descriptor alias(es); no image names a framework board id)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
