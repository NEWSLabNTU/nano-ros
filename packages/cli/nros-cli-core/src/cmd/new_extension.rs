//! `nros new <name> --kind runner|command [--lang rust|cpp]` (RFC-0119 D10,
//! phase-527 W7).
//!
//! Writes a runner or command package of the shape the provider scan reads: a
//! `package.xml` announcing `<nano_ros_provides kind="runner|command"
//! name="<name>"/>`, the descriptor (`nros-runner.toml` / `nros-command.toml`)
//! and a stub that implements the contract.
//!
//! - `--lang rust` (the default) is a cargo package on `nros-extension`. The
//!   crate is not on crates.io yet, so the manifest names it by registry name
//!   (`version = "*"`, the spelling every scaffold uses for an unpublished
//!   nros crate) and `nros build` adds the `[patch.crates-io]` row pointing at
//!   the tree's copy (`builder::host_tool::EXTENSION_CRATE`).
//! - `--lang cpp` is a CMake executable with no dependency: `nros-extension`
//!   is a Rust crate, so the C++ stub reads and writes the protocol JSON
//!   itself (RFC-0119 D5, "a runner in another language reads and writes the
//!   JSON").
//!
//! A runner stub implements `describe` and `run`: it executes the plan's
//! `elf` as a host process, as the `native` runner does, after printing one
//! line that says it ran. A command stub prints its arguments and the three
//! variables `nros` sets (RFC-0119 D7).
//!
//! Inside a workspace the package lands in `src/<name>/`, otherwise in
//! `<name>/` (`--destination-directory` overrides both).

use std::path::{Path, PathBuf};

use eyre::{Result, bail};

/// The two host-tool kinds `--kind` scaffolds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtKind {
    /// A runner package (RFC-0119 D6).
    Runner,
    /// A command package (RFC-0119 D7).
    Command,
}

impl ExtKind {
    /// `runner` / `command`; `None` for any other word.
    #[must_use]
    pub fn parse(s: &str) -> Option<ExtKind> {
        match s {
            "runner" => Some(ExtKind::Runner),
            "command" => Some(ExtKind::Command),
            _ => None,
        }
    }

    /// The announcement's `kind`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ExtKind::Runner => "runner",
            ExtKind::Command => "command",
        }
    }
}

/// The language a stub is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    Rust,
    Cpp,
}

/// What one scaffold writes: `(path relative to the package, contents)`.
fn files(kind: ExtKind, lang: Lang, name: &str) -> Vec<(String, String)> {
    let bin = name.replace('_', "-");
    let mut out = vec![("package.xml".to_string(), package_xml(kind, lang, name))];
    out.push(match kind {
        ExtKind::Runner => (
            "nros-runner.toml".to_string(),
            format!(
                "# What nros reads when this runner is the one selected.\n\
                 # `bin` names the executable the build produces.\n\
                 protocol = 1\n\
                 verbs    = [\"run\"]\n\
                 needs    = [\"elf\"]\n\
                 bin      = \"{bin}\"\n"
            ),
        ),
        ExtKind::Command => (
            "nros-command.toml".to_string(),
            format!(
                "# What nros reads when this command is the one selected.\n\
                 # `tier = \"dev\"` would place the verb under `nros dev {name}`.\n\
                 protocol = 1\n\
                 summary  = \"TODO: say what `nros {name}` does\"\n\
                 tier     = \"user\"\n\
                 bin      = \"{bin}\"\n"
            ),
        ),
    });
    match lang {
        Lang::Rust => {
            out.push(("Cargo.toml".to_string(), cargo_toml(kind, name, &bin)));
            out.push((
                "src/main.rs".to_string(),
                match kind {
                    ExtKind::Runner => rust_runner(name),
                    ExtKind::Command => rust_command(name),
                },
            ));
        }
        Lang::Cpp => {
            out.push(("CMakeLists.txt".to_string(), cmake_lists(name, &bin)));
            out.push((
                "src/main.cpp".to_string(),
                match kind {
                    ExtKind::Runner => cpp_runner(name),
                    ExtKind::Command => cpp_command(name),
                },
            ));
        }
    }
    out.push(("README.md".to_string(), readme(kind, lang, name)));
    out
}

