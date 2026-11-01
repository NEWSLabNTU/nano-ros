//! `nros dev` — the developer and build-system tier (RFC-0117 D1/D3).
//!
//! Every verb here already existed under an older spelling, and its callers
//! (cmake, west, `just`, gates) parse its output, so this module adds
//! SPELLINGS and nothing else. A `dev` command is parsed into [`Args`] and
//! [`lower`]ed into the [`Cmd`] its old spelling parses into, before the stale
//! guard and the dispatch see it. One dispatch therefore serves both
//! spellings, and the guard's per-verb answer (`cmd_name` in `lib.rs`) cannot
//! differ between them.
//!
//! The old spellings stay as hidden variants of [`Cmd`] and of the nouns they
//! lived under (RFC-0117 D5: a dev alias is silent). [`ALIASES`] lists every
//! pair, and the test below proves each new spelling lowers to exactly what
//! its old spelling parses to.

use clap::{Args as ClapArgs, Subcommand};

use crate::cmd::{
    self, Cmd, board_facts, entity_census, entity_inventory, image_facts, leaf_system,
    message_bound_knobs, metadata, model_path, profile, sdk_front, sdk_path, sdk_root,
    sizing_descriptor, toolchain, west_leaf_sizing, ws,
};

#[derive(Debug, ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    pub noun: Noun,
}

#[derive(Debug, Subcommand)]
pub enum Noun {
    /// Generate message bindings and baked sources for a build system.
    Codegen(cmd::codegen::Args),
    /// Facts about one image, for the configure step that builds it.
    #[command(subcommand)]
    Image(Image),
    /// Lower an RMW selection into the build facts its descriptor declares.
    #[command(subcommand)]
    Rmw(Rmw),
    /// Seed, check and rewrite the lockfiles and patch tables nros manages.
    #[command(subcommand)]
    Lock(Lock),
    /// The cargo build-profile table that cmake and shell recipes read.
    #[command(subcommand_negates_reqs = true)]
    Profile(profile::Args),
    /// Project a board descriptor for a build system.
    #[command(subcommand)]
    Board(Board),
    /// Workspace discovery: the interface search path, providers, build order.
    #[command(subcommand)]
    Ws(Ws),
    /// Where the SDK, its tools and the toolchain's manifest are.
    #[command(subcommand)]
    Sdk(Sdk),
    /// Collect component source metadata for orchestration planning.
    Metadata(metadata::Args),
}

#[derive(Debug, Subcommand)]
pub enum Image {
    /// Print the resolved image as data, for a west build that derives its
    /// cargo invocation from `[image.*]`.
    Facts(image_facts::Args),
    /// Print the resolved SystemModel path for a bringup, launch file and
    /// arguments.
    #[command(name = "model-path")]
    ModelPath(model_path::Args),
    /// Print a single-package leaf's deployment: board, RMW, domain, network
    /// identity and the C/C++ deploy token.
    #[command(name = "leaf-system")]
    LeafSystem(leaf_system::LeafSystemArgs),
    /// Print one deploy's resolved board facts and site config as
    /// `KEY=VALUE` lines.
    #[command(name = "board-facts")]
    BoardFacts(board_facts::BoardFactsArgs),
    /// Read back the sizing descriptor `nros sync` wrote for an entry.
    #[command(name = "sizing-descriptor")]
    SizingDescriptor(sizing_descriptor::SizingDescriptorArgs),
    /// A standalone Zephyr Rust leaf's derived sizes, for its west configure.
    #[command(name = "west-leaf-sizing")]
    WestLeafSizing(west_leaf_sizing::WestLeafSizingArgs),
    /// Compose an image's registered entity declarations into its inventory.
    #[command(name = "entity-inventory")]
    EntityInventory(entity_inventory::EntityInventoryArgs),
    /// Run, check or locate the entity census an image's configure compares
    /// against its contract.
    #[command(name = "entity-census")]
    EntityCensus(entity_census::EntityCensusArgs),
    /// Print the dimensions of a resolved SystemModel.
    #[command(name = "model-dims", hide = true)]
    ModelDims(ws::ModelDimsArgs),
    #[command(name = "message-bound-knobs", hide = true)]
    MessageBoundKnobs(message_bound_knobs::MessageBoundKnobsArgs),
}

