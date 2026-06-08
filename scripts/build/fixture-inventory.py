#!/usr/bin/env python3
"""Emit a read-only inventory of fixture build leaves.

This is a Phase 226.A diagnostic. It combines:

* examples/fixtures.toml single-package fixture rows;
* examples/fixtures.toml workspace fixture rows;
* Zephyr's expanded leaf emitter; and
* the small set of hand-authored recipe leaves that are not yet in the
  fixture manifest.

The script does not build, configure, run codegen, or mutate fixture
directories.
"""

import argparse
import csv
import hashlib
import os
import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib


FIELDS = [
    "source",
    "id",
    "platform",
    "kind",
    "lang",
    "rmw",
    "role",
    "dir",
    "build_root",
    "scheduler",
    "shared_mutation",
    "notes",
]


def load_toml(path):
    with path.open("rb") as f:
        return tomllib.load(f)


def short_hash(value):
    return hashlib.sha1(value.encode("utf-8")).hexdigest()[:8]


def stable_id(row):
    if row.get("id"):
        return row["id"]
    parts = [
        row.get("platform", ""),
        row.get("lang", ""),
        row.get("rmw", ""),
        row.get("dir", ""),
        ",".join(row.get("features") or []),
        row.get("target_dir", ""),
        row.get("target", ""),
    ]
    stem = "-".join(p for p in parts[:4] if p).replace("/", "-").replace("_", "-")
    return f"{stem}-{short_hash('|'.join(parts))}"


def role_from_dir(path):
    name = Path(path).name
    if name in {"nros-bench", "bins", "rust", "c", "cpp"}:
        return ""
    return name


def fixture_build_root(row):
    lang = row.get("lang", "")
    directory = row.get("dir", "")
    if lang in {"c", "cpp"}:
        subdir = row.get("build_subdir") or (f"build-{row['rmw']}" if row.get("rmw") else "build")
        return f"{directory}/{subdir}"
    target_dir = row.get("target_dir") or "target"
    return f"{directory}/{target_dir}"


def fixture_scheduler(row):
    platform = row.get("platform", "")
    lang = row.get("lang", "")
    rmw = row.get("rmw", "")
    fixture_id = row.get("id", "")

    if platform == "native" and lang in {"c", "cpp"}:
        if rmw == "cyclonedds":
            return "scripts/build/fixture-make-driver.sh native-cyclonedds-cmake"
        return "scripts/build/fixture-make-driver.sh native-cmake-rmw"

    if fixture_id:
        return f"scripts/build/fixtures-build.sh {platform} {lang} --id {fixture_id}"
    if rmw:
        return f"scripts/build/fixtures-build.sh {platform} {lang} {rmw}"
    return f"scripts/build/fixtures-build.sh {platform} {lang}"


def fixture_mutation(row):
    roots = [fixture_build_root(row)]
    if row.get("lang") == "rust" and row.get("dir", "").startswith("examples/"):
        roots.append(f"{row['dir']}/.cargo/config.toml")
        roots.append(f"{row['dir']}/generated")
    return "; ".join(roots)


def fixture_notes(row):
    notes = []
    if row.get("features"):
        notes.append("features=" + ",".join(row["features"]))
    if row.get("no_default_features"):
        notes.append("no-default-features")
    if row.get("target"):
        notes.append("target=" + row["target"])
    if row.get("env"):
        notes.append("env=" + ",".join(sorted(row["env"].keys())))
    if row.get("skip_probe"):
        notes.append("skip-probe")
    return "; ".join(notes)


def manifest_fixture_rows(manifest):
    for row in manifest.get("fixture", []):
        yield {
            "source": "examples/fixtures.toml",
            "id": stable_id(row),
            "platform": row.get("platform", ""),
            "kind": "manifest-cmake" if row.get("lang") in {"c", "cpp"} else "manifest-cargo",
            "lang": row.get("lang", ""),
            "rmw": row.get("rmw", ""),
            "role": role_from_dir(row.get("dir", "")),
            "dir": row.get("dir", ""),
            "build_root": fixture_build_root(row),
            "scheduler": fixture_scheduler(row),
            "shared_mutation": fixture_mutation(row),
            "notes": fixture_notes(row),
        }


