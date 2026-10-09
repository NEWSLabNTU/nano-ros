#!/usr/bin/env python3
"""Issue 1765 — a size-probe marker never reaches a linked image.

`nros::sizes::export_size!` emits, per probed type, a zero-filled static whose
SYMBOL SIZE is the number the `nros-c` / `nros-cpp` build scripts read back
(`__NROS_SIZE_<NAME>`), plus a fn-pointer marker that encodes the size in its
mangled name (`__NROS_SIZE_FN_<NAME>` -> `__nros_size_<NAME>::<N>`). They are a
build-time PROBE. Until issue 1765 they were also compiled into the staticlib
every C/C++ image links, because the feature that keeps them
(`ffi-size-markers`) was requested on `nros-c`'s and `nros-cpp`'s own `nros`
dependency. `#[no_mangle]` statics are exported from a staticlib, and the board
linker scripts put immutable data in `.text`, so each image carried the SUM of
the probed sizes as zero bytes in flash. Measured before the fix:

    image                                            text       markers
    FreeRTOS mps2-an385 C zenoh talker            587,256    111,252 B
    AN536 Cyclone C++ entry (committed bringup) 1,174,752     46,900 B
    same, bringup declaring `param_services`    1,546,552    327,784 B

The last row is phase-382 W3' carving the parameter store into
`EXECUTOR_SIZE`: the store paid once in `.bss` (intended) and again in `.text`.

The fix makes the markers PROBE-ONLY: the nested probe cargo requests the
feature for its own build of `nros` in its own target directory and marks that
build `NROS_SIZES_PROBE_BUILD=1`; `nros/build.rs` refuses the feature in any
other build. This gate holds that shape, in two modes.

DEFAULT (fast line, buildless) — the SOURCE rules, which reach every road by
construction, because every road (cargo leaf, cmake/Corrosion, west, the NuttX
`Make.defs` road) compiles `nros` through cargo with features named in a
tracked file:

  A. every marker item (`static`/`fn` named `__NROS_SIZE_*`, `__nros_size_*`,
     `__NROS_LU_SZ_*`) is `#[cfg(feature = "<probe-only feature>")]` — not a
     `cfg_attr(.., used)`, which is the shape that leaked: a `#[no_mangle]`
     static is exported whether or not it is `#[used]`;
  B. no tracked file ENABLES a probe-only feature — a dep-site `features = [..]`,
     a `[features]` forward, a `--features` in a script, recipe, workflow or
     cmake `FEATURES` list — except the probe's own request (`ENABLE_ALLOWED`);
  C. the refusal is wired: `nros/build.rs` reads `CARGO_FEATURE_FFI_SIZE_MARKERS`
     against the same variable `nros-sizes-build` sets on the nested cargo, and
     the probe requests the feature (or it would read every size as absent).

`--built` (a step after a fixture build) — the IMAGE rule, asked of the linker
output itself: no final image under the derived fixture roots defines a marker
symbol. The roots are DERIVED from the fixture manifest, one family per road,
and the scan prints how many images it examined on each, so a road that built
nothing reads as zero rather than as clean (issue 0196's reach question).
`--images PATH...` asks the same of named files (acceptance measurements).

The self-test runs on EVERY invocation (`check-gate-selftests`).
"""
from __future__ import annotations

import os
import re
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
import comments  # noqa: E402  the one comment stripper
import check_skip  # noqa: E402  NOT VERIFIED ledger (issue 1043)
from tracked import tracked  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
GATE = "size-markers-unlinked"

# The marker symbol families. A linked image must define none of them.
MARKER = re.compile(r"__NROS_SIZE_|__nros_size_|__NROS_LU_SZ_")
MARKER_BYTES = (b"__NROS_SIZE_", b"__nros_size_", b"__NROS_LU_SZ_")

# Probe-only feature -> the crate (repo-relative dir) that declares it.
PROBE_ONLY = {
    "ffi-size-markers": "packages/api/nros",
    "layout-size-markers": "packages/core/nros-node",
}