#[derive(Debug, Subcommand)]
pub enum Rmw {
    /// Lower one declared RMW name to the build facts its descriptor declares.
    Dispatch(ws::RmwDispatchArgs),
    /// Print a bringup's default session RMW file for one backend.
    Config(ws::RmwConfigArgs),
    /// A bridged domain's RMW session properties as a C header.
    #[command(name = "session-properties")]
    SessionProperties(ws::SessionPropertiesArgs),
}

#[derive(Debug, Subcommand)]
pub enum Lock {
    /// Seed `<dir>/Cargo.lock` from the registry pin set, when it has none.
    Seed(ws::SeedLockArgs),
    /// Hold the locks a plain `cargo` resolved to the pin set.
    Check(ws::CheckLockArgs),
    /// Write, or with `--check` report, the managed message rows of every
    /// Rust package.
    #[command(name = "managed-deps")]
    ManagedDeps(ws::ManagedDepsArgs),
    /// Print the `[patch.crates-io]` table a cmake runtime umbrella needs.
    #[command(name = "central-patch")]
    CentralPatch(ws::CentralPatchArgs),
}

#[derive(Debug, Subcommand)]
pub enum Board {
    /// Project a board descriptor into cmake variables.
    #[command(name = "cmake-vars")]
    CmakeVars(cmd::board::CmakeVarsArgs),
    /// Print a board's resolved network identity defaults.
    Net(cmd::board::NetArgs),
}

#[derive(Debug, Subcommand)]
pub enum Ws {
    /// Print the shell export adding a directory to the interface search path.
    Env(ws::EnvArgs),
    /// Print the workspace's packages in dependency order.
    Order(ws::OrderArgs),
    /// List packages that announce a provision, across the search path.
    Providers(ws::ProvidersArgs),
}

#[derive(Debug, Subcommand)]
pub enum Sdk {
    /// Point `$NROS_HOME/bin/<name>` at the newest installed version of a tool.
    Front(sdk_front::Args),
    /// Print the constructed install path of a provisioned tool.
    Path(sdk_path::Args),
    /// Print the nano-ros SDK root this toolchain resolves.
    Root(sdk_root::Args),
    /// Read, or stamp, the toolchain's `share/nros/manifest.toml`.
    Manifest(toolchain::ManifestArgs),
}

/// Lower a `dev` command into the [`Cmd`] its old spelling parses to.
pub fn lower(args: Args) -> Cmd {
    let ws = |command| Cmd::Ws(ws::Args { command });
    match args.noun {
        // `rust` and `system` are the old `generate-rust` and
        // `codegen-system`, gathered under the codegen noun.
        Noun::Codegen(mut a) => match a.command.take() {
            Some(cmd::codegen::Sub::Rust(r)) => Cmd::GenerateRust(r),
            Some(cmd::codegen::Sub::System(s)) => Cmd::CodegenSystem(s),
            other => {
                a.command = other;
                Cmd::Codegen(a)
            }
        },
        Noun::Image(v) => match v {
            Image::Facts(a) => Cmd::ImageFacts(a),
            Image::ModelPath(a) => Cmd::ModelPath(a),
            Image::LeafSystem(a) => ws(ws::Sub::LeafSystem(a)),
            Image::BoardFacts(a) => ws(ws::Sub::BoardFacts(a)),
            Image::SizingDescriptor(a) => ws(ws::Sub::SizingDescriptor(a)),
            Image::WestLeafSizing(a) => ws(ws::Sub::WestLeafSizing(a)),
            Image::EntityInventory(a) => ws(ws::Sub::EntityInventory(a)),
            Image::EntityCensus(a) => ws(ws::Sub::EntityCensus(a)),
            Image::ModelDims(a) => ws(ws::Sub::ModelDims(a)),
            Image::MessageBoundKnobs(a) => ws(ws::Sub::MessageBoundKnobs(a)),
        },
        Noun::Rmw(v) => match v {
            Rmw::Dispatch(a) => ws(ws::Sub::RmwDispatch(a)),
            Rmw::Config(a) => ws(ws::Sub::RmwConfig(a)),
            Rmw::SessionProperties(a) => ws(ws::Sub::SessionProperties(a)),
        },
        Noun::Lock(v) => match v {
            Lock::Seed(a) => ws(ws::Sub::SeedLock(a)),
            Lock::Check(a) => ws(ws::Sub::CheckLock(a)),
            Lock::ManagedDeps(a) => ws(ws::Sub::ManagedDeps(a)),
            Lock::CentralPatch(a) => ws(ws::Sub::CentralPatch(a)),
        },
        Noun::Profile(a) => Cmd::Profile(a),
        Noun::Board(v) => Cmd::Board(match v {
            Board::CmakeVars(a) => cmd::board::Args::CmakeVars(a),
            Board::Net(a) => cmd::board::Args::Net(a),
        }),
        Noun::Ws(v) => match v {
            Ws::Env(a) => ws(ws::Sub::Env(a)),
            Ws::Order(a) => ws(ws::Sub::Order(a)),
            Ws::Providers(a) => ws(ws::Sub::Providers(a)),
        },
        Noun::Sdk(v) => match v {
            Sdk::Front(a) => Cmd::SdkFront(a),
            Sdk::Path(a) => Cmd::SdkPath(a),
            Sdk::Root(a) => Cmd::SdkRoot(a),
            Sdk::Manifest(a) => Cmd::Toolchain(toolchain::Args {
                command: toolchain::Sub::Manifest(a),
            }),
        },
        Noun::Metadata(a) => Cmd::Metadata(a),
    }
}

