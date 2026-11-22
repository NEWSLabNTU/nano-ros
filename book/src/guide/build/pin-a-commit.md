# Pin a commit or a checkout

<!-- type: how-to · level: 3 -->

**Needs first:** [Install options, pinning, working offline](install-options.md#pinning-a-version)

A project that tracks nano-ros `main` between releases pins a commit
instead. The `[toolchain]` table takes exactly one of three forms; `git`
and `rev` are spelled as in a cargo git dependency:

```toml
[toolchain]
version = "0.6.0-nros11"                              # a release

# or one commit of a repository
git = "https://github.com/NEWSLabNTU/nano-ros"
rev = "09219edc75c9b9a788590c6ac586a7ba45185eab"

# or a nano-ros checkout on this machine
path = "../modules/nros"
```

- `rev` is the full 40-hex commit, and it is valid only beside `git`. A
  branch name, a tag or a short sha is refused: none of them is sure to name
  the same commit tomorrow. `git rev-parse <name>^{commit}` gives the sha.
- `path` is relative to the directory that holds `nros-toolchain.toml`, as a
  cargo `path` dependency is relative to its `Cargo.toml`; an absolute path is
  taken as written. It must be the root of a nano-ros clone.

`nros pin` writes either form for you, from the project directory:

```console
nros pin --rev <40-hex commit>
nros pin --rev <40-hex commit> --git <repository url>
nros pin --path ../modules/nros
```

- `--rev` refuses a branch, a tag or a short sha, as the pin itself does.
  Without `--git`, it keeps the repository the pin already names, else the
  nano-ros one.
- `--path` takes the checkout as your shell spells it and records it
  relative to the pin file. It refuses a directory that is not the root of a
  nano-ros clone.
- Each form replaces the one the file held, so the file always names
  exactly one.
- As with a release, `--dry-run` prints what would change and writes
  nothing.
- In CI (`CI` or `GITHUB_ACTIONS` set) it refuses to write: a pin is a
  source edit, made at a keyboard and committed.

The first command run under a `git` pin fetches that one commit into
`~/.nros/toolchains/git-<rev>/` and builds it with the tree's own
`scripts/bootstrap.sh`, which takes a few minutes. Later commands use it as
they would a release. The host needs `git`, `cargo` (from
[rustup](https://rustup.rs)), a C compiler and Python 3 with headers; unlike
`bootstrap.sh`, the pin does not install rustup for you. The build takes its
final name only after it finished and its source stamp checked, so an
interrupted or failed build leaves nothing behind and the next command tries
again. A different `rev` is a different directory, so moving the pin never
rebuilds a toolchain in place.

Nothing writes into that store toolchain, so its stale-CLI check passes on
its own and `NROS_SKIP_STALE_CHECK` is not needed.

Under a `path` pin, `nros` runs that checkout's own CLI,
`packages/cli/target/release/nros`; build it there first with
`scripts/bootstrap.sh --no-prompt`. Its stale-CLI check works as in any
checkout and refuses once a tracked CLI source is edited after the build.

`nros toolchain gc` and `nros toolchain remove` keep a `git-<rev>` toolchain
that a pin names. A toolchain built from a commit is never the default for an
unpinned project; only a pin selects one.
