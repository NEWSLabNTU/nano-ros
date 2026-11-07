#!/usr/bin/env python3
"""phase-535 W5 (nuttx arm): move the qemu-armv7a-nuttx pair rows into the behaviour bringups."""
import os, pathlib, re, sys

ROOT = pathlib.Path(os.environ.get("W5ROOT") or pathlib.Path(__file__).resolve().parents[2])
LANGS = ["rust", "c", "cpp"]
BRINGUPS = {
    "pubsub_bringup": (["talker", "listener"], 0),
    "service_bringup": (["service_server", "service_client"], 10),
    "action_bringup": (["action_server", "action_client"], 20),
}
# alloc::port_of(NuttxArm, lang, workload) — the leaves' locators.
BASE = {"rust": 8200, "c": 8300, "cpp": 8400}
TRIPLE = "armv7a-nuttx-eabihf"
RUST_ENV = ('{ RUSTUP_TOOLCHAIN = "nightly-2026-04-11", CARGO_UNSTABLE_BUILD_STD = "core,alloc,panic_abort", '
            'CARGO_UNSTABLE_BUILD_STD_FEATURES = "compiler-builtins-mem" }')


def images_block(lang, roles, off):
    out = ["", "# NuttX on QEMU virt (armv7-a). `locator` is the cell's `alloc::port_of`",
           "# on QEMU's SLIRP host. They came from the `examples/qemu-armv7a-nuttx` leaves."]
    for role in roles:
        out += ["", f"[image.nuttx_{role}]", 'board = "qemu-armv7a-nuttx"',
                f'launch = "{role}.launch.xml"']
        if lang == "rust":
            out.append('panic = "platform"')
        out.append(f'locator = "tcp/10.0.2.2:{BASE[lang] + off}"')
    return "\n".join(out) + "\n"


BOARD_CFG = '\n[board_config."qemu-armv7a-nuttx"]\n'


def add_images():
    for lang in LANGS:
        for b, (roles, off) in BRINGUPS.items():
            p = ROOT / f"examples/workspaces/{lang}/src/{b}/system.toml"
            s = p.read_text()
            if "[image.nuttx_" in s:
                continue
            m = re.search(r"^\[board_config\.", s, re.M)
            blk = images_block(lang, roles, off)
            if m:
                s = s[: m.start()] + blk.lstrip("\n") + "\n" + s[m.start():]
            else:
                s = s.rstrip("\n") + "\n" + blk
            if '[board_config."qemu-armv7a-nuttx"]' not in s:
                s = s.rstrip("\n") + "\n" + BOARD_CFG
            p.write_text(s)


def blocks(text):
    lines = text.splitlines(keepends=True)
    out = []
    i = 0
    while i < len(lines):
        if re.match(r"^\[\[\w+\]\]", lines[i]):
            start = i
            while start > 0 and lines[start - 1].startswith("#"):
                start -= 1
            j = i + 1
            while j < len(lines) and lines[j].strip() and not lines[j].startswith("[["):
                j += 1
            out.append((start, i, j, lines[i].strip(), "".join(lines[i + 1:j])))
            i = j
        else:
            i += 1
    return lines, out


def kv(body, key):
    m = re.search(rf'^{key}\s*=\s*"([^"]*)"', body, re.M)
    return m.group(1) if m else None


MOVED = ["listener", "service-server", "service-client", "action-server", "action-client"]


def row_text(lang, role, bringup):
    slug = role.replace("_", "-")
    rid = f"workspace-{lang}-nuttx-{slug}"
    l = ["[[workspace_fixture]]", f'id = "{rid}"', 'platform = "nuttx"', f'lang = "{lang}"',
         'rmw = "zenoh"', f'dir = "examples/workspaces/{lang}"', f'bringup = "src/{bringup}"',
         f'image = "nuttx_{role}"']
    if lang == "rust":
        l.append(f'codegen_out = "build-fixtures/{bringup}"')
        l.append(f'target_dir = "target-fixtures/nuttx-{slug}"')
        l.append(f'target = "{TRIPLE}"')
        l.append("skip_probe = true")
        l.append(f"env = {RUST_ENV}")
    else:
        l.append('build_subdir = "build/nuttx-zenoh-qemu-armv7a-nuttx/cmake"')
        l.append('cmake_defs = { NROS_PLATFORM = "nuttx", NROS_BOARD = "qemu-armv7a-nuttx" }')
    return "\n".join(l) + "\n"


def fixtures():
    p = ROOT / "examples/fixtures.toml"
    text = p.read_text()
    lines, bl = blocks(text)
    drop = {}
    last = None
    for (start, h, end, header, body) in bl:
        d = kv(body, "dir") or ""
        if header == "[[fixture]]":
            parts = d.split("/")
            if len(parts) == 4 and parts[:2] == ["examples", "qemu-armv7a-nuttx"]:
                if parts[3] in MOVED:
                    drop[start] = end
                else:
                    last = end
    new = []
    for lang in LANGS:
        for b, (roles, _off) in BRINGUPS.items():
            for role in roles:
                new.append(row_text(lang, role, b))
    hdr = ("\n# phase-535 W5 (RFC-0122 D1) — the NuttX (qemu-armv7a) example pairs as\n"
           "# images of the behaviour bringups. The cells resolve through\n"
           "# nros_tests::fixtures::workspace_image. The talker leaves stay: they are the\n"
           "# board's road witnesses (RFC-0122 D3).\n")
    lines[last - 1] = lines[last - 1] + hdr + "\n" + "\n".join(new)
    out = []
    i = 0
    while i < len(lines):
        if i in drop:
            j = drop[i]
            if j < len(lines) and not lines[j].strip():
                j += 1
            i = j
            continue
        out.append(lines[i])
        i += 1
    p.write_text("".join(out))
    print(f"dropped {len(drop)} leaf rows; added {len(new)} image rows")


if __name__ == "__main__":
    {"images": add_images, "fixtures": fixtures}[sys.argv[1]]()
