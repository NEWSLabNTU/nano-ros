# Runner and command packages

<!-- type: how-to · level: 4 -->

**Needs first:** [Organize a larger workspace](../guide/build/organize-a-larger-workspace.md)

You want `nros` to do something it does not ship: run an image on a launcher
or flasher nano-ros has no runner for, or add a verb of your own, such as a
generator your team runs on every workspace. Both are ordinary packages in
your workspace, built for the host by `nros build`, with no change to
nano-ros. By the end of this page, `nros run` drives your runner, or
`nros <verb>` runs your command.

## Before you start

- You have a workspace with a `src/` directory that `nros build` builds.
- The Rust scaffolds build with the host's Rust toolchain. A C++ package
  speaks the same protocol through its own CMake executable: pass
  `--lang cpp` to `nros new`.

## Runner packages

A runner teaches `nros run`, `nros flash` and `nros monitor` to drive a
launcher, a flasher or a console: an emulator nano-ros does not know, a
vendor flash tool, a remote board farm.

1. Scaffold the runner, from the workspace root:

   ```sh
   nros new my_runner --kind runner
   ```

   ```text
   nros new: scaffolded runner package `my_runner` at src/my_runner
   ```

   The package holds:

   - `package.xml`, whose `<nano_ros_provides kind="runner" name="my_runner"/>`
     announces the runner;
   - `nros-runner.toml`, what `nros` reads when this runner is selected:
     `protocol`, the `verbs` it serves (`run`, `flash`, `monitor`), what it
     `needs` from the build (`elf`, …) and the `bin` the build produces;
   - `src/main.rs`, a stub on the `nros-extension` crate that answers
     `describe` and runs the plan's `elf` as a host process, as the built-in
     native runner does. A test in it holds `describe` equal to
     `nros-runner.toml`.

2. Replace `run` in `src/main.rs` with what your target needs. `nros run`
   resolves a plan and executes your binary as `<bin> run --plan <plan.json>`;
   `nros-extension` reads the plan, checks the verb and hands you its
   artifacts and arguments.

3. Build it:

   ```sh
   nros build --packages-select my_runner
   ```

4. Select it for an image in the bringup's `system.toml`, then run the image:

   ```toml
   [image.my_image.run]
   method = "my_runner"
   ```

   ```sh
   nros run my_image
   ```

   A runner that serves `flash` is selected the same way, with
   `[image.<id>.flash] method`. A board can name it once for every image, in
   its descriptor's `[board.flash] method`: which method words exist is the
   answer of the installed runners, so `nros` checks only their shape.

## Command packages

A command adds a verb to `nros`. A verb `nros` does not know is looked up as
an announced command package.

1. Scaffold the command, from the workspace root:

   ```sh
   nros new my_cmd --kind command
   ```

   ```text
   nros new: scaffolded command package `my_cmd` at src/my_cmd
   ```

   The package holds a `package.xml` announcing
   `<nano_ros_provides kind="command" name="my_cmd"/>`, an
   `nros-command.toml` with the command's `protocol`, `summary`, `tier` and
   `bin`, and a `src/main.rs` that prints its arguments and environment.
   `tier = "user"` lists the verb in `nros help --all`.

2. Write the command in `src/main.rs`. `nros my_cmd <args>` runs your binary
   with the arguments unchanged, and with three variables set:
   `NROS_WS_ROOT` (the workspace), `NROS_TOOLCHAIN` (the toolchain's version)
   and `NROS_CLI` (the running `nros`). Ask `"$NROS_CLI"` for facts rather
   than deriving them again. Your exit status is `nros`'s.

3. Build it, and run it:

   ```sh
   nros build --packages-select my_cmd
   nros my_cmd hello world
   ```

   ```text
   nros my_cmd: hello world
     workspace /home/me/my_ws
     toolchain 0.6.0 (/home/me/.nros/bin/nros)
   ```

4. Check that `nros` lists it. Put a real sentence in `summary` first:

   ```sh
   nros help --all
   ```

   ```text
   Extension commands (from packages):
     nros my_cmd  TODO: say what `nros my_cmd` does (package my_cmd)
   ```

   A package that announces a built-in verb, such as `build`, is refused.

## Example

nano-ros's own extension commands are command packages, and their help is in
the [CLI reference](../reference/cli.md):

- `nros px4 msgs`, from the package `nros_px4_tools`, generates `px4_msgs`
  bindings from a PX4 message tree ([PX4](../guide/platforms/px4.md)).
- `nros router config`, from the package `nros_zenoh_tools`, prints the zenoh
  router configuration a bringup's router runs
  ([Configure zenoh](../guide/rmw/configure-zenoh.md)).

## If it doesn't work

- **`nros my_cmd` reports that the command is not built.** The package is
  announced but its binary is missing. Run
  `nros build --packages-select my_cmd`.
- **`nros run` cannot use the runner.** The image's
  `[image.<id>.run] method` names a word no built runner announces, or the
  runner's `nros-runner.toml` does not list the verb (`run`, `flash`,
  `monitor`).
- **The runner fails on a missing artifact.** The plan carries only what
  `needs` lists. Add the artifact (`elf`, `bin`, `hex`, …) to `needs`.

## See also

- [CLI reference](../reference/cli.md): every built-in verb, so you can pick a
  name that does not collide.
- [Add a board](add-a-board.md): `[board.run]` and `[board.flash]`, which
  name a runner for every image of a board.

## Design background

- [RFC-0119: CLI extension packages](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0119-cli-extension-packages.md): runner and command packages
- [RFC-0118: target execution model](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0118-target-execution-model.md): the plan a runner receives (D5)
