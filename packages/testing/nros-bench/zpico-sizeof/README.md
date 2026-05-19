# zpico-sizeof — Phase 64.2 memory benchmark

Two standalone `gcc`-compiled probes that print `sizeof()` of every zpico /
zenoh-pico entity struct. Used to populate the "Measured Memory Footprint"
section of `docs/guides/embedded-tuning.md`.

## Usage

```bash
cd /home/aeon/repos/nano-ros

# Public API handles + zpico.c entry-table structs.
gcc -I packages/zpico/zpico-sys/zenoh-pico/include \
    -I packages/zpico/zpico-sys/c/zpico \
    -I packages/zpico/zpico-sys/c/platform \
    -DZ_FEATURE_MULTI_THREAD=1 -DZENOH_LINUX -DZ_FEATURE_LINK_TCP=1 \
    -o /tmp/sizeof_probe packages/testing/nros-bench/zpico-sizeof/sizeof_probe.c
/tmp/sizeof_probe

# zenoh-pico internal struct sizes (heap-allocated per entity / session).
gcc -I packages/zpico/zpico-sys/zenoh-pico/include \
    -I packages/zpico/zpico-sys/c/zpico \
    -I packages/zpico/zpico-sys/c/platform \
    -DZ_FEATURE_MULTI_THREAD=1 -DZENOH_LINUX -DZ_FEATURE_LINK_TCP=1 \
    -o /tmp/internal_probe packages/testing/nros-bench/zpico-sizeof/internal_probe.c
/tmp/internal_probe
```

Re-run after any zenoh-pico submodule bump or `_z_*_t` field change to refresh
the numbers in the tuning guide. Output is stable across hosts (sizes depend
only on the C ABI + feature flags + word size).

## Scope

Static-allocation cost only — RX/TX batch buffer sizing (`ZPICO_BATCH_*`,
`ZPICO_FRAG_MAX_SIZE`) is per-platform and documented separately under
"Buffer Sizes" in the tuning guide. Heap-allocator usage on POSIX builds is
out of scope (use `valgrind --tool=massif` for that).