# Files that may NAME a probe-only feature as an enabler, and why.
ENABLE_ALLOWED = {
    # The size probe's request for its OWN nested build (issue 1765).
    "packages/tooling/nros-build-helpers/src/shared.rs",
    # The probe mechanism itself: its unit test feeds the feature to the
    # probe-only list, which is the one path that may carry it.
    "packages/tooling/nros-sizes-build/src/lib.rs",
    # A probe-marked test build (sets NROS_SIZES_PROBE_BUILD=1).
    "packages/tooling/nros-sizes-build/tests/bitcode_probe.rs",
    # Describes issue 0593's history in its docstring; enables nothing.
    "scripts/check-feature-contract.py",
    # This gate.
    "scripts/check-size-markers-unlinked.py",
}

# The variable that marks the probe's build; both spellings must agree.
NROS_BUILD_RS = "packages/api/nros/build.rs"
SIZES_BUILD_RS = "packages/tooling/nros-sizes-build/src/lib.rs"
PROBE_REQUEST_RS = "packages/tooling/nros-build-helpers/src/shared.rs"

SCAN_SUFFIXES = {".rs", ".toml", ".sh", ".py", ".just", ".cmake", ".yml", ".yaml", ".mk"}
SCAN_NAMES = {"justfile", "CMakeLists.txt", "Make.defs", "Makefile"}
SCAN_ROOTS = ("packages", "scripts", "just", "justfile", "cmake", "integrations",
              "examples", "zephyr", ".github", "Cargo.toml", "tests")

DECL = re.compile(r"\b(?:static|fn)\s+(?:\[<\s*)?((?:__NROS_SIZE_|__nros_size_|__NROS_LU_SZ_)\w*)")
CFG_FEATURE = re.compile(r'#\[\s*cfg\s*\(\s*feature\s*=\s*"([^"]+)"\s*\)\s*\]')


def rel(p: Path, root: Path) -> str:
    try:
        return str(p.relative_to(root))
    except ValueError:
        return str(p)


def lang_of(path: Path) -> str | None:
    lang = comments.lang_for(path)
    if lang is None and (path.name in ("Make.defs", "Makefile") or path.suffix == ".mk"):
        return "sh"  # `#` comments; good enough to blank them
    return lang


def code_text(path: Path, needles: tuple[str, ...] = ()) -> str:
    """`path` with comments blanked. With `needles`, a file that mentions none
    of them is returned raw and unstripped — stripping is the expensive part,
    and a file without the token cannot match after stripping either."""
    text = path.read_text(errors="replace")
    if needles and not any(n in text for n in needles):
        return text
    lang = lang_of(path)
    return comments.strip_comments(text, lang) if lang else text


def owning_feature(path_rel: str) -> str | None:
    for feat, crate in PROBE_ONLY.items():
        if path_rel.startswith(crate + "/"):
            return feat
    return None


# --- rule A ----------------------------------------------------------------
def rule_a(root: Path, files: list[Path]) -> list[str]:
    errs = []
    for f in files:
        if f.suffix != ".rs":
            continue
        r = rel(f, root)
        text = code_text(f, ("__NROS_SIZE_", "__nros_size_", "__NROS_LU_SZ_"))
        lines = text.splitlines()
        for i, line in enumerate(lines):
            m = DECL.search(line)
            if not m:
                continue
            # The attributes directly above the item (blank lines allowed).
            gates = set()
            j = i - 1
            while j >= 0 and (lines[j].strip().startswith("#[") or not lines[j].strip()):
                g = CFG_FEATURE.search(lines[j])
                if g:
                    gates.add(g.group(1))
                j -= 1
            want = owning_feature(r)
            if want is None:
                errs.append(f"{r}:{i + 1}: marker `{m.group(1)}` declared outside a crate "
                            f"that owns a probe-only feature ({sorted(PROBE_ONLY)})")
            elif want not in gates:
                errs.append(f"{r}:{i + 1}: marker `{m.group(1)}` is not "
                            f'`#[cfg(feature = "{want}")]` — a `cfg_attr(.., used)` or no gate '
                            f"at all leaves it in the linked staticlib (issue 1765)")
    return errs


