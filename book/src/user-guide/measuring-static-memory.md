# Measuring Static Memory

The [Static Pool Inventory](../reference/static-pool-inventory.md) tells you
which knobs exist and what they cost at their defaults. It cannot tell you what
*your* image costs — your knobs differ, your backend differs, and several pools
are sized by a `sizeof` that no comment can see.

For that, measure the image you built:

```sh
just mem-report path/to/your/binary
```

You get RAM broken down three ways — by symbol, by crate, and by declared pool —
plus the section totals, so you can see how much is *not* attributable to any
symbol:

```
RAM (.bss + .data), by section:  357,154 bytes
RAM attributed to symbols:       342,962 bytes
unattributed (padding, linker reservations, symbol-less data): 14,192 bytes (4.0%)
```

That last line matters. Alignment padding and linker-script reservations are
real RAM that no symbol names, so a budget built only from a list of pools will
come up short.

## Finding what to cut

The per-symbol list is sorted, so the first few lines are usually the whole
story:

```
## top 5 RAM symbols

       144,128   40.4%  nros_rmw_zenoh::shim::service::SERVICE_BUFFERS
       131,072   36.7%  nros_rmw_zenoh::shim::subscriber::LARGE_PAYLOADS
        32,768    9.2%  nros_rmw_zenoh::shim::subscriber::SMALL_PAYLOADS
```

Cross-reference each name against the [pool
inventory](../reference/static-pool-inventory.md) to find the knob that moves
it. Most of the large ones are pools with a knob you can set at build time.

Be aware that today these pools are sized by which backend you link, **not** by
what your node actually does: a publisher-only node still reserves the service
and large-payload pools in full. If your image looks far larger than the entities
you created would suggest, that is expected rather than a misconfiguration on
your side — see [issue
0827](https://github.com/nano-ros/nano-ros/blob/main/docs/issues/0827-unused-rmw-pools-dominate-static-ram.md).
Turning the corresponding knobs down is the current remedy.

## Showing a saving

Take a baseline, change something, and compare:

```sh
just mem-report my-binary --json > before.json
# ... tune a knob, rebuild ...
just mem-report my-binary --baseline before.json
```

Symbols that moved are annotated with their delta. This is also how a change to
nano-ros itself should report a memory saving: as a measured difference between
two named images, not as an estimate.

## Cross-compiled images

The tool prefers `llvm-nm`, which reads ELF files for any target; your host's
GNU `nm` is built for one target family and refuses a cross-built image with
*"File format not recognized"*.

You probably already have `llvm-nm` without knowing it: rustup ships it as part
of the `llvm-tools` component, under the toolchain's own `bin` directory rather
than on your `PATH`, and the tool looks there. If it reports that it found no
usable `nm`, run:

```sh
rustup component add llvm-tools
```
