---
id: 2358
title: "`nros setup board <zephyr-board> --zephyr-workspace` tells a Rust user to import a west fragment that does not exist, by an absolute path west refuses"
status: open
type: bug
severity: medium
area: [cli, setup, zephyr, rust]
related: [phase-548]
found: 2026-11-07
---

**Home: [phase-548](../roadmap/phase-548-three-languages-every-board.md)**
(found by W7, the Rust cell on `nucleo-f767zi-zephyr`).

## What

Measured in the `ros2` box (Humble, Zephyr 3.7) while measuring phase-548
W7. The west workspace is the one the Nucleo starter writes
(`book/src/starters/nucleo-f767zi.md`): its manifest allowlists `cmsis`,
`picolibc`, `mbedtls`, `tinycrypt`, `mcuboot`, `net-tools` and `hal_stm32`,
and no `zephyr-lang-rust` (dropped on purpose by 1a7df2c9d8, since a C
talker does not need it). That is right for the C tab. A Rust user following
the same page gets no working way to add the module:

1. `nros setup board nucleo-f767zi-zephyr --zephyr-workspace "$PWD"` prints

   ```text
   (d) zephyr-lang-rust: MISSING at <ws>/modules/lang/rust.
       Add the board's import fragment to your west manifest, then `west update`:
         manifest:
           self:
             import:
               - file: <checkout>/packages/boards/nros-board-nucleo-f767zi-zephyr/west-downstream.yml
   ```

   That file does not exist. `git ls-files | grep west-downstream` finds one
   fragment in the tree,
   `packages/boards/nros-board-zephyr/boards/fvp-aemv8r-smp/west-downstream.yml`.
   `ensure_lang_rust_module` in
   `packages/cli/nros-cli-core/src/cmd/setup.rs` joins `west-downstream.yml`
   onto the board's crate dir without checking it is there, so the hint names
   a missing file for every Zephyr board but that one.
2. Even with a fragment that exists, the hint's shape is refused. west 1.x
   says `Malformed manifest file ... "self: import: {'file': '<abs path>'}"
   contains an absolute path`. A `self: import: file:` is relative to the
   manifest repository.
3. Without the module, `nros build` fails late and in Zephyr's words:
   `prj.conf:10: warning: attempt to assign the value 'y' to the undefined
   symbol RUST` then `error: Aborting due to Kconfig warnings`. Nothing names
   `zephyr-lang-rust`, although `nros setup board` already knew it was
   missing.

What worked: a plain `projects:` entry in the user's own manifest, pinned to
the revision nano-ros's `west.yml` uses, then
`west update --narrow -o=--depth=1 zephyr-lang-rust`:

```yaml
    - name: zephyr-lang-rust
      remote: zephyrproject-rtos
      revision: 404fcefdbab0f2711cd342a1fab9d2eda52a1ba9
      path: modules/lang/rust
```

After that, `nros setup board` printed `(d) zephyr-lang-rust: present` and
the Rust image configured.

## Fix direction

- The hint prints the `projects:` entry above, with the revision read from
  the one place nano-ros pins it (the index or `west.yml`), instead of a
  fragment path. That works for any manifest, with no file to ship per board.
- Or ship the fragment where the hint looks, and print a relative-import
  recipe west accepts. This costs one file per board and still leaves the
  user to copy it into their manifest repository.
- `nros build` refuses a Rust Zephyr image whose workspace has no
  `modules/lang/rust`, before west runs, and names the module and the fix.
- The starter's Rust tab (phase-548 W8) shows the line.

## Done when

- On a west workspace without the module, `nros setup board <b>
  --zephyr-workspace` prints a recipe that, pasted into the manifest as
  printed, makes `west update` fetch `zephyr-lang-rust`. A test asserts the
  recipe names no missing file and no absolute `self: import:` path.
- `nros build` of a Rust Zephyr image on such a workspace stops with a
  message naming `zephyr-lang-rust`, not a Kconfig warning.