# --- rule B ----------------------------------------------------------------
def rule_b(root: Path, files: list[Path]) -> list[str]:
    errs = []
    for f in files:
        r = rel(f, root)
        if r in ENABLE_ALLOWED:
            continue
        try:
            text = code_text(f, tuple(PROBE_ONLY))
        except (OSError, ValueError):
            continue
        for feat, crate in PROBE_ONLY.items():
            if feat not in text:
                continue
            own_crate = r.startswith(crate + "/")
            for n, line in enumerate(text.splitlines(), 1):
                if feat not in line:
                    continue
                if own_crate and f.name == "Cargo.toml" and re.match(rf"\s*{re.escape(feat)}\s*=", line):
                    continue  # the declaration itself
                if own_crate and f.suffix == ".rs":
                    # A crate cannot enable its own feature from its source; it
                    # can only gate on it (`cfg`) or refuse it (`build.rs`).
                    continue
                errs.append(f"{r}:{n}: enables probe-only feature `{feat}` — only the size "
                            f"probe's nested build may (issue 1765): {line.strip()[:120]}")
    return errs


# --- rule C ----------------------------------------------------------------
def probe_env_value(text: str) -> str | None:
    m = re.search(r'const\s+PROBE_BUILD_ENV\s*:\s*&str\s*=\s*"([^"]+)"', text)
    return m.group(1) if m else None


def rule_c(root: Path) -> list[str]:
    errs = []
    try:
        nros_b = code_text(root / NROS_BUILD_RS)
        sizes_b = code_text(root / SIZES_BUILD_RS)
        req = code_text(root / PROBE_REQUEST_RS)
    except OSError as e:
        return [f"rule C: cannot read {e.filename}"]
    a, b = probe_env_value(nros_b), probe_env_value(sizes_b)
    if a is None or b is None:
        errs.append(f"rule C: `PROBE_BUILD_ENV` missing from {NROS_BUILD_RS} or {SIZES_BUILD_RS}")
    elif a != b:
        errs.append(f"rule C: probe variable disagrees — {NROS_BUILD_RS} reads `{a}`, "
                    f"{SIZES_BUILD_RS} sets `{b}`; the refusal would fire on the probe itself")
    if "CARGO_FEATURE_FFI_SIZE_MARKERS" not in nros_b or "refuse_size_markers_outside_the_probe()" not in nros_b:
        errs.append(f"rule C: {NROS_BUILD_RS} no longer refuses `ffi-size-markers` outside the probe")
    if not re.search(r"\.env\(\s*PROBE_BUILD_ENV\s*,\s*\"1\"\s*\)", sizes_b):
        errs.append(f"rule C: {SIZES_BUILD_RS} no longer marks the nested probe build")
    if '"ffi-size-markers"' not in req or "find_dep_rlib_with_probe_features" not in req:
        errs.append(f"rule C: {PROBE_REQUEST_RS} no longer requests `ffi-size-markers` for the "
                    f"probe — every probed size would read as absent")
    return errs


def scan_files(root: Path) -> list[Path]:
    out = []
    for f in tracked(*[r for r in SCAN_ROOTS if (root / r).exists()], repo=root):
        if f.suffix in SCAN_SUFFIXES or f.name in SCAN_NAMES:
            if "generated" in f.parts:
                continue
            out.append(f)
    return out


def source_rules(root: Path) -> list[str]:
    files = scan_files(root)
    return rule_a(root, files) + rule_b(root, files) + rule_c(root)


# --- the image rule --------------------------------------------------------
def elf_kind(path: Path) -> str | None:
    """'exec' for a final linked image, else None.

    EXEC, or DYN with a PT_INTERP (a PIE executable). A shared object or a
    relocatable is not an image; an archive is not ELF at all.
    """
    try:
        with open(path, "rb") as fh:
            hdr = fh.read(64)
            if len(hdr) < 52 or hdr[:4] != b"\x7fELF":
                return None
            is64 = hdr[4] == 2
            end = "<" if hdr[5] == 1 else ">"
            e_type = struct.unpack_from(end + "H", hdr, 16)[0]
            if e_type == 2:
                return "exec"
            if e_type != 3:
                return None
            if is64:
                phoff = struct.unpack_from(end + "Q", hdr, 32)[0]
                phentsize, phnum = struct.unpack_from(end + "HH", hdr, 54)
            else:
                phoff = struct.unpack_from(end + "I", hdr, 28)[0]
                phentsize, phnum = struct.unpack_from(end + "HH", hdr, 42)
            for k in range(min(phnum, 64)):
                fh.seek(phoff + k * phentsize)
                p_type = struct.unpack(end + "I", fh.read(4))[0]
                if p_type == 3:  # PT_INTERP
                    return "exec"
    except (OSError, struct.error):
        return None
    return None


