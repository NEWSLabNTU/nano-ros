#!/usr/bin/env python3
"""Measure a `[tool.*].dist.<host>` artifact's host FLOOR from its own binaries.

RFC-0099 D5 / phase-447 D1. A dist's `floor = { .. }` is what `nros setup`
compares the HOST against before it downloads anything, so the number has to be
read off the artifact, never guessed from the runner it was built on. A dist
built on `ubuntu-22.04` does not need glibc 2.35 just because the runner had it:
it needs the highest `GLIBC_x.y` symbol version any of its ELF files actually
references, and that is frequently lower.

WHAT IS MEASURED

Every file in the RELEASED archive (downloaded by URL, sha256-verified) whose
header says ELF for the artifact's own machine, or Mach-O:

* ELF — `readelf -W -d -l -V`:
  - `PT_INTERP` (the loader it asks for)       -> reported, not a floor field
  - `DT_NEEDED`                                 -> the external closure, minus
    what the dist ships itself (same rule as `check-dist-runtime-deps`)
  - version NEEDS (`.gnu.version_r`) against the loader/libc family
    (`libc.so.6`, `libm.so.6`, `libpthread`, `libdl`, `librt`, `ld-linux*`)
    -> `glibc` = the highest `GLIBC_x.y` referenced
  - version needs against `libstdc++.so.6` when the dist does NOT bundle it
    -> `glibcxx` = the highest `GLIBCXX_3.4.N`
* Mach-O (thin or fat) — `LC_BUILD_VERSION.minos` / `LC_VERSION_MIN_MACOSX`
  -> `macos` = the highest deployment target of any binary.

A Linux artifact with no dynamically-linked ELF at all (no `PT_INTERP`, no
`DT_NEEDED`) has no libc floor, and the script says so: that is what
`floor = { none = "<reason>" }` is for.

The archive is STREAMED (`tarfile` `r|*` over `zstd -dc`/`xz`/gzip, or
`zipfile`), so a 1.3 GB Zephyr SDK is inspected without being unpacked.

Usage:
  measure-dist-floor.py                      # every dist row in the index
  measure-dist-floor.py qemu ninja           # only these tools
  measure-dist-floor.py --host linux-arm64   # only this host column
  measure-dist-floor.py --cache DIR          # keep downloads (default tmp/dist-floor-cache)
  measure-dist-floor.py --json               # machine-readable
"""

import argparse
import hashlib
import json
import os
import re
import struct
import subprocess
import sys
import tarfile
import tempfile
import zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
INDEX = os.path.join(ROOT, "nros-sdk-index.toml")
DEFAULT_CACHE = os.path.join(ROOT, "tmp", "dist-floor-cache")

# ELF e_machine per host-key arch.
EM = {"x86_64": 62, "arm64": 183}
# The loader/libc family — the half of glibc `bundle.sh` never bundles, and
# therefore the half whose version is the host's to supply.
LIBC_FAMILY = re.compile(r"^(libc|libm|libpthread|libdl|librt|libutil|libresolv|ld-linux.*)\.so")
BASE = re.compile(
    r"^(libc|libm|libdl|libpthread|librt|libstdc\+\+|libgcc_s|libutil|libresolv"
    r"|ld-linux.*|linux-vdso)\.so"
)


def load_index():
    try:
        import tomllib as toml
    except ModuleNotFoundError:
        import tomli as toml
    with open(INDEX, "rb") as fh:
        return toml.load(fh)


def vkey(v):
    return tuple(int(p) if p.isdigit() else 0 for p in v.split("."))


def vmax(a, b):
    if a is None:
        return b
    if b is None:
        return a
    return a if vkey(a) >= vkey(b) else b


# --------------------------------------------------------------------- ELF


def readelf(path):
    out = subprocess.run(
        ["readelf", "-W", "-d", "-l", "-V", path], capture_output=True, text=True, timeout=120
    ).stdout
    interp, needed, verneed = None, set(), {}
    current = None
    for line in out.splitlines():
        m = re.search(r"Requesting program interpreter: ([^\]]+)\]", line)
        if m:
            interp = m.group(1)
        m = re.search(r"\(NEEDED\)\s+Shared library: \[([^\]]+)\]", line)
        if m:
            needed.add(m.group(1))
        m = re.search(r"Version: \d+\s+File: (\S+)\s+Cnt:", line)
        if m:
            current = m.group(1)
            verneed.setdefault(current, set())
            continue
        m = re.search(r"Name: (\S+)\s+Flags:", line)
        if m and current:
            verneed[current].add(m.group(1))
    return interp, needed, verneed


