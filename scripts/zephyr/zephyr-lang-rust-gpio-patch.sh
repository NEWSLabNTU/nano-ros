#!/usr/bin/env bash
# scripts/zephyr/zephyr-lang-rust-gpio-patch.sh
#
# Issue 0432 — make `zephyr-lang-rust` compile for a board whose devicetree has
# gpio nodes. Until this lands, the `zephyr` crate does not build for ANY real
# board; native_sim has no gpio nodes, which is the only reason it went unseen.
#
# TWO defects, both upstream at the pinned `404fcef`, both in the DT generator
# rather than in nano-ros:
#
# 1. ARITY. `crate::device::gpio::GpioPin::new` takes `(…, pin: u32, dt_flags:
#    u32)` — it assumes a TWO-cell gpio. Not every controller is: Zephyr's own
#    `arm,mps2-fpgaio-gpio` on mps2_an385 declares `#gpio-cells = <1>`, so
#    `gpios = <&gpio_led0 0>` is complete and correct and there is no flags cell
#    to forward. `augment.rs` passes exactly the cells it finds, so the emitted
#    call is one argument short and the generated code does not compile:
#
#        error[E0061]: this function takes 6 arguments but 5 arguments were supplied
#          GpioPin::new(&UNIQUE, &STATIC, device, device_static, 0u32)
#
#    Four occurrences on mps2_an385 — measured, not predicted. A missing flags
#    cell means "no flags", which is what 0 encodes, and is what the C side does
#    with `DT_PHA_BY_IDX_OR(..., flags, 0)`. So pad to the constructor's arity.
#
# 2. MISSING `cfg:`. The `gpio-keys` augment in `dt-rust.yaml` carries no `cfg:`
#    key, while its two siblings (`gpio-controller`, `gpio-leds`) both carry
#    `cfg: CONFIG_GPIO`. So with `CONFIG_GPIO=n` its instances are still emitted
#    while the `raw` bindings they reference disappear with the driver — 14
#    errors instead of 4. That is why `CONFIG_GPIO=n` does not dodge defect 1.
#
# Both are upstreamable and neither is nano-ros-specific; see
# `zephyr/patches.yml` (module `zephyr-lang-rust`) for the `west patch` delivery
# of the same change to downstream BYO workspaces.
#
# Idempotent: each hunk is guarded by a grep for its own marker.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NANO_ROS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
IN_TREE_WORKSPACE="$NANO_ROS_ROOT/zephyr-workspace"
LEGACY_WORKSPACE="$(cd "$NANO_ROS_ROOT/.." && pwd)/nano-ros-workspace"

if [ -n "${1:-}" ]; then
    WORKSPACE="$1"
elif [ -n "${NROS_ZEPHYR_WORKSPACE:-}" ]; then
    WORKSPACE="$NROS_ZEPHYR_WORKSPACE"
elif [ -d "$IN_TREE_WORKSPACE/zephyr" ]; then
    WORKSPACE="$IN_TREE_WORKSPACE"
else
    WORKSPACE="$LEGACY_WORKSPACE"
fi

RUST_MODULE="$WORKSPACE/modules/lang/rust"
AUGMENT="$RUST_MODULE/zephyr-build/src/devicetree/augment.rs"
DT_YAML="$RUST_MODULE/dt-rust.yaml"

# Absent module is NOT an error: a C/C++-only workspace legitimately has none,
# and the setup recipes call every patch script unconditionally.
if [ ! -d "$RUST_MODULE" ]; then
    echo "zephyr-lang-rust-gpio-patch: no rust module at $RUST_MODULE — skipping"
    exit 0
fi
for f in "$AUGMENT" "$DT_YAML"; do
    if [ ! -f "$f" ]; then
        echo "ERROR: $f missing (unexpected layout for zephyr-lang-rust)" >&2
        exit 1
    fi
done

changed=0

# --- 1. pad `gpios` cells to the constructor's arity -------------------------
if ! grep -q "nano-ros: pad gpios to the constructor arity" "$AUGMENT"; then
    python3 - "$AUGMENT" <<'PY'
import sys, pathlib
p = pathlib.Path(sys.argv[1])
t = p.read_text()
old = "                let args: Vec<u32> = words[1..].iter().map(|n| n.as_number().unwrap()).collect();"
new = """                // nano-ros: pad gpios to the constructor arity (issue 0432).
                //
                // `GpioPin::new` takes `(…, pin, dt_flags)` — a TWO-cell gpio —
                // but a controller may declare `#gpio-cells = <1>` (Zephyr's own
                // `arm,mps2-fpgaio-gpio` does), so the devicetree correctly holds
                // one cell and the emitted call is an argument short:
                //
                //   error[E0061]: this function takes 6 arguments but 5 were supplied
                //
                // A missing flags cell means "no flags" = 0, which is what the C
                // side does via `DT_PHA_BY_IDX_OR(..., flags, 0)`.
                let mut args: Vec<u32> = words[1..].iter().map(|n| n.as_number().unwrap()).collect();
                if pname == "gpios" && args.len() < 2 {
                    args.resize(2, 0);
                }"""
assert old in t, "augment.rs: expected args line not found (upstream moved?)"
p.write_text(t.replace(old, new, 1))
PY
    changed=1
    echo "  [patched] $AUGMENT — gpios cell padding"
fi

# --- 2. gpio-keys gains the `cfg:` its siblings have -------------------------
if ! grep -q "nano-ros: gpio-keys needs the same cfg" "$DT_YAML"; then
    python3 - "$DT_YAML" <<'PY'
import sys, pathlib
p = pathlib.Path(sys.argv[1])
t = p.read_text()
old = "- name: gpio-keys\n  rules:"
new = ("- name: gpio-keys\n"
       "  # nano-ros: gpio-keys needs the same cfg as its siblings (issue 0432).\n"
       "  # Without it the instances are emitted even when CONFIG_GPIO=n, while\n"
       "  # the `raw` bindings they reference disappear with the driver.\n"
       "  cfg: CONFIG_GPIO\n"
       "  rules:")
assert old in t, "dt-rust.yaml: gpio-keys augment not in the expected shape"
p.write_text(t.replace(old, new, 1))
PY
    changed=1
    echo "  [patched] $DT_YAML — gpio-keys cfg: CONFIG_GPIO"
fi

if [ "$changed" -eq 0 ]; then
    echo "zephyr-lang-rust-gpio-patch: already applied"
else
    echo "zephyr-lang-rust-gpio-patch: done"
fi