def has_marker_bytes(path: Path) -> bool:
    """Prefilter: does the file's bytes mention a marker name at all?"""
    tail = b""
    try:
        with open(path, "rb") as fh:
            while True:
                chunk = fh.read(1 << 22)
                if not chunk:
                    return False
                buf = tail + chunk
                if any(m in buf for m in MARKER_BYTES):
                    return True
                tail = buf[-16:]
    except OSError:
        return False


def marker_symbols(path: Path) -> list[str]:
    """Defined marker symbols (with size) per `llvm-nm`/`nm`; a bytes hit with no
    tool still counts as a finding — the name is in the image either way."""
    if not has_marker_bytes(path):
        return []
    for nm in ("llvm-nm", "nm"):
        try:
            out = subprocess.run([nm, "-S", "--defined-only", str(path)],
                                 capture_output=True, text=True, check=True).stdout
        except (OSError, subprocess.CalledProcessError):
            continue
        hits = [ln.strip() for ln in out.splitlines() if MARKER.search(ln)]
        return hits
    return [f"(marker name bytes present; no nm to list them) {path}"]


PRUNE = {".git", "CMakeFiles", "deps", ".fingerprint", "incremental", "src", "generated",
         "include", "nano_ros", "corrosion"}


def images_under(root: Path) -> list[Path]:
    out = []
    if not root.is_dir():
        return out
    # walk-ok: `root` is a BUILD directory derived from the fixture manifest;
    # what this looks for (a linked image) is untracked by construction.
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in PRUNE
                       and not (d == "build" and os.path.isdir(os.path.join(dirpath, "deps")))]
        for n in filenames:
            if n.startswith("build-script-") or n.endswith((".so", ".o", ".a", ".rlib", ".d", ".rmeta")):
                continue
            p = Path(dirpath, n)
            if elf_kind(p):
                out.append(p)
    return out


def _manifest(sub: str, coords: str | None = None) -> list[list[str]]:
    """Records of `fixtures-manifest.py <sub>`. Fatal on failure: an empty
    root set reads exactly like "nothing is built" (issue 1001)."""
    cmd = [sys.executable, str(ROOT / "scripts/build/fixtures-manifest.py"), sub]
    if coords:
        cmd += ["--coords-from", coords]
    out = subprocess.run(cmd, capture_output=True, text=True, check=True).stdout
    return [ln.split("\x1f") for ln in out.splitlines() if ln.strip()]


def lane_coords(path: str | None) -> set[tuple[str, str, str]] | None:
    if not path:
        return None
    out = set()
    for ln in Path(path).read_text().splitlines():
        ln = ln.strip()
        if ln and not ln.startswith("#"):
            parts = tuple(x.strip() for x in ln.split(","))
            if len(parts) == 3:
                out.add(parts)
    return out


def derived_roots(coords_file: str | None) -> dict[str, set[Path]]:
    """{road: roots}, derived from the fixture manifest — never a literal list.

    Scoped to the RUN's lane when `NROS_TEST_COORDS` names one (every `just ci`
    tier exports it): a lane answers for the images it built, and an older
    out-of-lane image left on disk is not this lane's evidence either way.
    Unset (a manual run, `just ci full`) means every row.

    One root per row and BUILD DIR, not per leaf: a leaf can hold `build-zenoh/`
    and `build-xrce/` for two different rows, and only the in-lane one counts.
    """
    want = lane_coords(coords_file)
    build_root = Path(os.environ.get("NROS_BUILD_ROOT") or ROOT / "build")
    zephyr_root = subprocess.run(
        ["bash", str(ROOT / "scripts/lib/zephyr-workspace.sh"), "--root", str(ROOT), "build-root"],
        capture_output=True, text=True).stdout.strip() or str(build_root / "zephyr-workspace-builds")
    roads: dict[str, set[Path]] = {"cmake": set(), "cargo": set(), "west": set(), "nuttx-make": set()}
    # cargo leaves: the shared group dir their row redirects into (phase-340).
    for row in _manifest("fixture-groups"):
        coord = tuple(row[-3:]) if len(row) >= 3 else ()
        if want is not None and coord not in want:
            continue
        slug = row[2] if len(row) > 2 else ""
        roads["cargo"].add(build_root / "cargo-fixtures" / slug if slug else ROOT / row[0])
    # cmake leaves: `<leaf>/<build dir>`.
    for row in _manifest("list", coords_file):
        if len(row) > 1 and row[0] and row[1]:
            roads["cmake"].add(ROOT / row[0] / row[1])
    # workspaces: the cmake build_subdir, else the cargo target + build dirs.
    for row in _manifest("list-workspaces", coords_file):
        if len(row) < 3 or not row[2]:
            continue
        ws = ROOT / row[2]
        for k in (5, 6, 7):
            if len(row) > k and row[k]:
                (roads["cmake"] if k == 5 else roads["cargo"]).add(ws / row[k])
    # west: `<zephyr build root>/<build dir>/zephyr/zephyr.{exe,elf}`.
    for row in _manifest("west-leaves", coords_file):
        if len(row) > 6 and row[6]:
            roads["west"].add(Path(zephyr_root) / row[6] / "zephyr")
    # cmake reaching cargo through Corrosion, and the host link proof.
    roads["cargo"].add(build_root / "corrosion-cargo")
    roads["cargo"].add(build_root / "link-determinism")
    # The NuttX `Make.defs` road links the Rust staticlib into the KERNEL image.
    if want is None or any(c[0] == "nuttx" for c in want):
        roads["nuttx-make"].add(Path(os.environ.get("NUTTX_DIR") or ROOT / "third-party/nuttx/nuttx"))
    return roads


