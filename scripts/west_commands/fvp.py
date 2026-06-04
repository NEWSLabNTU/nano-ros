# scripts/west_commands/fvp.py
#
# Phase 215.D.1 — `west fvp run` extension.
#
# A board's `cmake/board/nano-ros-board-<board>.cmake` (Phase 215.B/.C) declares
# `NROS_BOARD_RUNNER=armfvp` in its CMakeCache.txt for FVP-targeted boards.
# `west fvp run` reads that cache key, resolves the ARM Fast Models binary via
# `scripts/zephyr/resolve-fvp-bin.sh`, exports `ARMFVP_BIN_PATH`, then
# delegates to `west build -d <build_dir> -t run` so Zephyr's stock
# `cmake/emu/armfvp.cmake` target launches the simulator.
#
# Discovery: this command is registered via `scripts/west-commands.yml`, which
# is in turn referenced from `zephyr/module.yml` (`west-commands:` key). Any
# west workspace that lists nano-ros as a project picks it up automatically.
#
# License-gated: the FVP itself requires accepting the Arm EULA. This script
# does NOT download anything; the resolver only finds an already-installed
# binary.

import os
import subprocess
import sys

from west.commands import WestCommand


class FvpRun(WestCommand):
    def __init__(self):
        super().__init__(
            'fvp',
            'run a nano-ros FVP board in the Arm Fast Models simulator',
            'Build + launch a nano-ros Zephyr application on an Arm Fast '
            'Models FVP target. Reads NROS_BOARD_RUNNER from the build '
            'directory CMakeCache.txt, resolves the FVP binary, exports '
            'ARMFVP_BIN_PATH, then delegates to `west build -t run`.',
        )

    def do_add_parser(self, parser_adder):
        parser = parser_adder.add_parser(self.name, help=self.help)
        sub = parser.add_subparsers(dest='action', required=True)
        run_p = sub.add_parser('run', help='build + launch FVP')
        run_p.add_argument(
            '-d', '--build-dir', default='build/',
            help='Zephyr build directory (default: build/)',
        )
        return parser

    def do_run(self, args, unknown_args):
        if args.action != 'run':
            self.die(f"unknown action: {args.action}")

        cache = self._read_cache(args.build_dir)
        runner = cache.get('NROS_BOARD_RUNNER')
        if runner != 'armfvp':
            self.die(
                f"board NROS_BOARD_RUNNER='{runner}' is not 'armfvp'; "
                f"`west fvp run` only supports FVP boards. "
                f"Use `west build -t run` for other runners."
            )

        repo_root = self._find_nros_root()
        resolver = os.path.join(repo_root, 'scripts/zephyr/resolve-fvp-bin.sh')
        if not os.path.exists(resolver):
            self.die(f"resolver missing: {resolver}")

        try:
            fvp_bin = subprocess.check_output(
                ['bash', resolver], env=os.environ.copy(),
            ).decode().strip()
        except subprocess.CalledProcessError as e:
            self.die(f"resolve-fvp-bin.sh failed (exit {e.returncode}); "
                     f"see stderr above for setup hints")

        env = os.environ.copy()
        env['ARMFVP_BIN_PATH'] = fvp_bin
        self.inf(f"FVP binary dir: {fvp_bin}")
        self.inf(f"Launching `west build -d {args.build_dir} -t run` ...")
        os.execvpe(
            'west', ['west', 'build', '-d', args.build_dir, '-t', 'run'], env,
        )

    def _read_cache(self, build_dir):
        """Parse `<build_dir>/CMakeCache.txt`. Returns a dict of cache keys."""
        path = os.path.join(build_dir, 'CMakeCache.txt')
        if not os.path.isfile(path):
            self.die(f"CMakeCache.txt not found in build dir '{build_dir}'; "
                     f"run `west build` first")
        out = {}
        with open(path, 'r') as fh:
            for line in fh:
                line = line.strip()
                if not line or line.startswith('//') or line.startswith('#'):
                    continue
                # Format: NAME:TYPE=VALUE
                eq = line.find('=')
                colon = line.find(':')
                if eq < 0 or colon < 0 or colon > eq:
                    continue
                name = line[:colon]
                value = line[eq + 1:]
                out[name] = value
        return out

    def _find_nros_root(self):
        """Walk up from this script until `packages/boards/` is found."""
        here = os.path.dirname(os.path.abspath(__file__))
        cur = here
        while True:
            if os.path.isdir(os.path.join(cur, 'packages', 'boards')):
                return cur
            parent = os.path.dirname(cur)
            if parent == cur:
                self.die(
                    f"could not locate nano-ros root (no packages/boards/ "
                    f"ancestor of {here})"
                )
            cur = parent
