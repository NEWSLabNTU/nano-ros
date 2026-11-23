//! Every `NROS_*` variable a test sets on a spawned process names a variable
//! something reads at RUN time (issue 2908).
//!
//! A knob rename can leave a test setting a name only the BUILD reads
//! (`option_env!`, `cargo:rerun-if-env-changed`) or a name nothing reads any
//! more. Nothing fails at the set: the process boots on its default, so the
//! test fails somewhere else for a reason that looks like the network. The
//! zenoh action cells set `NROS_ENTRY_LOCATOR` (the build-time carrier) where
//! the runtime reads `NROS_LOCATOR`, and dialled the default router port; the
//! peer-mode pair set `NROS_DOMAIN_ID` where the runtime reads
//! `ROS_DOMAIN_ID`, and lost its domain isolation.
//!
//! What counts as a runtime read, on a line naming the variable:
//! - a shell expansion, `$NAME` or `${NAME` (a script, or a `bash -c` /
//!   `python3 -c "$NAME"` string a test builds). Not in a CMake file, where
//!   `${NAME}` is a CMake variable: there only `$ENV{NAME}` reads the
//!   environment;
//! - the quoted name beside a reader call: `getenv(`, `var(`, `var_os(`, a
//!   typed `env_<type>(` helper, or Python's `environ`.
//!
//! A line that only bakes or writes the name (`option_env!`, `env!(`,
//! `rerun-if-env-changed`, `rustc-env`, `set_var`, `remove_var`, `setenv`, a
//! `.env(` call) is not a read.

use std::collections::BTreeSet;

/// One `.env("NROS_…"` call: where it is and the name it sets.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EnvSet {
    pub file: String,
    pub line: usize,
    pub name: String,
}

const SET_PREFIX: &str = ".env(\"NROS_";

/// The `.env("NROS_…"` calls in one source text.
pub fn env_sets(file: &str, text: &str) -> Vec<EnvSet> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let mut rest = line;
        while let Some(at) = rest.find(SET_PREFIX) {
            let tail = &rest[at + ".env(\"".len()..];
            let name: String = tail
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if tail[name.len()..].starts_with('"') {
                out.push(EnvSet {
                    file: file.to_string(),
                    line: i + 1,
                    name: name.clone(),
                });
            }
            rest = &tail[name.len()..];
        }
    }
    out
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `$NAME` or `${NAME`, with `NAME` whole.
fn shell_expands(line: &str, name: &str) -> bool {
    for prefix in ["${", "$"] {
        let pat = format!("{prefix}{name}");
        let mut rest = line;
        while let Some(at) = rest.find(&pat) {
            let after = rest[at + pat.len()..].chars().next();
            if !after.is_some_and(is_name_char) {
                return true;
            }
            rest = &rest[at + pat.len()..];
        }
    }
    false
}

const BUILD_OR_WRITE: [&str; 8] = [
    "option_env!",
    "env!(",
    "rerun-if-env-changed",
    "rustc-env",
    "set_var",
    "remove_var",
    "setenv",
    ".env(",
];

const READER_CALLS: [&str; 10] = [
    "getenv(",
    "var(",
    "var_os(",
    "environ",
    "env_get(",
    "env_long(",
    "env_path(",
    "env_u64(",
    "env_u32(",
    "env_usize(",
];

/// True if `line`, from the file `path`, reads the environment variable
/// `name` at run time.
pub fn reads_at_runtime(path: &str, line: &str, name: &str) -> bool {
    let cmake = path.ends_with(".cmake") || path.ends_with("CMakeLists.txt");
    if cmake {
        return line.contains(&format!("$ENV{{{name}}}"));
    }
    if shell_expands(line, name) {
        return true;
    }
    let quoted = format!("\"{name}\"");
    line.contains(&quoted)
        && !BUILD_OR_WRITE.iter().any(|m| line.contains(m))
        && READER_CALLS.iter().any(|r| line.contains(r))
}

/// The names in `corpus` (path, text) that some line reads at run time,
/// restricted to `wanted`.
pub fn runtime_read_names<'a>(
    corpus: impl IntoIterator<Item = (&'a str, &'a str)>,
    wanted: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut read = BTreeSet::new();
    for (path, text) in corpus {
        for line in text.lines() {
            if !line.contains("NROS_") {
                continue;
            }
            for name in wanted {
                if !read.contains(name) && reads_at_runtime(path, line, name) {
                    read.insert(name.clone());
                }
            }
        }
    }
    read
}