def elf_machine(head):
    if head[:4] != b"\x7fELF" or len(head) < 20:
        return None
    endian = "<" if head[5] == 1 else ">"
    return struct.unpack(endian + "H", head[18:20])[0]


# ------------------------------------------------------------------ Mach-O

LC_VERSION_MIN_MACOSX = 0x24
LC_BUILD_VERSION = 0x32
MH = {b"\xcf\xfa\xed\xfe": ("<", 32), b"\xce\xfa\xed\xfe": ("<", 28)}
FAT = {b"\xca\xfe\xba\xbe", b"\xca\xfe\xba\xbf"}


def _ver(x):
    return f"{x >> 16}.{(x >> 8) & 0xFF}"


def macho_minos(data):
    """Highest deployment target in a thin or fat Mach-O, or None."""
    magic = data[:4]
    if magic in FAT:
        (n,) = struct.unpack(">I", data[4:8])
        best = None
        wide = magic == b"\xca\xfe\xba\xbf"
        step = 32 if wide else 20
        for i in range(n):
            off = 8 + i * step
            if wide:
                _, _, o, s = struct.unpack(">iiQQ", data[off : off + 24])
            else:
                _, _, o, s, _ = struct.unpack(">iiIII", data[off : off + 20])
            best = vmax(best, macho_minos(data[o : o + s]))
        return best
    if magic not in MH:
        return None
    endian, hdr = MH[magic]
    ncmds = struct.unpack(endian + "I", data[16:20])[0]
    off, best = hdr, None
    for _ in range(ncmds):
        if off + 8 > len(data):
            break
        cmd, size = struct.unpack(endian + "II", data[off : off + 8])
        if cmd == LC_BUILD_VERSION:
            _, minos = struct.unpack(endian + "II", data[off + 8 : off + 16])
            best = vmax(best, _ver(minos))
        elif cmd == LC_VERSION_MIN_MACOSX:
            (v,) = struct.unpack(endian + "I", data[off + 8 : off + 12])
            best = vmax(best, _ver(v))
        off += max(size, 8)
    return best


# ---------------------------------------------------------------- archives


def members(archive):
    """Yield (name, bytes-reader) for every regular file in the archive."""
    name = archive.lower()
    if name.endswith((".zip", ".whl")):
        with zipfile.ZipFile(archive) as z:
            for info in z.infolist():
                if not info.is_dir():
                    yield info.filename, (lambda i=info: z.read(i))
        return
    if name.endswith((".zst", ".tzst")):
        proc = subprocess.Popen(["zstd", "-dc", archive], stdout=subprocess.PIPE)
        stream = proc.stdout
    else:
        proc, stream = None, open(archive, "rb")
    try:
        with tarfile.open(fileobj=stream, mode="r|*") as t:
            for m in t:
                if m.isfile():
                    f = t.extractfile(m)
                    yield m.name, (lambda f=f: f.read())
    finally:
        stream.close()
        if proc:
            proc.wait()


def dir_members(root):
    """Yield (name, reader) for every regular file under an unpacked dist."""
    # walk-ok: the subject is an unpacked SDK OUTSIDE the repository (e.g. an
    # installed Zephyr SDK); nothing in it is tracked.
    for dirpath, _, names in os.walk(root):
        for n in names:
            p = os.path.join(dirpath, n)
            if os.path.isfile(p) and not os.path.islink(p):
                yield os.path.relpath(p, root), (lambda p=p: open(p, "rb").read())


def measure_dir(root, host):
    return measure(root, host, source=dir_members(root))