fn package_xml(kind: ExtKind, lang: Lang, name: &str) -> String {
    let what = match kind {
        ExtKind::Runner => format!("The `{name}` runner: how `nros run` runs an image."),
        ExtKind::Command => format!("The `nros {name}` command."),
    };
    let build_type = match lang {
        Lang::Rust => "nros_cargo",
        Lang::Cpp => "nros_cmake",
    };
    format!(
        "<?xml version=\"1.0\"?>\n\
         <package format=\"3\">\n  \
           <name>{name}</name>\n  \
           <version>0.1.0</version>\n  \
           <description>{what}</description>\n  \
           <maintainer email=\"you@example.com\">TODO</maintainer>\n  \
           <license>Apache-2.0</license>\n\n  \
           <export>\n    \
             <build_type>{build_type}</build_type>\n    \
             <nano_ros_provides kind=\"{kind}\" name=\"{name}\"/>\n  \
           </export>\n\
         </package>\n",
        kind = kind.as_str(),
    )
}

fn cargo_toml(kind: ExtKind, name: &str, bin: &str) -> String {
    let deps = match kind {
        ExtKind::Runner => "nros-extension = { version = \"*\" }\n",
        ExtKind::Command => "",
    };
    format!(
        "# A {kind} package. `nros build` builds it for this host into\n\
         # build/nros/host/{name}/, with a target directory of its own.\n\
         #\n\
         # nano-ros crates are not published; `version = \"*\"` is only the\n\
         # left-hand side of the patch `nros build` adds for `nros-extension`.\n\
         [package]\n\
         name = \"{name}\"\n\
         version = \"0.1.0\"\n\
         edition = \"2024\"\n\
         publish = false\n\
         \n\
         [[bin]]\n\
         name = \"{bin}\"\n\
         path = \"src/main.rs\"\n\
         \n\
         [dependencies]\n\
         {deps}\
         \n\
         # Its own workspace root: a host tool shares no target directory with an\n\
         # image (one target directory per workspace root).\n\
         [workspace]\n",
        kind = kind.as_str(),
    )
}

fn rust_runner(name: &str) -> String {
    format!(
        r#"//! The `{name}` runner: `describe` and `run`.
//!
//! `nros run` resolves a plan and executes `<bin> run --plan <plan.json>`;
//! `nros-extension` reads the plan, checks the verb, and hands it here. This
//! stub runs the plan's `elf` as a host process, the way the `native` runner
//! does. Replace `run` with what your target needs: a flasher, an emulator, a
//! remote board farm.

use std::process::{{Command, ExitCode}};

use nros_extension::{{
    describe::Describe,
    exit::RunnerExit,
    plan::{{RunPlan, Verb}},
    runner,
}};

/// What this runner says about itself. Keep it equal to `nros-runner.toml`
/// (the test below holds the two together).
fn describe() -> Describe {{
    Describe::builder().verb(Verb::Run).need("elf").build()
}}

fn main() -> ExitCode {{
    runner::main(&describe(), |_verb, plan| run(plan))
}}

fn run(plan: &RunPlan) -> RunnerExit {{
    let Some(exe) = plan.artifacts.get("elf") else {{
        return runner::fail("the plan carries no `elf` artifact to run");
    }};
    eprintln!("{name}: running {{}}", exe.display());
    let mut cmd = Command::new(exe);
    cmd.args(&plan.passthrough);
    runner::exec(&mut cmd)
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn describe_agrees_with_the_descriptor() {{
        runner::descriptor_agrees(&describe(), include_str!("../nros-runner.toml")).unwrap();
    }}
}}
"#
    )
}

fn rust_command(name: &str) -> String {
    format!(
        r#"//! `nros {name}`.
//!
//! `nros {name} <args...>` executes this binary with `<args...>` unchanged.
//! The environment carries `NROS_WS_ROOT` (the workspace), `NROS_TOOLCHAIN`
//! (the toolchain's version) and `NROS_CLI` (the running `nros`): ask
//! `"$NROS_CLI" dev ...` for facts rather than re-deriving them. The exit
//! status is `nros`'s.

use std::process::ExitCode;

fn main() -> ExitCode {{
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {{
        println!("Usage: nros {name} [ARGS...]");
        return ExitCode::SUCCESS;
    }}
    let var = |k: &str| std::env::var(k).unwrap_or_default();
    println!("nros {name}: {{}}", args.join(" "));
    println!("  workspace {{}}", var("NROS_WS_ROOT"));
    println!("  toolchain {{}} ({{}})", var("NROS_TOOLCHAIN"), var("NROS_CLI"));
    ExitCode::SUCCESS
}}
"#
    )
}