/// The sets in `sets` whose name nothing in `corpus` reads at run time.
pub fn unread<'a>(
    sets: &[EnvSet],
    corpus: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<EnvSet> {
    let wanted: BTreeSet<String> = sets.iter().map(|s| s.name.clone()).collect();
    let read = runtime_read_names(corpus, &wanted);
    sets.iter()
        .filter(|s| !read.contains(&s.name))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // The negative control: issue 2908's shape. The test sets the build-time
    // carrier, and the only lines naming it bake it.
    #[test]
    fn a_name_only_the_build_reads_is_refused() {
        let test = r#"Command::new(bin).env("NROS_ENTRY_LOCATOR", &locator).spawn();"#;
        let corpus = [
            (
                "nros/src/init.rs",
                r#"let l = option_env!("NROS_ENTRY_LOCATOR");"#,
            ),
            (
                "nros/build.rs",
                r#"println!("cargo:rerun-if-env-changed=NROS_ENTRY_LOCATOR");"#,
            ),
            ("nros/src/env.rs", r#"let locator = var("NROS_LOCATOR");"#),
            ("t.rs", test),
        ];
        let sets = env_sets("t.rs", test);
        assert_eq!(sets.len(), 1);
        let bad = unread(&sets, corpus.iter().map(|(p, t)| (*p, *t)));
        assert_eq!(bad, sets, "the build-time carrier must be refused");
    }

    #[test]
    fn a_name_the_runtime_reads_passes() {
        let test = r#"cmd.env("NROS_LOCATOR", &l).env("NROS_SPIN_MS", "10");"#;
        let corpus = [
            ("env.rs", r#"let locator = var("NROS_LOCATOR");"#),
            ("main.c", r#"const char *s = getenv("NROS_SPIN_MS");"#),
            ("t.rs", test),
        ];
        let sets = env_sets("t.rs", test);
        assert_eq!(sets.len(), 2);
        assert!(unread(&sets, corpus.iter().map(|(p, t)| (*p, *t))).is_empty());
    }

    #[test]
    fn a_shell_expansion_is_a_read_and_a_longer_name_is_not() {
        assert!(reads_at_runtime(
            "t.rs",
            r#"python3 -c "$NROS_BULK_PUB""#,
            "NROS_BULK_PUB"
        ));
        assert!(reads_at_runtime(
            "renode.sh",
            r#"x="${NROS_TWIN_RENODE:-}""#,
            "NROS_TWIN_RENODE"
        ));
        assert!(!reads_at_runtime(
            "t.sh",
            "echo $NROS_BULK_PUB_X",
            "NROS_BULK_PUB"
        ));
        assert!(!reads_at_runtime(
            "lib.rs",
            r#"let why = retired_statement_env_refusal("NROS_DOMAIN_ID");"#,
            "NROS_DOMAIN_ID"
        ));
        assert!(!reads_at_runtime(
            "census.rs",
            r#"unsafe { env::set_var("NROS_RMW", "x") };"#,
            "NROS_RMW"
        ));
    }

    // In CMake `${NAME}` is a CMake variable, which is how the build-time
    // locator reached the first version of this check as a "read".
    #[test]
    fn a_cmake_variable_is_not_an_environment_read() {
        let line = r#"set(_loc "${NROS_ENTRY_LOCATOR}")"#;
        assert!(!reads_at_runtime(
            "cmake/X.cmake",
            line,
            "NROS_ENTRY_LOCATOR"
        ));
        let env = r#"set(_loc "$ENV{NROS_ENTRY_LOCATOR}")"#;
        assert!(reads_at_runtime("cmake/X.cmake", env, "NROS_ENTRY_LOCATOR"));
    }

    /// The live check: every `.env("NROS_…"` in this crate's sources and
    /// tests names a variable something in the checkout reads at run time.
    #[test]
    fn every_runtime_env_a_test_sets_is_read_by_something() {
        let root = crate::project_root();
        let out = std::process::Command::new("git")
            .args(["ls-files", "-z"])
            .current_dir(&root)
            .output()
            .expect("git ls-files");
        assert!(out.status.success(), "git ls-files failed");
        let code_ext = [
            "rs", "c", "h", "cpp", "hpp", "cc", "py", "sh", "just", "cmake",
        ];
        let mut corpus = Vec::new();
        for path in String::from_utf8_lossy(&out.stdout).split('\0') {
            let ext_ok = path
                .rsplit_once('.')
                .is_some_and(|(_, e)| code_ext.contains(&e))
                || path.ends_with("justfile");
            if !ext_ok || path.starts_with("docs/") {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(root.join(path)) {
                corpus.push((path.to_string(), text));
            }
        }
        let tests_dir = "packages/testing/nros-tests/";
        let sets: Vec<EnvSet> = corpus
            .iter()
            .filter(|(p, _)| p.starts_with(tests_dir))
            .flat_map(|(p, t)| env_sets(p, t))
            .collect();
        assert!(
            sets.len() > 100,
            "found only {} `.env(\"NROS_…\"` calls: the scan is not reading the tests",
            sets.len()
        );
        let bad = unread(&sets, corpus.iter().map(|(p, t)| (p.as_str(), t.as_str())));
        assert!(
            bad.is_empty(),
            "a test sets an NROS_* variable nothing reads at run time, so the \
             process runs on its default instead (issue 2908). Set the name the \
             runtime reads (e.g. NROS_LOCATOR, ROS_DOMAIN_ID), or drop the set:\n{}",
            bad.iter()
                .map(|s| format!("  {}:{}  {}", s.file, s.line, s.name))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}
