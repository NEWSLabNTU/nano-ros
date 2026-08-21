# Worked Example — STM32F4 Out of Tree

This chapter takes one real board all the way through
[the customization ladder](./overview.md): an **STM32F429/F407** with Ethernet,
built and maintained entirely outside the nano-ros repository.

It exists because that is the honest arrangement for this board. nano-ros
carried `nros-board-stm32f4` and `nros-board-rtic-stm32f4` in-tree for a long
time, and **no CI lane ever booted either of them** — the hardware is not in the
test rack and QEMU does not model the STM32 MAC. An in-tree crate reads as a
support promise, so the two crates left the supported matrix (RFC-0064,
phase-337 W7.a) and became this page instead. The hardware works exactly as well
as it did; what changed is who verifies it, and that is now you.

If you want the general shape rather than this specific board, read
[Custom Board Package](./custom-board.md) and
[Vendor Overlay Board Crate](./vendor-overlay.md) first — this chapter assumes
them and only shows the STM32F4-specific decisions.

## What this board actually needs

Four facts drive everything below. They came from the retired crate's own
descriptor:

| Fact | Value |
|---|---|
| Target | `thumbv7em-none-eabihf` |
| Link | `-C link-arg=-Tlink.x`, plus a memory map for the chip |
| Net stack ownership | **nanoros-owned** — nano-ros supplies both the MAC driver and the IP stack |
| Transports | Ethernet (STM32 MAC + smoltcp) or serial (USART) |

The third row is the expensive one, and it is why this board costs more than a
Zephyr or NuttX port. On an RTOS-owned board the host ecosystem already brings
up the NIC and the IP stack, and nano-ros only asks it for a socket. Here,
nothing else brings up the NIC — so the driver and the stack are part of your
board, not part of the platform. Budget for that before anything else:
RFC-0064 measures a MAC driver at roughly 500 lines when one does not already
exist.

## Which rung you are on

The [ladder](./overview.md) has three rungs, and this board sits on different
ones depending on what you are changing.

| What you want | Rung | What you write |
|---|---|---|
| A different chip in the F4 family (F407 vs F429) | 1 — declare | one `chip` value + its memory map |
| A different PHY, same MAC | 2 — hook | the board's Ethernet init function |
| A proprietary link instead of Ethernet | 1 + 2 | `link.custom` + the transport vtable |
| An IP stack that is not smoltcp | 3 — escape | the platform ABI's net functions directly |

Rung 3 always exists, and it carries a rule worth stating plainly: **if you ship
an artifact by hand, the generator does not overwrite it and the drift gate does
not flag it.** You can adopt generated glue one file at a time and keep the
files you have reason to own.

## The shape of the crate

Your board crate depends on nano-ros; nano-ros does not depend on it. That is
the whole point of taking it out of tree, and it is what lets you pin a
nano-ros release and upgrade on your own schedule.

```
my-stm32f4-board/
  Cargo.toml          # depends on nros + the bare-metal platform
  nros-board.toml      # the descriptor the `nros` CLI reads
  build.rs             # emits the linker script / memory map
  memory.x             # FLASH + RAM origin and length for your chip
  src/lib.rs           # the board ZST, its trait impls, and `run()`
```

Compose your `Config` on `nros_board_common::BaseConfig` rather than declaring
the network fields again — it carries `{mac, ip, netmask, gateway,
zenoh_locator, domain_id}` and the `DeployOverlay` merge, and keeps your board
aligned with every other one when those fields change. Board-specific settings
(a UART base address, a PHY address) stay on your own struct beside it:

```rust
pub struct Config {
    pub base: nros_board_common::BaseConfig,
    pub phy_addr: u8,
}
```

`BaseConfig` stores the **netmask**; `prefix()` and `with_prefix()` convert if
you think in CIDR.

## The descriptor

`nros-board.toml` is what makes the `nros` CLI able to generate an entry for
your board. The retired crate's descriptor is a working template — the fields
that matter for a bare-metal Cortex-M board with its own stack:

```toml
[[board]]
names               = ["my-stm32f4"]
platform            = "stm32"
toolchain           = "stable"
platform_feature    = "platform-bare-metal"
link_kind           = "none"
entry_kind          = "board-run"
supported_netstacks = ["smoltcp"]
chip                = "stm32f429"
board_crate         = "my-stm32f4-board"

# entry_kind = "board-run" needs the matching [board.entry] block —
# copy the one from packages/boards/nros-board-mps2-an385/nros-board.toml
# and swap the crate name.
[board.entry]
crate_name = "my_stm32f4_board"
signature  = "#[my_stm32f4_board::entry]\nfn main() -> !"

cargo_config = '''
[build]
target = "thumbv7em-none-eabihf"

[target.thumbv7em-none-eabihf]
rustflags = ["-C", "link-arg=-Tlink.x"]
'''
```

`supported_netstacks` names who brings up the link (here your crate's
smoltcp glue — nothing outside it). Beware unknown keys: the descriptor
parser ignores what it does not know rather than erroring, so a typo'd
key silently does nothing.

## What you take on

Be clear-eyed about the trade, because it is the reason this page exists rather
than a crate:

- **You own verification.** No nano-ros lane boots this board. A nano-ros
  release being green says nothing about your board; only your hardware does.
- **You own the driver.** A MAC or PHY change is yours to make and yours to
  test.
- **You gain release control.** Your board no longer moves when nano-ros
  refactors its board tree — you upgrade when you choose, and a breaking change
  in the seam is visible as a version bump instead of a surprise.

The seam you depend on is deliberately narrow: the platform ABI is 92 C
functions, and no rung of the ladder requires nano-ros to know what your stack
*is*. That is what makes an out-of-tree board sustainable rather than a fork.

## If you want it back in tree

The bar is a **witness**: a lane that boots the board and asserts real delivery.
That means hardware in the test rack or a QEMU machine that models the MAC. A
board with no Runtime cell cannot be tier 1 or 2 —
`scripts/check-board-tiers.py` enforces exactly that, which is what moved these
crates out in the first place.