fn cmake_lists(name: &str, bin: &str) -> String {
    format!(
        "# A host tool: `nros build` configures this file into\n\
         # build/nros/host/{name}/cmake and builds `{bin}` for this host.\n\
         cmake_minimum_required(VERSION 3.16)\n\
         project({name} LANGUAGES CXX)\n\
         set(CMAKE_CXX_STANDARD 17)\n\
         set(CMAKE_CXX_STANDARD_REQUIRED ON)\n\
         add_executable({bin} src/main.cpp)\n"
    )
}

fn cpp_runner(name: &str) -> String {
    format!(
        r#"// The `{name}` runner: `describe` and `run`.
//
// `nros-extension` is a Rust crate, so this runner speaks the protocol JSON
// itself: `describe` prints the reply, and `run --plan <file>` reads the plan.
// The plan reader below is deliberately small: it pulls the `elf` artifact out
// of the plan's JSON. Use a JSON library once the runner needs more of it.
//
// Exit codes: 0 success, 1 the target failed, 2 the runner
// failed.

#include <cstdio>
#include <fstream>
#include <iterator>
#include <string>
#include <sys/wait.h>
#include <unistd.h>

namespace {{

// Keep equal to nros-runner.toml.
const char *const kDescribe = "{{\"protocol\":1,\"verbs\":[\"run\"],\"needs\":[\"elf\"]}}";

int fail(const std::string &msg) {{
    std::fprintf(stderr, "{name}: %s\n", msg.c_str());
    return 2;
}}

// The string value of `"<key>":"..."` after `from`, or "" (no escapes read).
std::string string_after(const std::string &json, const std::string &key, size_t from = 0) {{
    const std::string needle = "\"" + key + "\"";
    size_t k = json.find(needle, from);
    if (k == std::string::npos) return "";
    size_t q = json.find('"', json.find(':', k + needle.size()) + 1);
    if (q == std::string::npos) return "";
    size_t end = json.find('"', q + 1);
    return end == std::string::npos ? "" : json.substr(q + 1, end - q - 1);
}}

}} // namespace

int main(int argc, char **argv) {{
    const std::string verb = argc > 1 ? argv[1] : "";
    if (verb == "describe") {{
        std::puts(kDescribe);
        return 0;
    }}
    if (verb != "run" || argc != 4 || std::string(argv[2]) != "--plan") {{
        return fail("usage: describe | run --plan <plan.json>");
    }}
    std::ifstream in(argv[3]);
    if (!in) return fail(std::string("cannot read ") + argv[3]);
    const std::string plan((std::istreambuf_iterator<char>(in)), std::istreambuf_iterator<char>());
    const size_t artifacts = plan.find("\"artifacts\"");
    const std::string elf =
        artifacts == std::string::npos ? "" : string_after(plan, "elf", artifacts);
    if (elf.empty()) return fail("the plan carries no `elf` artifact to run");
    std::fprintf(stderr, "{name}: running %s\n", elf.c_str());
    std::fflush(nullptr);
    pid_t pid = fork();
    if (pid < 0) return fail("fork failed");
    if (pid == 0) {{
        execl(elf.c_str(), elf.c_str(), static_cast<char *>(nullptr));
        std::_Exit(127);
    }}
    int status = 0;
    if (waitpid(pid, &status, 0) < 0) return fail("waitpid failed");
    if (WIFEXITED(status) && WEXITSTATUS(status) == 127) return fail("could not start " + elf);
    return WIFEXITED(status) && WEXITSTATUS(status) == 0 ? 0 : 1;
}}
"#
    )
}