def scan_images(paths: list[Path]) -> list[str]:
    errs = []
    for p in paths:
        for hit in marker_symbols(p):
            errs.append(f"{rel(p, ROOT)}: {hit}")
    return errs


def built_mode() -> int:
    roads = derived_roots(os.environ.get("NROS_TEST_COORDS"))
    seen: set[Path] = set()
    per_road = {}
    all_imgs = []
    for road, roots in roads.items():
        imgs = []
        for r in sorted(roots):
            if road == "nuttx-make":
                cand = [r / "nuttx"] if (r / "nuttx").is_file() and elf_kind(r / "nuttx") else []
            elif road == "west":
                cand = [r / n for n in ("zephyr.exe", "zephyr.elf") if (r / n).is_file()]
            else:
                cand = images_under(r)
            for c in cand:
                rp = c.resolve()
                if rp not in seen:
                    seen.add(rp)
                    imgs.append(c)
        per_road[road] = len(imgs)
        all_imgs.extend(imgs)
    print(f"{GATE}: images examined per road: "
          + ", ".join(f"{k}={v}" for k, v in per_road.items()))
    if not all_imgs:
        return check_skip.unverified(GATE + "-image", "no built image under any derived fixture root")
    errs = scan_images(all_imgs)
    if errs:
        print(f"{GATE}: FAIL — {len(errs)} size-probe marker symbol(s) in linked images "
              f"(issue 1765):", file=sys.stderr)
        for e in errs[:60]:
            print(f"  {e}", file=sys.stderr)
        return 1
    print(f"{GATE}: OK — no size-probe marker in {len(all_imgs)} linked image(s)")
    return 0


# --- self-test -------------------------------------------------------------
def _w(base: Path, relp: str, text: str) -> Path:
    p = base / relp
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(text)
    return p


GOOD_MACRO = '''macro_rules! export_size {
    ($n:ident = $t:ty) => { paste::paste! {
        #[cfg(feature = "ffi-size-markers")]
        #[used]
        #[unsafe(no_mangle)]
        pub static [<__NROS_SIZE_ $n>]: [u8; $n] = [0u8; $n];
        #[cfg(feature = "ffi-size-markers")]
        #[inline(never)]
        pub fn [<__nros_size_ $n>]<const N: usize>() -> usize { N }
    } };
}
'''
LEAKY_MACRO = GOOD_MACRO.replace('#[cfg(feature = "ffi-size-markers")]\n        #[used]',
                                 '#[cfg_attr(feature = "ffi-size-markers", used)]', 1)


