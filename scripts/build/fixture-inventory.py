#!/usr/bin/env python3
"""Emit a read-only inventory of fixture build leaves.

This is a Phase 226.A diagnostic. It combines:

* examples/fixtures.toml single-package fixture rows;
* examples/fixtures.toml workspace fixture rows;
* Zephyr's expanded leaf emitter; and
* platform preflight / SDK prerequisite rows; and
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
    if row.get("skip_build"):
        notes.append("skip-build")
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


def prerequisite_rows():
    """Return read-only prerequisite rows for fixture build recipes.

    These rows intentionally model recipe-owned setup and serial preflight
    steps without changing build execution. They make dependencies visible to
    Phase 226's future make-graph work.
    """

    rows = [
        {
            "id": "root-generate-bindings",
            "platform": "all",
            "kind": "preflight",
            "role": "generate-bindings",
            "dir": "examples",
            "build_root": "examples/**/generated",
            "scheduler": "just build-test-fixtures -> just generate-bindings",
            "shared_mutation": "examples/**/generated; packages/testing/**/generated",
            "notes": "root build-test-fixtures prerequisite; scripts/regenerate-bindings.sh",
        },
        {
            "id": "root-zenoh-posix-staticlib-fixture",
            "platform": "native",
            "kind": "preflight",
            "lang": "rust",
            "rmw": "zenoh",
            "role": "zenoh-staticlib",
            "dir": "packages/zpico/nros-rmw-zenoh-staticlib",
            "build_root": "target-zenoh-fixture-posix",
            "scheduler": "just build-test-fixtures -> just build-zenoh-posix-fixture",
            "shared_mutation": "target-zenoh-fixture-posix",
            "notes": "root fixture prerequisite for zenoh archive/header parity tests",
        },
        {
            "id": "native-rust-ws-sync-preflight",
            "platform": "native",
            "kind": "preflight",
            "lang": "rust",
            "role": "ws-sync",
            "dir": "examples/native/rust",
            "build_root": "examples/native/rust/*/generated",
            "scheduler": "just native build-fixture-rust",
            "shared_mutation": "examples/native/rust/{talker,listener,service-*,action-*,custom-transport-*}/generated; examples/native/rust/*/.cargo/config.toml",
            "notes": "serial nros ws sync plus codegen-stamp before native Rust fixture leaves",
        },
        {
            "id": "native-cmake-codegen-tool",
            "platform": "native",
            "kind": "preflight",
            "lang": "c/cpp",
            "role": "host-codegen",
            "dir": "packages/cli",
            "build_root": "packages/cli/target",
            "scheduler": "just native build-fixture-extras",
            "shared_mutation": "packages/cli/target",
            "notes": "nros CLI ws-sync probe and host nros-codegen for native C/C++ fixtures",
        },
        {
            "id": "qemu-arm-toolchain-prereq",
            "platform": "qemu-arm-baremetal",
            "kind": "sdk-prereq",
            "role": "arm-none-eabi-gcc",
            "dir": "nros-sdk-index.toml",
            "build_root": "build/qemu",
            "scheduler": "just qemu setup-qemu / just workspace apt-packages",
            "shared_mutation": "build/qemu",
            "notes": "build-fixtures skips without arm-none-eabi-gcc; patched qemu provisioned by setup-qemu for tests",
        },
        {
            "id": "stm32f4-toolchain-prereq",
            "platform": "stm32f4",
            "kind": "sdk-prereq",
            "role": "arm-none-eabi-gcc",
            "dir": "nros-sdk-index.toml",
            "build_root": "",
            "scheduler": "just stm32f4 setup",
            "shared_mutation": "$NROS_HOME/sdk/openocd; host arm-none-eabi-gcc",
            "notes": "build-fixtures skips without arm-none-eabi-gcc; setup provisions openocd for hardware flow",
        },
        {
            "id": "freertos-sdk-prereq",
            "platform": "freertos",
            "kind": "sdk-prereq",
            "role": "freertos-lwip-toolchain",
            "dir": "nros-sdk-index.toml",
            "build_root": "$FREERTOS_DIR; $LWIP_DIR",
            "scheduler": "just freertos setup",
            "shared_mutation": "$FREERTOS_DIR; $LWIP_DIR; build/qemu",
            "notes": "requires FreeRTOS kernel, lwIP, arm-none-eabi-gcc, and qemu-arm-freertos setup",
        },
        {
            "id": "freertos-rust-ws-sync-preflight",
            "platform": "freertos",
            "kind": "preflight",
            "lang": "rust",
            "role": "ws-sync",
            "dir": "examples/qemu-arm-freertos/rust",
            "build_root": "examples/qemu-arm-freertos/rust/*/generated",
            "scheduler": "just freertos build-examples",
            "shared_mutation": "examples/qemu-arm-freertos/rust/*/generated; examples/qemu-arm-freertos/rust/*/.cargo/config.toml",
            "notes": "serial nros ws sync before FreeRTOS Rust role fixtures",
        },
        {
            "id": "freertos-cmake-codegen-tool",
            "platform": "freertos",
            "kind": "preflight",
            "lang": "c/cpp",
            "role": "host-codegen",
            "dir": "packages/cli",
            "build_root": "packages/cli/target",
            "scheduler": "just freertos build-fixture-extras",
            "shared_mutation": "packages/cli/target",
            "notes": "host nros-codegen and toolchain CMake defs before FreeRTOS C/C++ leaves",
        },
        {
            "id": "nuttx-sdk-prereq",
            "platform": "nuttx",
            "kind": "sdk-prereq",
            "role": "nuttx-toolchain",
            "dir": "nros-sdk-index.toml",
            "build_root": "$NUTTX_DIR; $NUTTX_APPS_DIR",
            "scheduler": "just nuttx setup",
            "shared_mutation": "$NUTTX_DIR; $NUTTX_APPS_DIR",
            "notes": "requires NuttX sources and arm-none-eabi-gcc; build-fixtures skips when absent",
        },
        {
            "id": "nuttx-kernel-export-preflight",
            "platform": "nuttx",
            "kind": "preflight",
            "role": "kernel-export",
            "dir": "scripts/nuttx/build-nuttx.sh",
            "build_root": "$NUTTX_DIR/staging",
            "scheduler": "just nuttx build-fixtures -> just nuttx build",
            "shared_mutation": "$NUTTX_DIR/staging/libc.a; $NUTTX_DIR/include/nuttx/config.h",
            "notes": "idempotent kernel export required before Rust/C/C++ NuttX fixture builds",
        },
        {
            "id": "nuttx-rustup-warmup-preflight",
            "platform": "nuttx",
            "kind": "preflight",
            "lang": "rust",
            "role": "rustup-build-std-warmup",
            "dir": "examples/qemu-arm-nuttx/rust-toolchain.toml",
            "build_root": "$RUSTUP_HOME/toolchains",
            "scheduler": "just nuttx build-fixtures",
            "shared_mutation": "$RUSTUP_HOME/downloads; $RUSTUP_HOME/toolchains",
            "notes": "serial rust-src/cargo/rust-std install before parallel -Z build-std fixture leaves",
        },
        {
            "id": "nuttx-cmake-codegen-tool",
            "platform": "nuttx",
            "kind": "preflight",
            "lang": "c/cpp",
            "role": "host-codegen",
            "dir": "packages/cli",
            "build_root": "packages/cli/target",
            "scheduler": "just nuttx build-fixtures",
            "shared_mutation": "packages/cli/target",
            "notes": "host nros-codegen and NuttX CMake defs before C/C++ leaves",
        },
        {
            "id": "threadx-linux-sdk-prereq",
            "platform": "threadx-linux",
            "kind": "sdk-prereq",
            "role": "threadx-netx",
            "dir": "nros-sdk-index.toml",
            "build_root": "$THREADX_DIR; $NETX_DIR",
            "scheduler": "just threadx_linux setup",
            "shared_mutation": "$THREADX_DIR; $NETX_DIR",
            "notes": "requires ThreadX and NetX Duo sources; build recipes skip when absent",
        },
        {
            "id": "threadx-linux-cmake-codegen-tool",
            "platform": "threadx-linux",
            "kind": "preflight",
            "lang": "c/cpp",
            "role": "host-codegen",
            "dir": "packages/cli",
            "build_root": "packages/cli/target",
            "scheduler": "just threadx_linux build-fixture-extras",
            "shared_mutation": "packages/cli/target",
            "notes": "host nros-codegen and ThreadX/NetX CMake defs before C/C++ leaves",
        },
        {
            "id": "threadx-riscv64-sdk-prereq",
            "platform": "threadx-riscv64",
            "kind": "sdk-prereq",
            "role": "threadx-netx-riscv64-toolchain",
            "dir": "nros-sdk-index.toml",
            "build_root": "$THREADX_DIR; $NETX_DIR",
            "scheduler": "just threadx_riscv64 setup",
            "shared_mutation": "$THREADX_DIR; $NETX_DIR; host riscv64 toolchain",
            "notes": "requires ThreadX/NetX, riscv64-unknown-elf-gcc, picolibc, and qemu-system-riscv64",
        },
        {
            "id": "threadx-riscv64-cmake-codegen-tool",
            "platform": "threadx-riscv64",
            "kind": "preflight",
            "lang": "c/cpp",
            "role": "host-codegen",
            "dir": "packages/cli",
            "build_root": "packages/cli/target",
            "scheduler": "just threadx_riscv64 build-fixture-extras",
            "shared_mutation": "packages/cli/target",
            "notes": "host nros-codegen and board CMake defs before C/C++ leaves",
        },
        {
            "id": "esp32-sdk-prereq",
            "platform": "qemu-esp32-baremetal",
            "kind": "sdk-prereq",
            "role": "esp32-qemu-tooling",
            "dir": "nros-sdk-index.toml",
            "build_root": "build/esp32-qemu",
            "scheduler": "just esp32 setup",
            "shared_mutation": "build/esp32-qemu; rustup riscv32imc-unknown-none-elf target",
            "notes": "ESP32-C3 qemu/image packaging prerequisites for qemu-esp32 fixture tests",
        },
        {
            "id": "esp32-manifest-env-preflight",
            "platform": "esp32",
            "kind": "preflight",
            "lang": "rust",
            "role": "nightly-build-std-env",
            "dir": "examples/esp32/rust",
            "build_root": "$RUSTUP_HOME/toolchains",
            "scheduler": "just esp32 build-examples / just native build-examples",
            "shared_mutation": "$RUSTUP_HOME/toolchains",
            "notes": "manifest leaves require nightly RUSTUP_TOOLCHAIN plus SSID/PASSWORD env defaults",
        },
        {
            "id": "zephyr-sdk-prereq",
            "platform": "zephyr",
            "kind": "sdk-prereq",
            "role": "zephyr-workspace",
            "dir": "just/zephyr-setup.just",
            "build_root": "$ZEPHYR_WORKSPACE",
            "scheduler": "just zephyr setup",
            "shared_mutation": "$ZEPHYR_WORKSPACE; $ZEPHYR_VENV_BIN",
            "notes": "west, Zephyr workspace, venv/toolchain env, and optional line-specific setup",
        },
        {
            "id": "zephyr-workspace-preflight",
            "platform": "zephyr",
            "kind": "preflight",
            "role": "workspace-env-patches",
            "dir": "scripts/zephyr",
            "build_root": "$ZEPHYR_WORKSPACE",
            "scheduler": "just zephyr build-fixtures",
            "shared_mutation": "$ZEPHYR_WORKSPACE/zephyr; build/zephyr-cache",
            "notes": "workspace validation, ROS interface env, venv PATH, Zephyr patches, log/cache dirs",
        },
        {
            "id": "zephyr-rust-ws-sync-preflight",
            "platform": "zephyr",
            "kind": "preflight",
            "lang": "rust",
            "role": "ws-sync",
            "dir": "examples/zephyr/rust",
            "build_root": "examples/zephyr/rust/*/generated",
            "scheduler": "just zephyr build-fixtures",
            "shared_mutation": "examples/zephyr/rust/*/generated; examples/zephyr/rust/*/.cargo/config.toml; examples/zephyr/rust/*/build/.nros-ws-sync.stamp",
            "notes": "serial nros ws sync before Zephyr west fixture leaves",
        },
        {
            "id": "zephyr-cmake-codegen-tool",
            "platform": "zephyr",
            "kind": "preflight",
            "lang": "c/cpp",
            "role": "host-codegen",
            "dir": "packages/cli",
            "build_root": "packages/cli/target",
            "scheduler": "just zephyr build-fixtures",
            "shared_mutation": "packages/cli/target",
            "notes": "host nros-codegen passed to Zephyr C/C++ nros_generate_interfaces",
        },
        {
            "id": "esp-idf-sdk-prereq",
            "platform": "esp_idf",
            "kind": "sdk-prereq",
            "role": "esp-idf",
            "dir": "just/esp_idf.just",
            "build_root": "$NROS_ESP_IDF_WORKSPACE",
            "scheduler": "just esp_idf doctor/setup",
            "shared_mutation": "$NROS_ESP_IDF_WORKSPACE; tests/esp-idf-smoke/build",
            "notes": "idf.py and ESP-IDF environment required before esp-idf smoke fixture",
        },
    ]
    for row in rows:
        normalized = {field: "" for field in FIELDS}
        normalized.update(row)
        normalized["source"] = "just/*.just"
        yield normalized


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
    rows.extend(prerequisite_rows())
    rows.extend(zephyr_rows(repo_root, args.include_zephyr))
    rows.extend(hand_authored_rows())

    if args.platform:
        rows = [row for row in rows if row.get("platform") == args.platform]
    if args.lang:
        rows = [row for row in rows if row.get("lang") == args.lang]
    if args.source:
        rows = [row for row in rows if row.get("source") == args.source]
    return sorted(
        rows,
        key=lambda r: (
            r.get("platform", ""),
            r.get("kind", ""),
            r.get("lang", ""),
            r.get("rmw", ""),
            r.get("id", ""),
        ),
    )


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
