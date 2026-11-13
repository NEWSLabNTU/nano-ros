"""Where cargo put a build script's `OUT_DIR` — the ONE reader of cargo's JSON.

RFC-0103 D8 / phase-484 W6. A build script's products live in `$OUT_DIR`,
which cargo hashes per unit and reports on its stable JSON stream:

    {"reason":"build-script-executed", "package_id": …, "out_dir": …}

even for a FRESH unit (cargo replays the recorded output). Non-cargo consumers
— cmake configures, `just` recipes — need that path, and three separate
parsers of the stream existed: `scripts/build/cargo-out-dir-headers.py`, the
`xrce-cffi-out-dir.py` helper (which matched the package with a bare
`<name>#` substring), and a one-line pipeline printed inside the
nros-rmw-xrce CMakeLists. This module is the one spelling; the config header
moving into resolve (D8) is what eventually retires it.
"""

from __future__ import annotations

import json
from typing import Iterable


def package_matches(package_id: str, name: str) -> bool:
    """True when a cargo `package_id` names `name`.

    Cargo spells these at least two ways and has changed the spelling before:

        path+file:///…/packages/api/nros-c#0.5.0
        registry+https://…#heapless@0.8.0

    So match the NAME rather than parse a format: either the `#name@version`
    tail, or the last path segment before `#`.
    """
    if "#" not in package_id:
        return package_id == name
    head, tail = package_id.rsplit("#", 1)
    if "@" in tail:
        return tail.rsplit("@", 1)[0] == name
    return head.rstrip("/").rsplit("/", 1)[-1] == name


def _executed(lines: Iterable[str]) -> Iterable[tuple[str, str]]:
    """`(package_id, out_dir)` of every `build-script-executed` message."""
    for line in lines:
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            msg = json.loads(line)
        except ValueError:
            continue
        if msg.get("reason") != "build-script-executed":
            continue
        od = msg.get("out_dir")
        if od:
            yield msg.get("package_id", ""), od


def out_dirs(lines: Iterable[str], package: str) -> list[str]:
    """Every `out_dir` cargo reported for `package`, in stream order.

    Several units of one package can run their build script in one invocation
    (one per feature set); the one a caller wants is normally the LAST.
    """
    return [od for pid, od in _executed(lines) if package_matches(pid, package)]


def artifact_files(lines: Iterable[str], package: str, suffix: str) -> list[str]:
    """The files cargo reported building for `package` (`compiler-artifact`'s
    `filenames`) that end in `suffix`, in stream order: where a staticlib
    actually landed, whatever target dir, build dir or profile made it so
    (issue 2400 W3, the archive a placer walks)."""
    found: list[str] = []
    for line in lines:
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            msg = json.loads(line)
        except ValueError:
            continue
        if msg.get("reason") != "compiler-artifact":
            continue
        if not package_matches(msg.get("package_id", ""), package):
            continue
        found += [f for f in msg.get("filenames") or [] if f.endswith(suffix)]
    return found


def all_out_dirs(lines: Iterable[str]) -> list[str]:
    """Every `out_dir` cargo reported, whatever the package, in stream order:
    for a consumer that looks for a FILE a build script wrote rather than for
    one package (issue 2400 W5, the stated stack)."""
    return [od for _pid, od in _executed(lines)]


def self_test() -> None:
    assert package_matches("path+file:///r/packages/api/nros-c#0.5.0", "nros-c")
    assert package_matches("registry+https://x#heapless@0.8.0", "heapless")
    assert not package_matches("path+file:///r/nros-cpp#0.5.0", "nros-c")
    # The defect the bare-substring match had: a sibling whose name ENDS with
    # the wanted one.
    assert not package_matches("path+file:///r/foo-nros-c#0.5.0", "nros-c")
    stream = [
        '{"reason":"compiler-artifact"}',
        'not json',
        '{"reason":"build-script-executed","package_id":"path+file:///a/nros-c#1","out_dir":"/o1"}',
        '{"reason":"build-script-executed","package_id":"path+file:///a/nros-cpp#1","out_dir":"/x"}',
        '{"reason":"build-script-executed","package_id":"path+file:///a/nros-c#1","out_dir":"/o2"}',
    ]
    assert out_dirs(stream, "nros-c") == ["/o1", "/o2"]
    assert all_out_dirs(stream) == ["/o1", "/x", "/o2"]
    art = [
        '{"reason":"compiler-artifact","package_id":"path+file:///a/nros-cpp#1",'
        '"filenames":["/t/x/release/libnros_cpp.a","/t/x/release/libnros_cpp.rlib"]}',
        '{"reason":"compiler-artifact","package_id":"path+file:///a/nros-c#1",'
        '"filenames":["/t/x/release/libnros_c.a"]}',
    ]
    assert artifact_files(art, "nros-cpp", ".a") == ["/t/x/release/libnros_cpp.a"]
    assert artifact_files(art, "absent", ".a") == []
    assert out_dirs(stream, "absent") == []