def _elf32(e_type: int, payload: bytes = b"", symbol: str | None = None, size: int = 0) -> bytes:
    """A minimal little-endian ELF32 (ARM) — with one defined global OBJECT
    `symbol` of `size` bytes when given — so the image rule is exercised through
    a real `nm`, not only through the byte prefilter."""
    hdr = bytearray(52)
    hdr[:4] = b"\x7fELF"
    hdr[4], hdr[5], hdr[6] = 1, 1, 1
    struct.pack_into("<HHI", hdr, 16, e_type, 40, 1)
    struct.pack_into("<H", hdr, 40, 52)
    if symbol is None:
        return bytes(hdr) + payload
    text = payload or b"\0" * 16
    strtab = b"\0" + symbol.encode() + b"\0"
    shstr = b"\0.text\0.symtab\0.strtab\0.shstrtab\0"
    names = {n: shstr.index(n.encode()) for n in (".text", ".symtab", ".strtab", ".shstrtab")}
    sym = bytes(16) + struct.pack("<IIIBBH", 1, 0, size, 0x11, 0, 1)
    off_text = 52
    off_str = off_text + len(text)
    off_shs = off_str + len(strtab)
    off_sym = (off_shs + len(shstr) + 3) & ~3
    body = text + strtab + shstr + b"\0" * (off_sym - off_shs - len(shstr)) + sym
    shoff = off_sym + len(sym)
    sh = bytes(40)
    sh += struct.pack("<10I", names[".text"], 1, 6, 0, off_text, len(text), 0, 0, 4, 0)
    sh += struct.pack("<10I", names[".symtab"], 2, 0, 0, off_sym, len(sym), 3, 1, 4, 16)
    sh += struct.pack("<10I", names[".strtab"], 3, 0, 0, off_str, len(strtab), 0, 0, 1, 0)
    sh += struct.pack("<10I", names[".shstrtab"], 3, 0, 0, off_shs, len(shstr), 0, 0, 1, 0)
    struct.pack_into("<IIHHHHHH", hdr, 32, shoff, 0, 52, 0, 0, 40, 5, 4)
    return bytes(hdr) + body + sh