def measure(archive, host, source=None):
    os_name, arch = host.split("-", 1)
    own, files, libc_paths = set(), [], []
    glibc = glibcxx = macos = None
    interps, needed_all, dynamic = set(), set(), 0
    elf_seen = 0
    with tempfile.TemporaryDirectory() as td:
        for name, read in source if source is not None else members(archive):
            base = os.path.basename(name)
            if ".so" in base or base.endswith(".dylib"):
                own.add(base)
            # A dist that ships its OWN loader + libc (the Zephyr SDK's
            # `sysroots/x86_64-pokysdk-linux`) runs those binaries against the
            # bundled copy, not the host's — so their GLIBC needs are not a
            # host floor. Remember where each bundled libc lives.
            if base.startswith("libc.so.6") or base.startswith("ld-linux"):
                libc_paths.append(name)
            data = read()
            head = data[:64]
            if os_name == "macos":
                v = macho_minos(data)
                if v:
                    macos = vmax(macos, v)
                continue
            if elf_machine(head) != EM.get(arch):
                continue
            elf_seen += 1
            p = os.path.join(td, "elf")
            with open(p, "wb") as fh:
                fh.write(data)
            interp, needed, verneed = readelf(p)
            if interp or needed:
                dynamic += 1
            if interp:
                interps.add(interp)
            needed_all |= needed
            files.append((name, verneed, interp))
    # The sysroot a bundled libc belongs to: `<root>/lib/libc.so.6` -> `<root>`.
    roots = {os.path.dirname(os.path.dirname(p)) for p in libc_paths}
    roots.discard("")
    system_loader = ("/lib/", "/lib64/", "/usr/lib/", "/usr/lib64/")

    def self_hosted(name, interp):
        if interp and not interp.startswith(system_loader):
            return True  # asks for the dist's own loader
        return any(name == r or name.startswith(r + "/") for r in roots)

    excluded = 0
    for name, verneed, interp in files:
        if self_hosted(name, interp):
            excluded += 1
            continue
        for lib, names in verneed.items():
            if LIBC_FAMILY.match(lib):
                for n in names:
                    if n.startswith("GLIBC_") and n[6:7].isdigit():
                        glibc = vmax(glibc, n[6:])
            elif lib == "libstdc++.so.6" and lib not in own:
                for n in names:
                    if n.startswith("GLIBCXX_"):
                        glibcxx = vmax(glibcxx, n[8:])
    external = sorted(s for s in needed_all if s not in own and not BASE.match(s))
    return {
        "glibc": glibc,
        "glibcxx": glibcxx,
        "macos": macos,
        "interp": sorted(interps),
        "external_needed": external,
        "elf_files": elf_seen,
        "dynamic_elf_files": dynamic,
        # ELF files run against a libc the dist bundles itself, and therefore
        # left out of the floor (reported, so the exclusion is never silent).
        "self_hosted_excluded": excluded,
        "bundles_libstdcxx": "libstdc++.so.6" in own,
    }