fn cpp_command(name: &str) -> String {
    format!(
        r#"// `nros {name}`.
//
// `nros {name} <args...>` executes this binary with `<args...>` unchanged. The
// environment carries NROS_WS_ROOT, NROS_TOOLCHAIN and NROS_CLI; ask
// "$NROS_CLI" dev ... for facts rather than re-deriving them. The exit status
// is nros's.

#include <cstdio>
#include <cstdlib>
#include <string>

int main(int argc, char **argv) {{
    for (int i = 1; i < argc; ++i) {{
        const std::string a = argv[i];
        if (a == "--help" || a == "-h") {{
            std::puts("Usage: nros {name} [ARGS...]");
            return 0;
        }}
    }}
    auto var = [](const char *k) {{
        const char *v = std::getenv(k);
        return v ? v : "";
    }};
    std::printf("nros {name}:");
    for (int i = 1; i < argc; ++i) std::printf(" %s", argv[i]);
    std::printf("\n  workspace %s\n  toolchain %s (%s)\n", var("NROS_WS_ROOT"),
                var("NROS_TOOLCHAIN"), var("NROS_CLI"));
    return 0;
}}
"#
    )
}

fn readme(kind: ExtKind, lang: Lang, name: &str) -> String {
    let (use_it, check) = match kind {
        ExtKind::Runner => (
            format!(
                "Select it for an image in the bringup's `system.toml`:\n\n\
                 ```toml\n[image.<id>.run]\nmethod = \"{name}\"\n```\n\n\
                 then `nros run <id>`."
            ),
            format!("nros dev runner check {name}"),
        ),
        ExtKind::Command => (
            format!("Run it as `nros {name} <args...>`; `nros help --all` lists it."),
            format!("nros dev command check {name}"),
        ),
    };
    let lang = match lang {
        Lang::Rust => "Rust, on `nros-extension`",
        Lang::Cpp => "C++, speaking the protocol JSON itself",
    };
    format!(
        "# {name}\n\n\
         A {kind} package, in {lang}. `nros build` builds it for this\n\
         host with the workspace.\n\n\
         {use_it}\n\n\
         Check that it builds, answers `describe` and speaks a protocol this `nros`\n\
         speaks:\n\n\
         ```sh\n{check}\n```\n",
        kind = kind.as_str(),
    )
}

/// A package name the scan and cargo both accept: lowercase ASCII letters,
/// digits and `_`, starting with a letter (ROS's package-name rule).
fn check_name(name: &str) -> Result<()> {
    let ok = name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !ok {
        bail!(
            "`{name}` is not a package name: use lowercase letters, digits and `_`, starting \
             with a letter (e.g. `acme_flash`)"
        );
    }
    Ok(())
}

/// Where the package goes: `--destination-directory/<name>`, else
/// `<ws>/src/<name>` inside a workspace (run from its root or any directory
/// below it), else `<name>`.
fn target_dir(args: &crate::cmd::new::Args, name: &str, cwd: &Path) -> PathBuf {
    if args.destination_directory.is_some() {
        return crate::cmd::new::package_dir(args, name);
    }
    // One workspace detector for every `nros new` (phase-531 W4): the node
    // rung and the extension kinds agree on what "inside a workspace" means.
    match crate::cmd::new_workspace::enclosing_workspace(cwd) {
        Some(ws) if ws == cwd => PathBuf::from("src").join(name),
        Some(ws) => ws.join("src").join(name),
        None => PathBuf::from(name),
    }
}