def self_test() -> None:
    def expect(cond: bool, what: str) -> None:
        if not cond:
            raise SystemExit(f"{GATE} self-test FAILED: {what}")

    with tempfile.TemporaryDirectory() as td:
        t = Path(td)
        good = _w(t, "packages/api/nros/src/sizes.rs", GOOD_MACRO)
        expect(rule_a(t, [good]) == [], "rule A flagged a correctly gated marker")
        leak = _w(t, "packages/api/nros/src/leak.rs", LEAKY_MACRO)
        expect(len(rule_a(t, [leak])) == 1, "rule A missed a `cfg_attr(.., used)` marker (issue 1765's shape)")
        stray = _w(t, "packages/api/nros-c/src/x.rs", GOOD_MACRO)
        expect(rule_a(t, [stray]), "rule A missed a marker in a crate that owns no probe-only feature")
        commented = _w(t, "packages/api/nros/src/c.rs", "// pub static __NROS_SIZE_X: [u8; 1] = [0];\n")
        expect(rule_a(t, [commented]) == [], "rule A read a comment as a declaration")

        decl = _w(t, "packages/api/nros/Cargo.toml",
                  '[features]\n# ffi-size-markers is probe-only\nffi-size-markers = []\n')
        gate_use = _w(t, "packages/api/nros/src/g.rs", '#[cfg(feature = "ffi-size-markers")]\nfn f() {}\n')
        expect(rule_b(t, [decl, gate_use]) == [], "rule B flagged the declaration / the crate's own cfg")
        dep = _w(t, "packages/api/nros-c/Cargo.toml",
                 '[dependencies]\nnros = { path = "../nros", features = ["ffi-size-markers"] }\n')
        expect(len(rule_b(t, [dep])) == 1, "rule B missed a dep-site enabling the markers (the pre-1765 shape)")
        fwd = _w(t, "packages/api/nros-cpp/Cargo.toml", '[features]\nx = ["nros/ffi-size-markers"]\n')
        expect(len(rule_b(t, [fwd])) == 1, "rule B missed a `[features]` forward")
        cli = _w(t, "just/x.just", 'r:\n    cargo build -p nros --features rmw-cffi,ffi-size-markers\n')
        expect(len(rule_b(t, [cli])) == 1, "rule B missed a `--features` in a recipe")
        cm = _w(t, "cmake/x.cmake", 'corrosion_import_crate(FEATURES layout-size-markers)\n')
        expect(len(rule_b(t, [cm])) == 1, "rule B missed a cmake FEATURES list")
        cmt = _w(t, "scripts/y.sh", '# never pass ffi-size-markers here\necho ok\n')
        expect(rule_b(t, [cmt]) == [], "rule B read a comment as an enabler")

        _w(t, NROS_BUILD_RS, 'const PROBE_BUILD_ENV: &str = "NROS_SIZES_PROBE_BUILD";\n'
           'fn main() { refuse_size_markers_outside_the_probe(); }\n'
           'fn refuse_size_markers_outside_the_probe() { let _ = "CARGO_FEATURE_FFI_SIZE_MARKERS"; }\n')
        _w(t, SIZES_BUILD_RS, 'pub const PROBE_BUILD_ENV: &str = "NROS_SIZES_PROBE_BUILD";\n'
           'fn f(cmd: &mut C) { cmd.env(PROBE_BUILD_ENV, "1"); }\n')
        _w(t, PROBE_REQUEST_RS, 'const F: &str = "ffi-size-markers";\n'
           'fn p() { find_dep_rlib_with_probe_features("nros", "__NROS_SIZE_", &[F]); }\n')
        expect(rule_c(t) == [], f"rule C flagged a wired refusal: {rule_c(t)}")
        _w(t, SIZES_BUILD_RS, 'pub const PROBE_BUILD_ENV: &str = "NROS_SIZE_PROBE";\n'
           'fn f(cmd: &mut C) { cmd.env(PROBE_BUILD_ENV, "1"); }\n')
        expect(any("disagrees" in e for e in rule_c(t)), "rule C missed the two spellings drifting")
        _w(t, PROBE_REQUEST_RS, 'fn p() { find_dep_rlib("nros", "__NROS_SIZE_"); }\n')
        expect(any("requests" in e for e in rule_c(t)), "rule C missed the probe no longer asking")

        # The image rule: an executable carrying a marker name is found, a clean
        # one is not, and neither an archive nor a relocatable is an image.
        dirty = t / "img" / "dirty"
        dirty.parent.mkdir()
        dirty.write_bytes(_elf32(2, symbol="__NROS_SIZE_EXECUTOR_SIZE", size=24976))
        clean = t / "img" / "clean"
        clean.write_bytes(_elf32(2, symbol="nros_executor_spin", size=4))
        # The name as a STRING, not a symbol (a log line, a debug string): not
        # a marker, so not a finding — the prefilter alone would have said yes.
        quoted = t / "img" / "quoted"
        quoted.write_bytes(_elf32(2, b"see __NROS_SIZE_X\0", symbol="main", size=4))
        reloc = t / "img" / "x.o"
        reloc.write_bytes(_elf32(1, b"__NROS_SIZE_X\0"))
        (t / "img" / "lib.a").write_bytes(b"!<arch>\n__NROS_SIZE_X")
        expect(elf_kind(dirty) == "exec" and elf_kind(reloc) is None, "ELF image classification")
        expect(has_marker_bytes(dirty) and not has_marker_bytes(clean), "marker byte prefilter")
        found = images_under(t / "img")
        expect(sorted(p.name for p in found) == ["clean", "dirty", "quoted"], f"image walk found {found}")
        hits = scan_images([dirty])
        expect(len(hits) == 1 and "__NROS_SIZE_EXECUTOR_SIZE" in hits[0],
               f"the image scan missed a defined marker symbol: {hits}")
        expect(scan_images([clean]) == [] and scan_images([quoted]) == [],
               "the image scan flagged an image that defines no marker")
    check_skip.self_test()


def main(argv: list[str]) -> int:
    self_test()
    if argv[:1] == ["--self-test"]:
        print(f"{GATE}: self-test OK")
        return 0
    if argv[:1] == ["--images"]:
        paths = [Path(a) for a in argv[1:]]
        errs = scan_images(paths)
        for e in errs:
            print(f"  {e}", file=sys.stderr)
        print(f"{GATE}: {'FAIL' if errs else 'OK'} — {len(paths)} image(s), {len(errs)} marker symbol(s)")
        return 1 if errs else 0
    if argv[:1] == ["--built"]:
        return built_mode()
    if argv:
        print(f"usage: {sys.argv[0]} [--self-test | --built | --images ELF...]", file=sys.stderr)
        return 2
    errs = source_rules(ROOT)
    if errs:
        print(f"{GATE}: FAIL — {len(errs)} violation(s) (issue 1765):", file=sys.stderr)
        for e in errs:
            print(f"  {e}", file=sys.stderr)
        return 1
    print(f"{GATE}: OK — every size marker is probe-only, nothing outside the probe enables it, "
          f"and nros/build.rs refuses it elsewhere")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