def fetch(url, sha256, dest):
    if not os.path.exists(dest):
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        subprocess.run(
            # Resumable: a 1.3 GB Zephyr SDK over a slow link dies mid-stream
            # (measured: curl 92, HTTP/2 stream reset at 221 MB), and a restart
            # from zero would never finish.
            [
                "curl", "-L", "--fail", "--silent", "--show-error",
                "--retry", "8", "--retry-all-errors", "-C", "-",
                "-o", dest + ".part", url,
            ],
            check=True,
        )
        os.replace(dest + ".part", dest)
    h = hashlib.sha256()
    with open(dest, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    if h.hexdigest() != sha256:
        raise SystemExit(f"sha256 mismatch for {url}: {h.hexdigest()} != {sha256}")
    return dest


def floor_of(host, m):
    """The `floor = { .. }` a measurement supports, as a TOML inline table."""
    if host.startswith("macos"):
        return f'{{ macos = "{m["macos"]}" }}' if m["macos"] else None
    if not m["dynamic_elf_files"]:
        return '{ none = "no dynamically-linked ELF: no PT_INTERP, no DT_NEEDED" }'
    if m.get("self_hosted_excluded", 0) >= m["dynamic_elf_files"]:
        return (
            '{ none = "every dynamic ELF runs on the dist\'s OWN bundled loader + '
            'glibc (a self-contained sysroot); nothing links the host libc" }'
        )
    parts = []
    if m["glibc"]:
        parts.append(f'glibc = "{m["glibc"]}"')
    if m["glibcxx"]:
        parts.append(f'glibcxx = "{m["glibcxx"]}"')
    return "{ " + ", ".join(parts) + " }" if parts else None


def self_test():
    """The parsers must be able to say something — and to say NOTHING."""
    # A thin arm64 Mach-O header with one LC_BUILD_VERSION, minos 11.0.
    lc = struct.pack("<IIIIII", LC_BUILD_VERSION, 24, 1, (11 << 16), (13 << 16), 0)
    hdr = b"\xcf\xfa\xed\xfe" + struct.pack("<iiIIIII", 0x0100000C, 0, 2, 1, len(lc), 0, 0)
    assert macho_minos(hdr + lc) == "11.0", macho_minos(hdr + lc)
    assert macho_minos(b"\x7fELF" + b"\0" * 60) is None
    elf = b"\x7fELF\x02\x01" + b"\0" * 12 + struct.pack("<H", 62)
    assert elf_machine(elf) == 62
    assert elf_machine(b"PK\x03\x04" + b"\0" * 20) is None
    assert vmax("2.17", "2.34") == "2.34" and vmax("2.4", "2.34") == "2.34"
    assert floor_of("linux-x86_64", {"dynamic_elf_files": 0}).startswith("{ none")
    # Every dynamic ELF on a bundled loader (a Yocto sysroot): no host floor,
    # and it must say so rather than print nothing.
    assert "OWN bundled loader" in floor_of(
        "linux-x86_64",
        {"dynamic_elf_files": 108, "self_hosted_excluded": 109, "glibc": None, "glibcxx": None},
    )
    # ...but ONE host-linked binary among them restores a real floor.
    assert floor_of(
        "linux-x86_64",
        {"dynamic_elf_files": 108, "self_hosted_excluded": 107, "glibc": "2.17", "glibcxx": None},
    ) == '{ glibc = "2.17" }'
    assert floor_of("linux-x86_64", {"dynamic_elf_files": 3, "glibc": "2.34", "glibcxx": None}) == (
        '{ glibc = "2.34" }'
    )


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("tools", nargs="*")
    ap.add_argument("--host")
    ap.add_argument("--cache", default=DEFAULT_CACHE)
    ap.add_argument("--json", action="store_true")
    ap.add_argument("-j", "--jobs", type=int, default=1, help="artifacts fetched in parallel")
    ap.add_argument(
        "--dir",
        help="measure an ALREADY-UNPACKED dist here instead of downloading "
        "(needs exactly one tool and --host; say where it came from)",
    )
    a = ap.parse_args()
    index = load_index()
    work = []
    for name, tool in sorted(index.get("tool", {}).items()):
        if a.tools and name not in a.tools:
            continue
        for host, dist in sorted(tool.get("dist", {}).items()):
            if a.host and host != a.host:
                continue
            work.append((name, host, dist))

    def one(item):
        name, host, dist = item
        if a.dir:
            m = measure_dir(a.dir, host)
        else:
            url = dist["url"]
            dest = os.path.join(a.cache, name, host, os.path.basename(url))
            m = measure(fetch(url, dist["sha256"], dest), host)
        m.update(tool=name, host=host, floor=floor_of(host, m))
        return m

    if a.dir and len(work) != 1:
        raise SystemExit("--dir measures ONE artifact: name one tool and pass --host")
    from concurrent.futures import ThreadPoolExecutor

    results = []
    with ThreadPoolExecutor(max_workers=max(1, a.jobs)) as pool:
        for m in pool.map(one, work):
            results.append(m)
            if not a.json:
                print(f"[tool.{m['tool']}] dist.{m['host']}: floor = {m['floor']}")
                print(
                    f"    elf={m['elf_files']} dynamic={m['dynamic_elf_files']} "
                    f"interp={m['interp']} bundles_libstdc++={m['bundles_libstdcxx']}"
                )
                if m["external_needed"]:
                    print(f"    external DT_NEEDED: {', '.join(m['external_needed'])}")
                sys.stdout.flush()
    if a.json:
        json.dump(results, sys.stdout, indent=2)
        print()
    return 0


if __name__ == "__main__":
    self_test()
    sys.exit(main())