/// Every old spelling a `dev` verb replaced, as `(old, new)` command paths.
///
/// The old spelling stays a hidden, silent alias (RFC-0117 D5). The tests
/// below hold this table complete in both directions: every `dev` verb has a
/// row, and every row's two spellings parse to the same command.
pub const ALIASES: &[(&str, &str)] = &[
    ("codegen", "dev codegen"),
    ("generate-rust", "dev codegen rust"),
    ("codegen-system", "dev codegen system"),
    ("image-facts", "dev image facts"),
    ("model-path", "dev image model-path"),
    ("ws leaf-system", "dev image leaf-system"),
    ("ws board-facts", "dev image board-facts"),
    ("ws sizing-descriptor", "dev image sizing-descriptor"),
    ("ws west-leaf-sizing", "dev image west-leaf-sizing"),
    ("ws entity-inventory", "dev image entity-inventory"),
    ("ws entity-census", "dev image entity-census"),
    ("ws model-dims", "dev image model-dims"),
    ("ws message-bound-knobs", "dev image message-bound-knobs"),
    ("ws rmw-dispatch", "dev rmw dispatch"),
    ("ws rmw-config", "dev rmw config"),
    ("ws session-properties", "dev rmw session-properties"),
    ("ws seed-lock", "dev lock seed"),
    ("ws check-lock", "dev lock check"),
    ("ws managed-deps", "dev lock managed-deps"),
    ("ws central-patch", "dev lock central-patch"),
    ("profile", "dev profile"),
    ("board cmake-vars", "dev board cmake-vars"),
    ("board net", "dev board net"),
    ("ws env", "dev ws env"),
    ("ws order", "dev ws order"),
    ("ws providers", "dev ws providers"),
    ("sdk-front", "dev sdk front"),
    ("sdk-path", "dev sdk path"),
    ("sdk-root", "dev sdk root"),
    ("toolchain manifest", "dev sdk manifest"),
    ("metadata", "dev metadata"),
];

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    #[derive(Debug, Parser)]
    #[command(name = "nros")]
    pub(crate) struct Cli {
        #[command(subcommand)]
        pub(crate) cmd: Cmd,
    }

    pub(crate) fn find<'a>(root: &'a clap::Command, path: &str) -> &'a clap::Command {
        path.split(' ').fold(root, |c, name| {
            c.find_subcommand(name)
                .unwrap_or_else(|| panic!("`nros {path}`: no subcommand `{name}`"))
        })
    }

    /// The argument and subcommand tree, without help text: two spellings
    /// over one `Args` type have the same shape, two different types do not.
    pub(crate) fn shape(c: &clap::Command) -> String {
        let mut args: Vec<String> = c
            .get_arguments()
            .map(|a| {
                format!(
                    "{}:{:?}:{:?}:{}",
                    a.get_id(),
                    a.get_long(),
                    a.get_short(),
                    a.is_required_set()
                )
            })
            .collect();
        args.sort();
        let mut subs: Vec<String> = c
            .get_subcommands()
            .map(|s| format!("{}{{{}}}", s.get_name(), shape(s)))
            .collect();
        subs.sort();
        format!("[{}]({})", args.join(","), subs.join(","))
    }

    /// A parseable argv for `path`: descend through required subcommands
    /// (taking the first), then supply every required argument with `1`, or
    /// its first possible value.
    pub(crate) fn argv(root: &clap::Command, path: &str) -> Vec<String> {
        let mut v: Vec<String> = std::iter::once("nros")
            .chain(path.split(' '))
            .map(String::from)
            .collect();
        let mut c = find(root, path);
        while c.is_subcommand_required_set() {
            c = c.get_subcommands().next().expect("a required subcommand");
            v.push(c.get_name().to_string());
        }
        let sample = |a: &clap::Arg| {
            a.get_possible_values()
                .first()
                .map(|p| p.get_name().to_string())
                .unwrap_or_else(|| "1".to_string())
        };
        let mut positional: Vec<&clap::Arg> = Vec::new();
        for a in c.get_arguments().filter(|a| a.is_required_set()) {
            if a.is_positional() {
                positional.push(a);
            } else if let Some(long) = a.get_long() {
                v.push(format!("--{long}"));
                v.push(sample(a));
            }
        }
        positional.sort_by_key(|a| a.get_index());
        v.extend(positional.into_iter().map(sample));
        v
    }

    pub(crate) fn parsed(argv: &[String]) -> Cmd {
        match Cli::try_parse_from(argv) {
            Ok(cli) => crate::cmd::lower(cli.cmd),
            Err(e) => panic!("`{}` does not parse: {e}", argv.join(" ")),
        }
    }

    #[test]
    fn every_alias_pair_parses_to_the_same_command() {
        let root = Cli::command();
        for (old, new) in ALIASES {
            assert_eq!(
                shape(find(&root, old)),
                shape(find(&root, new)),
                "`nros {old}` and `nros {new}` take different arguments"
            );
            let old_argv = argv(&root, old);
            let mut new_argv = argv(&root, new);
            // The tail after the command path must be the same for both, so
            // build the new argv from the old one's tail.
            let old_tail = &old_argv[1 + old.split(' ').count()..];
            new_argv.truncate(1 + new.split(' ').count());
            new_argv.extend(old_tail.iter().cloned());
            assert_eq!(
                format!("{:?}", parsed(&old_argv)),
                format!("{:?}", parsed(&new_argv)),
                "`{}` and `{}` lower to different commands",
                old_argv.join(" "),
                new_argv.join(" ")
            );
        }
    }

    #[test]
    fn every_dev_verb_has_an_alias_row() {
        let root = Cli::command();
        let dev = find(&root, "dev");
        let mut missing = Vec::new();
        for noun in dev.get_subcommands() {
            let noun_path = format!("dev {}", noun.get_name());
            // A noun that IS an old verb (codegen, profile, metadata) is one
            // row; its own subcommands came with it.
            if ALIASES.iter().any(|(_, new)| *new == noun_path) {
                continue;
            }
            for verb in noun.get_subcommands() {
                let path = format!("{noun_path} {}", verb.get_name());
                if !ALIASES.iter().any(|(_, new)| *new == path) {
                    missing.push(path);
                }
            }
        }
        assert!(
            missing.is_empty(),
            "dev verbs with no alias row: {missing:?}"
        );
    }

    #[test]
    fn old_dev_spellings_are_hidden() {
        let root = Cli::command();
        let shown: Vec<&str> = ALIASES
            .iter()
            .map(|(old, _)| *old)
            .filter(|old| !find(&root, old).is_hide_set())
            .collect();
        assert!(shown.is_empty(), "old spellings still in help: {shown:?}");
        assert!(
            find(&root, "dev").is_hide_set(),
            "`nros dev` must be hidden"
        );
    }
}