/// `nros new <name> --kind runner|command`.
///
/// # Errors
///
/// No or an invalid name, a language with no stub, or a directory that
/// already exists (without `--force`).
pub fn scaffold(args: &crate::cmd::new::Args, kind: ExtKind) -> Result<()> {
    let Some(name) = args.name.as_ref().and_then(|p| p.to_str()) else {
        bail!("`nros new <name> --kind {}` requires a name", kind.as_str());
    };
    check_name(name)?;
    let lang = match crate::cmd::new::language_arg(args.lang.as_deref(), nros_lang::Language::Rust)?
    {
        nros_lang::Language::Rust => Lang::Rust,
        nros_lang::Language::Cpp => Lang::Cpp,
        nros_lang::Language::C => bail!(
            "`--kind {}` scaffolds `--lang rust` or `--lang cpp`, not `c`",
            kind.as_str()
        ),
    };
    let cwd = std::env::current_dir()?;
    let dir = target_dir(args, name, &cwd);
    if dir.exists() && !args.force {
        bail!(
            "{} already exists; pass --force to write into it",
            dir.display()
        );
    }
    for (rel, body) in files(kind, lang, name) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, body)?;
    }
    let check = match kind {
        ExtKind::Runner => "runner",
        ExtKind::Command => "command",
    };
    eprintln!(
        "nros new: scaffolded {} package `{name}` at {}\n  next: nros build --packages-select \
         {name} && nros dev {check} check {name}",
        kind.as_str(),
        dir.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_and_language_writes_its_files() {
        for (kind, lang, want) in [
            (
                ExtKind::Runner,
                Lang::Rust,
                &["nros-runner.toml", "Cargo.toml", "src/main.rs"][..],
            ),
            (
                ExtKind::Command,
                Lang::Rust,
                &["nros-command.toml", "Cargo.toml", "src/main.rs"][..],
            ),
            (
                ExtKind::Runner,
                Lang::Cpp,
                &["nros-runner.toml", "CMakeLists.txt", "src/main.cpp"][..],
            ),
            (
                ExtKind::Command,
                Lang::Cpp,
                &["nros-command.toml", "CMakeLists.txt", "src/main.cpp"][..],
            ),
        ] {
            let got: Vec<String> = files(kind, lang, "acme_tool")
                .into_iter()
                .map(|(p, _)| p)
                .collect();
            for w in want {
                assert!(got.iter().any(|g| g == w), "{kind:?}/{lang:?}: {got:?}");
            }
            assert!(got.iter().any(|g| g == "package.xml"));
        }
    }

    #[test]
    fn the_descriptor_bin_is_the_cargo_bin() {
        let f = files(ExtKind::Runner, Lang::Rust, "acme_tool");
        let get = |n: &str| f.iter().find(|(p, _)| p == n).unwrap().1.clone();
        assert!(get("nros-runner.toml").contains("bin      = \"acme-tool\""));
        assert!(get("Cargo.toml").contains("name = \"acme-tool\""));
        assert!(get("Cargo.toml").contains("nros-extension = { version = \"*\" }"));
        assert!(
            get("package.xml").contains("<nano_ros_provides kind=\"runner\" name=\"acme_tool\"/>")
        );
    }

    #[test]
    fn the_cpp_describe_reply_is_a_valid_one() {
        let f = files(ExtKind::Runner, Lang::Cpp, "acme_tool");
        let main = &f.iter().find(|(p, _)| p == "src/main.cpp").unwrap().1;
        let line = main.lines().find(|l| l.contains("kDescribe =")).unwrap();
        let json = line
            .split_once('"')
            .unwrap()
            .1
            .rsplit_once('"')
            .unwrap()
            .0
            .replace("\\\"", "\"");
        let d = nros_extension::describe::Describe::parse(&json).unwrap();
        nros_extension::runner::descriptor_agrees(
            &d,
            &f.iter().find(|(p, _)| p == "nros-runner.toml").unwrap().1,
        )
        .unwrap();
    }

    /// `nros new x --kind runner` lands in `<ws>/src/x` from the workspace
    /// root and from any directory below it, and in `./x` outside one — the
    /// same detector the node rung uses.
    #[test]
    fn an_extension_lands_in_the_enclosing_workspaces_src() {
        #[derive(clap::Parser)]
        struct Cli {
            #[command(flatten)]
            args: crate::cmd::new::Args,
        }
        use clap::Parser;
        let args = Cli::try_parse_from(["nros-new", "x", "--kind", "runner"])
            .unwrap()
            .args;
        let t = tempfile::tempdir().unwrap();
        let ws = t.path().join("ws");
        let pkg = ws.join("src/talker_node");
        std::fs::create_dir_all(pkg.join("src")).unwrap();
        std::fs::write(pkg.join("package.xml"), "<package/>").unwrap();

        assert_eq!(target_dir(&args, "x", &ws), PathBuf::from("src/x"));
        assert_eq!(target_dir(&args, "x", &pkg.join("src")), ws.join("src/x"));
        assert_eq!(target_dir(&args, "x", &ws.join("src")), ws.join("src/x"));
        let outside = t.path().join("elsewhere");
        std::fs::create_dir_all(&outside).unwrap();
        assert_eq!(target_dir(&args, "x", &outside), PathBuf::from("x"));
    }

    #[test]
    fn a_bad_name_is_refused() {
        assert!(check_name("acme_flash").is_ok());
        for bad in ["Acme", "1acme", "acme-flash", ""] {
            assert!(check_name(bad).is_err(), "{bad}");
        }
    }
}