def workspace_rows(manifest):
    for row in manifest.get("workspace_fixture", []):
        build_root = row.get("target_dir") if row.get("lang") == "rust" else row.get("build_subdir", "")
        mutations = [f"{row.get('dir', '')}/{row.get('codegen_out', '')}"]
        if row.get("lang") == "rust":
            mutations.append(f"{row.get('dir', '')}/.cargo/config.toml")
        if build_root:
            mutations.append(f"{row.get('dir', '')}/{build_root}")
        yield {
            "source": "examples/fixtures.toml",
            "id": row.get("id", ""),
            "platform": row.get("platform", ""),
            "kind": "workspace",
            "lang": row.get("lang", ""),
            "rmw": row.get("rmw", ""),
            "role": row.get("entry", ""),
            "dir": row.get("dir", ""),
            "build_root": f"{row.get('dir', '')}/{build_root}" if build_root else row.get("dir", ""),
            "scheduler": f"scripts/build/workspace-fixtures-build.sh {row.get('platform', '')}",
            "shared_mutation": "; ".join(m for m in mutations if not m.endswith("/")),
            "notes": f"bringup={row.get('bringup', '')}; entry={row.get('entry', '')}",
        }


def zephyr_rows(repo_root, include_zephyr):
    if not include_zephyr:
        return []

    cmd = [
        str(repo_root / "scripts/build/zephyr-fixture-leaves.sh"),
        "--emit",
        "records",
        "--include-logging-smoke",
    ]
    try:
        proc = subprocess.run(cmd, cwd=repo_root, text=True, capture_output=True, check=True)
    except subprocess.CalledProcessError as exc:
        return [
            {
                "source": "scripts/build/zephyr-fixture-leaves.sh",
                "id": "zephyr-inventory-error",
                "platform": "zephyr",
                "kind": "inventory-error",
                "lang": "",
                "rmw": "",
                "role": "",
                "dir": "",
                "build_root": "",
                "scheduler": "just zephyr build-fixtures",
                "shared_mutation": "",
                "notes": (exc.stderr or exc.stdout).strip().replace("\n", " "),
            }
        ]

    rows = []
    for line in proc.stdout.splitlines():
        if not line:
            continue
        fields = line.split("\t")
        if len(fields) < 22:
            continue
        (
            _kind,
            fixture_id,
            _target,
            board,
            lang,
            _lang_tag,
            role,
            rmw,
            src,
            _src_dir,
            _build_name,
            build_dir,
            *_rest,
        ) = fields
        rows.append(
            {
                "source": "scripts/build/zephyr-fixture-leaves.sh",
                "id": fixture_id,
                "platform": "zephyr",
                "kind": "zephyr-west",
                "lang": lang,
                "rmw": "" if rmw == "default" else rmw,
                "role": role,
                "dir": src,
                "build_root": build_dir,
                "scheduler": "scripts/build/zephyr-fixture-make-driver.sh",
                "shared_mutation": build_dir,
                "notes": f"board={board}",
            }
        )
    return rows


