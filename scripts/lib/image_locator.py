#!/usr/bin/env python3
"""The locator an image STATES in its `system.toml` — issue 1865.

An embedded image dials the locator `nros::main!` bakes from its `system.toml`
(`[image.<id>] locator` over `[image_defaults]`, then the `router` of the
network its first interface sits on (RFC-0103 D10d, phase-497 W2), then
`[system]` — the overlay `nros_orchestration_ir::leaf_system` resolves). Whoever starts the router that
image dials must listen on THAT port. When a recipe spelled the port itself,
the two parted: `just qemu test-rtic-main-e2e` listened on 10200 while its
image baked the board default `tcp/10.0.2.2:7450`, and nothing compared them.

So the router side reads the same file the build reads, through this one
spelling, and `check-image-locator-bake` resolves the image side through it
too. Python and Rust (`nros_tests::fixtures::baked_locator`) each read the
overlay; there is no third reader.

Usage::

    image_locator.py <leaf-dir | system.toml> [--image <id>] [--port]

A leaf (a `system.toml` declaring ONE image) needs no `--image`. Prints the
locator, or with `--port` its TCP port. Exits 1 with the reason when the image
states none — a router recipe must never fall back to a literal.
"""

import os
import sys

try:
    import tomllib
except ImportError:  # Python < 3.11
    import tomli as tomllib


def load(path):
    if os.path.isdir(path):
        path = os.path.join(path, "system.toml")
    with open(path, "rb") as f:
        return tomllib.load(f), path


def leaf_image_id(doc):
    """The image a leaf builds when none is named: its ONE image, else the one
    `[system] default_images` names (the leaf reader's rule), else None."""
    images = sorted(doc.get("image", {}))
    if len(images) == 1:
        return images[0]
    defaults = doc.get("system", {}).get("default_images", [])
    if len(defaults) == 1 and defaults[0] in images:
        return defaults[0]
    return None


def stated_locator(doc, image_id):
    """`(locator | None, error | None)` for `[image.<image_id>]`."""
    image = doc.get("image", {}).get(image_id)
    if image is None:
        return None, f"`[image.{image_id}]` is not declared"
    for table in (image, doc.get("image_defaults", {})):
        if isinstance(table.get("locator"), str):
            return table["locator"], None
    router = network_router(doc, image)
    if router is not None:
        return router, None
    system = doc.get("system", {})
    if isinstance(system.get("locator"), str):
        return system["locator"], None
    return None, None


def network_router(doc, image):
    """D10d — the `router` of the network the image's FIRST interface (by name,
    `[image.<id>.net]` over `[image_defaults.net]`) names, or None. Mirrors
    `nros_orchestration_ir::system_net::network_router`."""
    ifs = dict(doc.get("image_defaults", {}).get("net", {}) or {})
    ifs.update(image.get("net", {}) or {})
    if not ifs:
        return None
    first = ifs[sorted(ifs)[0]]
    name = first.get("network") if isinstance(first, dict) else None
    router = doc.get("network", {}).get(name, {}).get("router") if name else None
    return router if isinstance(router, str) else None


def locator_port(locator):
    try:
        return int(locator.rsplit(":", 1)[1])
    except (IndexError, ValueError):
        return None


def main(argv):
    args = list(argv)
    want_port = "--port" in args
    if want_port:
        args.remove("--port")
    image_id = None
    if "--image" in args:
        i = args.index("--image")
        image_id = args[i + 1]
        del args[i : i + 2]
    if len(args) != 1:
        print(__doc__.split("Usage::")[1].strip().splitlines()[0].strip(), file=sys.stderr)
        return 2
    doc, path = load(args[0])
    if image_id is None:
        image_id = leaf_image_id(doc)
        if image_id is None:
            print(f"image_locator: {path} declares {len(doc.get('image', {}))} images; "
                  f"name one with --image", file=sys.stderr)
            return 1
    locator, err = stated_locator(doc, image_id)
    if err or locator is None:
        print(f"image_locator: {path}: {err or f'`[image.{image_id}]` states no locator'} — "
              f"the image dials its board's compiled-in default, which no recipe can name "
              f"(issue 1865). State `locator` there.", file=sys.stderr)
        return 1
    if want_port:
        port = locator_port(locator)
        if port is None:
            print(f"image_locator: {path}: `{locator}` carries no TCP port", file=sys.stderr)
            return 1
        print(port)
    else:
        print(locator)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
