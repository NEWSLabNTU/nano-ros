# nros-px4-nuttx-compile

A snapshot of how PX4 compiles a NuttX module's C++, for one gate:
`just check px4-uorb-nuttx-compile`
(`scripts/check/check-px4-uorb-nuttx-compile.py`). The gate compiles
nano-ros's uORB backend and its PX4 push-wake glue with these flags against
these headers. CI has no PX4 tree, so the gate needs this copy.
It exists because of
[issue 2419](../../../docs/issues/archived/2419-px4-uorb-glue-no-nuttx-compile-gate.md).

Nothing here is authored. Every file is generated or copied from upstream.

| file | what |
|---|---|
| `snapshot.toml` | provenance (`[px4]`: tag, commit, board config, NuttX and apps commits, compiler), the compile options and include roots (`[flags]`), licence and archive hashes |
| `headers.tar.xz` | the 173 PX4 / NuttX headers the backend reaches, plus both NuttX `include/cxx` directories, verbatim |
| `LICENSE-PX4` | PX4-Autopilot's `LICENSE` (BSD-3-Clause), at the pinned commit |
| `LICENSE-NuttX`, `NOTICE-NuttX` | NuttX's `LICENSE` and `NOTICE` (Apache-2.0), at the commit that PX4 pins |

## Provenance

`snapshot.toml` `[px4]` is the record: PX4 `v1.17.0`, commit
`d6f12ad1c4f70ad3230afd7d86e971421e02fef4` (the `[source.px4-autopilot] ref` in
`nros-sdk-index.toml`; the gate fails if the two disagree), board config
`px4_fmu-v6x_default`, NuttX commit `fb2fadf6f599c1406f052db013efd00a2518e72c`.
The module flags are the set that 214 of the 543 stock `px4_add_module()` C++
TUs share in that build's `compile_commands.json`.

## Regenerating

Do this when the PX4 pin moves. You need the pinned `arm-none-eabi-gcc` and
PX4's Python requirements.

```sh
git clone https://github.com/PX4/PX4-Autopilot.git px4 && cd px4
git checkout d6f12ad1c4f70ad3230afd7d86e971421e02fef4
git submodule update --init platforms/nuttx/NuttX/nuttx platforms/nuttx/NuttX/apps
cmake -S . -B ../px4-build -DCONFIG=px4_fmu-v6x_default -GNinja
ninja -C ../px4-build nuttx_context uORB/topics/uORBTopics.hpp
cd <nano-ros>
scripts/gen-px4-nuttx-compile-snapshot.py --px4 <px4> --build <px4-build>
```

The configure writes NuttX's context into the PX4 tree, so use a tree that
belongs to you. The output is byte-reproducible: the same inputs give the same
archive.

## Why a tarball

If the headers were checked in as files, other gates would read them as
nano-ros source: `weak-symbols`, `no-unbounded-condvar-wait`,
`c-array-pool-floors` and `comment-names` all flagged them, and clang-format
would reflow them. The gate unpacks the archive into a temporary directory
and checks the archive's sha256 against `snapshot.toml`.