def hand_authored_rows():
    rows = [
        {
            "id": "qemu-smoltcp-bridge",
            "platform": "qemu-arm-baremetal",
            "kind": "hand-authored-cargo",
            "lang": "rust",
            "rmw": "",
            "role": "qemu-smoltcp-bridge",
            "dir": "packages/reference/qemu-smoltcp-bridge",
            "build_root": "packages/reference/qemu-smoltcp-bridge/target",
            "scheduler": "just qemu build-fixtures",
            "shared_mutation": "packages/reference/qemu-smoltcp-bridge/target",
            "notes": "not covered by examples/fixtures.toml",
        },
        {
            "id": "native-rust-cyclonedds-talker",
            "platform": "native",
            "kind": "hand-authored-cargo",
            "lang": "rust",
            "rmw": "cyclonedds",
            "role": "talker",
            "dir": "examples/native/rust/talker",
            "build_root": "examples/native/rust/talker/target-cyclonedds",
            "scheduler": "scripts/build/fixture-make-driver.sh native-cyclonedds-rust",
            "shared_mutation": "examples/native/rust/talker/generated; examples/native/rust/talker/target-cyclonedds",
            "notes": "pure-cargo Cyclone lane outside manifest",
        },
        {
            "id": "native-rust-cyclonedds-listener",
            "platform": "native",
            "kind": "hand-authored-cargo",
            "lang": "rust",
            "rmw": "cyclonedds",
            "role": "listener",
            "dir": "examples/native/rust/listener",
            "build_root": "examples/native/rust/listener/target-cyclonedds",
            "scheduler": "scripts/build/fixture-make-driver.sh native-cyclonedds-rust",
            "shared_mutation": "examples/native/rust/listener/generated; examples/native/rust/listener/target-cyclonedds",
            "notes": "pure-cargo Cyclone lane outside manifest",
        },
        {
            "id": "threadx-riscv64-rust-talker-cyclonedds",
            "platform": "threadx-riscv64",
            "kind": "hand-authored-cmake",
            "lang": "rust",
            "rmw": "cyclonedds",
            "role": "talker",
            "dir": "examples/qemu-riscv64-threadx/rust/talker",
            "build_root": "examples/qemu-riscv64-threadx/rust/talker/build-cyclonedds",
            "scheduler": "just threadx_riscv64 build-fixture-extras",
            "shared_mutation": "examples/qemu-riscv64-threadx/rust/talker/build-cyclonedds",
            "notes": "gated helper build_threadx_cmake_rmw",
        },
        {
            "id": "esp32-qemu-flash-images",
            "platform": "qemu-esp32-baremetal",
            "kind": "postprocess",
            "lang": "rust",
            "rmw": "zenoh",
            "role": "flash-image",
            "dir": "examples/qemu-esp32-baremetal/rust",
            "build_root": "build/esp32-qemu",
            "scheduler": "just esp32 build-qemu",
            "shared_mutation": "build/esp32-qemu",
            "notes": "espflash packs talker/listener ELFs after manifest cargo leaves",
        },
        {
            "id": "esp32-qemu-logging-smoke-flash-image",
            "platform": "qemu-esp32-baremetal",
            "kind": "postprocess",
            "lang": "rust",
            "rmw": "zenoh",
            "role": "logging-smoke",
            "dir": "packages/testing/nros-tests/bins/logging-smoke-esp32-qemu",
            "build_root": "packages/testing/nros-tests/bins/logging-smoke-esp32-qemu/target",
            "scheduler": "just esp32 build-logging-smoke",
            "shared_mutation": "logging-smoke ELF .bin sibling",
            "notes": "espflash packs binary after manifest cargo leaf",
        },
        {
            "id": "esp-idf-smoke",
            "platform": "esp_idf",
            "kind": "hand-authored-idf",
            "lang": "c",
            "rmw": "",
            "role": "smoke",
            "dir": "tests/esp-idf-smoke",
            "build_root": "tests/esp-idf-smoke/build",
            "scheduler": "just esp_idf build-fixtures",
            "shared_mutation": "tests/esp-idf-smoke/build; tests/esp-idf-smoke/sdkconfig",
            "notes": "idf.py set-target/build path",
        },
    ]
    for row in rows:
        row["source"] = "just/*.just"
        yield row


def collect(repo_root, args):
    manifest = load_toml(repo_root / args.manifest)
    rows = []
    rows.extend(manifest_fixture_rows(manifest))
    rows.extend(workspace_rows(manifest))
    rows.extend(zephyr_rows(repo_root, args.include_zephyr))
    rows.extend(hand_authored_rows())

    if args.platform:
        rows = [row for row in rows if row["platform"] == args.platform]
    if args.lang:
        rows = [row for row in rows if row["lang"] == args.lang]
    if args.source:
        rows = [row for row in rows if row["source"] == args.source]
    return sorted(rows, key=lambda r: (r["platform"], r["kind"], r["lang"], r["rmw"], r["id"]))


def write_tsv(rows, out):
    writer = csv.DictWriter(out, fieldnames=FIELDS, delimiter="\t", lineterminator="\n")
    writer.writeheader()
    for row in rows:
        writer.writerow({field: row.get(field, "") for field in FIELDS})


def write_summary(rows, out):
    counts = {}
    for row in rows:
        key = (row["platform"], row["kind"])
        counts[key] = counts.get(key, 0) + 1
    out.write("platform\tkind\tcount\n")
    for (platform, kind), count in sorted(counts.items()):
        out.write(f"{platform}\t{kind}\t{count}\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", default="examples/fixtures.toml")
    parser.add_argument("--platform")
    parser.add_argument("--lang")
    parser.add_argument("--source")
    parser.add_argument("--summary", action="store_true")
    parser.add_argument(
        "--no-zephyr",
        dest="include_zephyr",
        action="store_false",
        help="omit Zephyr's dynamic leaf emitter",
    )
    parser.set_defaults(include_zephyr=True)
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[2]
    rows = collect(repo_root, args)
    if args.summary:
        write_summary(rows, sys.stdout)
    else:
        write_tsv(rows, sys.stdout)


if __name__ == "__main__":
    try:
        main()
    except BrokenPipeError:
        try:
            sys.stdout.close()
        finally:
            os._exit(1)
