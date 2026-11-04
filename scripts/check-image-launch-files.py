#!/usr/bin/env python3
"""Phase-535 W1 (RFC-0122 D8) — every image's launch file exists.

An `[image.<id>] launch = "<file>"` (or the bringup's `[system]
default_launch`) names a file in the bringup's `launch/` directory. A typo, or
a launch file deleted while an image still names it, surfaces today only when
`nros configure` resolves that bringup, which a lane that does not build the image
never does. Phase-535 adds one image row per (platform, role, rmw) to the
language workspaces, so this check moves to the fast line, where it costs
nothing.

A `<pkg>:<file>` value names another package's launch file and is checked in
that package. An image with no launch file at all has one GENERATED
(RFC-0121 D3), so there is nothing to check. Reads only tracked files.
"""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from tracked import tracked  # noqa: E402  issue 0721: index lookup, not a walk

try:
    import tomllib  # Python 3.11+
except ModuleNotFoundError:  # pragma: no cover
    import tomli as tomllib

REPO = Path(__file__).resolve().parents[1]


def problems(root: Path, systems: list) -> list:
    out = []
    for rel in systems:
        path = root / rel
        try:
            doc = tomllib.loads(path.read_text(encoding="utf-8"))
        except (OSError, tomllib.TOMLDecodeError):
            continue  # a malformed file is another gate's subject
        images = doc.get("image") or {}
        if not isinstance(images, dict):
            continue
        default = (doc.get("system") or {}).get("default_launch")
        bringup = path.parent
        for image_id, image in images.items():
            if not isinstance(image, dict):
                continue
            launch = image.get("launch", default)
            if not launch:
                continue
            pkg, _, file = launch.rpartition(":")
            base = bringup.parent / pkg if pkg else bringup
            if not (base / "launch" / file).is_file():
                out.append(f"{rel}: [image.{image_id}] launch = \"{launch}\" -> "
                           f"{(base / 'launch' / file).relative_to(root)} does not exist")
    return out


def self_test() -> int:
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        b = root / "ws/src/pubsub_bringup"
        (b / "launch").mkdir(parents=True)
        (b / "launch/talker.launch.xml").write_text("<launch/>\n")
        (b / "system.toml").write_text(
            '[image.ok]\nboard = "x"\nlaunch = "talker.launch.xml"\n'
            '[image.gen]\nboard = "x"\n'
            '[image.bad]\nboard = "x"\nlaunch = "listener.launch.xml"\n')
        got = problems(root, ["ws/src/pubsub_bringup/system.toml"])
        if len(got) != 1 or "[image.bad]" not in got[0]:
            print(f"check-image-launch-files self-test: FAIL {got}")
            return 1
    return 0


def main() -> int:
    if self_test():
        return 1
    systems = [str(p.relative_to(REPO)) for p in tracked(REPO, name="system.toml", repo=REPO)]
    bad = problems(REPO, systems)
    if bad:
        print("check-image-launch-files: FAIL — an image names a launch file that is not there:")
        for b in bad:
            print("    " + b)
        return 1
    print(f"check-image-launch-files: OK ({len(systems)} system.toml read)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
