//! Which roads carry each knob from `[image.<id>.knobs]` (phase-568 W2a).
//!
//! RFC-0117 D9 refuses a `-D` naming a knob and points at
//! `[image.<id>.knobs.<group>]`, its one committed place. That remedy works
//! only where the image's value actually reaches the code that reads the
//! knob, and issue 2496 found a row where it did not. So the refusal is on
//! only for the rows this table reads `yes` on every road
//! (`build_args::knob_refusal_on`), and this module MEASURES, from
//! the code, which roads carry which rows. The test pins the table in
//! `knob_road_coverage.tsv`: a row whose coverage drops is red, and so is one
//! that improves without the table being updated, so the table stays the
//! true size of the follow-up (W2c).
//!
//! **Consumers** of a row, found by its env name `NROS_<id>`:
//! - `rust`: a non-test Rust source under `packages/` names it (a build
//!   script or `nros_zephyr_build::knob` reader);
//! - `cmake`: a configure-time CMake file under `cmake/` or `zephyr/` names
//!   it, other than the generated knob tables and the carriers below.
//!
//! **Carriers** of `[image.<id>.knobs]`, per road:
//! - **cargo:** every row goes into `build/<image>/nros-cargo.toml` `[env]`,
//!   which reaches build scripts. It carries a row with a `rust` reader.
//! - **cmake:** the image-env file (`NanoRosImageEnv.cmake`) is handed to
//!   every cargo command, which reaches `rust` readers. A `cmake` reader sees
//!   the value only through `nros_image_env_value(<var> NROS_<id>)`, so a
//!   row with a cmake reader that does not use it is NOT carried (2496).
//! - **west:** a row whose Kconfig symbol the module defines is rendered as a
//!   Kconfig default (`leaf_kconfig::defaults_text`), which reaches both its
//!   cmake readers (through `CONFIG_*`) and its Rust readers (the module's knob
//!   forward). A row with no symbol is passed to cargo and recorded as
//!   `NROS_IMAGE_ENV_ROW_<name>`, which `nros_image_env_value` reads, so it is
//!   carried when it has no cmake reader or its readers ask that function.
//!
//! A cell reads `yes`, `no`, or `?`: no reader was found BY NAME, so nothing
//! is proven for that row. A `?` is not "no reader": some code reads knobs
//! through a table rather than a literal (nros-zpico-build iterates the
//! registry), and a `?` row is never turned on until its readers are named.

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        path::{Path, PathBuf},
    };

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("repo root")
            .to_path_buf()
    }

    const SNAPSHOT: &str = "src/builder/knob_road_coverage.tsv";

    /// Files a knob name appears in that are tables or carriers, not readers.
    const NOT_READERS: &[&str] = &[
        "packages/tooling/nros-zephyr-build/src/knob_registry.rs",
        "packages/tooling/nros-zephyr-build/knobs.toml",
        "cmake/NanoRosKnobRegistry.cmake",
        "cmake/NanoRosImageEnv.cmake",
        "zephyr/cmake/nros_cargo_build.cmake",
    ];

    fn walk(dir: &Path, exts: &[&str], out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if matches!(
                    name.as_str(),
                    "target"
                        | "third-party"
                        | "tests"
                        | "build"
                        | ".git"
                        | "generated"
                        | "testing_workspaces"
                ) || name.starts_with("target-")
                {
                    continue;
                }
                walk(&p, exts, out);
            } else if exts.iter().any(|x| name.ends_with(x)) {
                out.push(p);
            }
        }
    }

    /// Source text with test modules and comments dropped.
    fn code(text: &str, comment: &str) -> String {
        let mut out = String::new();
        let mut in_test = false;
        for line in text.lines() {
            if line == "#[cfg(test)]" {
                in_test = true;
            }
            if in_test {
                if line == "}" {
                    in_test = false;
                }
                continue;
            }
            let l = line.trim_start();
            if l.starts_with(comment) {
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
        out
    }

    /// CMake text with each quoted string reduced to the `${...}` references
    /// in it: a name inside a message is not a read, a `${NAME}` is.
    fn cmake_uses(text: &str) -> String {
        let mut out = String::new();
        let mut in_str = false;
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\\' if in_str => {
                    chars.next();
                }
                '"' => {
                    in_str = !in_str;
                    out.push(' ');
                }
                '$' if in_str && chars.peek() == Some(&'{') => {
                    out.push_str(" ${");
                    chars.next();
                    for d in chars.by_ref() {
                        out.push(d);
                        if d == '}' {
                            break;
                        }
                    }
                    out.push(' ');
                }
                _ if in_str => {}
                c => out.push(c),
            }
        }
        out
    }

    fn names_word(text: &str, word: &str) -> bool {
        text.match_indices(word).any(|(i, _)| {
            let before = text[..i].chars().last();
            let after = text[i + word.len()..].chars().next();
            let id = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
            !id(before) && !id(after)
        })
    }

    struct Corpus {
        rust: Vec<(String, String)>,
        cmake: Vec<(String, String)>,
        kconfig: String,
    }

    fn corpus(root: &Path) -> Corpus {
        let rel = |p: &Path| p.strip_prefix(root).unwrap_or(p).display().to_string();
        let mut rs = Vec::new();
        walk(&root.join("packages"), &[".rs"], &mut rs);
        let mut cm = Vec::new();
        for d in ["cmake", "zephyr"] {
            walk(&root.join(d), &[".cmake", "CMakeLists.txt"], &mut cm);
        }
        // Issue 2496 — a configure-time reader also lives beside its package
        // (`packages/rmw/cyclonedds/.../cmake/NrosRmwCycloneddsDescriptorCap.cmake`
        // reads NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES). Scanning only `cmake/`
        // and `zephyr/` counted such a row as carried by having no cmake reader.
        walk(&root.join("packages"), &[".cmake"], &mut cm);
        let read = |files: Vec<PathBuf>, comment: &str| {
            files
                .into_iter()
                .filter(|p| !NOT_READERS.contains(&rel(p).as_str()))
                .filter_map(|p| {
                    std::fs::read_to_string(&p)
                        .ok()
                        .map(|t| (rel(&p), code(&t, comment)))
                })
                .collect::<Vec<_>>()
        };
        Corpus {
            rust: read(rs, "//"),
            cmake: read(cm, "#")
                .into_iter()
                .map(|(p, t)| (p, cmake_uses(&t)))
                .collect(),
            kconfig: std::fs::read_to_string(root.join("zephyr/Kconfig")).unwrap_or_default(),
        }
    }

    fn cell(reader: bool, carried: bool) -> &'static str {
        match (reader, carried) {
            (false, _) => "?",
            (true, true) => "yes",
            (true, false) => "no",
        }
    }

    /// `id -> [cargo, cmake, west]`, measured.
    fn measure(c: &Corpus) -> BTreeMap<String, [&'static str; 3]> {
        let mut out = BTreeMap::new();
        for row in nros_zephyr_build::KNOBS
            .iter()
            .filter(|r| r.retired.is_none())
        {
            let env = row.env;
            let rust = c
                .rust
                .iter()
                .any(|(_, t)| t.contains(&format!("\"{env}\"")));
            let cmake_files: Vec<&String> = c
                .cmake
                .iter()
                .filter(|(_, t)| names_word(t, env))
                .map(|(_, t)| t)
                .collect();
            let cmake = !cmake_files.is_empty();
            let via_image_env = c.cmake.iter().any(|(_, t)| {
                t.split("nros_image_env_value(")
                    .skip(1)
                    .any(|rest| names_word(rest.split(')').next().unwrap_or(""), env))
            });
            let sym = row.kconfig.trim_start_matches("CONFIG_");
            let kconfig = c
                .kconfig
                .lines()
                .any(|l| l.trim() == format!("config {sym}"));
            let reader = rust || cmake;
            let cargo = cell(rust, rust);
            let cmake_road = cell(reader, (!cmake || via_image_env) && reader);
            // Issue 2496 — `nros_image_env_value` also reads the west road's
            // recorded row (`NROS_IMAGE_ENV_ROW_<name>`), so a cmake reader
            // that asks it is reached on west too.
            let west = cell(reader, kconfig || !cmake || via_image_env);
            out.insert(row.id.to_string(), [cargo, cmake_road, west]);
        }
        out
    }

    fn render(m: &BTreeMap<String, [&str; 3]>) -> String {
        let mut s = String::from(
            "# Generated by `knob_roads::tests` (phase-568 W2a): does [image.<id>.knobs]\n\
             # reach each road's readers of the row? yes / no / ? (no reader found by name: unproven).\n\
             # Regenerate: NROS_KNOB_ROADS_WRITE=1 cargo nextest run -p nros-cli-core -E 'test(knob_roads)'\n\
             id\tcargo\tcmake\twest\n",
        );
        for (id, [a, b, c]) in m {
            s.push_str(&format!("{id}\t{a}\t{b}\t{c}\n"));
        }
        s
    }

    #[test]
    fn knob_road_coverage_matches_the_pinned_table() {
        let root = repo_root();
        let c = corpus(&root);
        assert!(
            c.rust.len() > 500 && c.cmake.len() > 20,
            "the corpus is wrong"
        );
        let got = render(&measure(&c));
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(SNAPSHOT);
        if std::env::var_os("NROS_KNOB_ROADS_WRITE").is_some() {
            std::fs::write(&path, &got).unwrap();
            return;
        }
        let want = std::fs::read_to_string(&path).unwrap_or_default();
        if got != want {
            let rows = |s: &str| -> BTreeMap<String, String> {
                s.lines()
                    .filter(|l| !l.starts_with('#') && !l.starts_with("id\t"))
                    .filter_map(|l| {
                        l.split_once('\t')
                            .map(|(a, b)| (a.to_string(), b.to_string()))
                    })
                    .collect()
            };
            let (g, w) = (rows(&got), rows(&want));
            let mut diff = Vec::new();
            for (id, v) in &g {
                if w.get(id) != Some(v) {
                    diff.push(format!("{id}: pinned {:?}, measured {v:?}", w.get(id)));
                }
            }
            for id in w.keys().filter(|k| !g.contains_key(*k)) {
                diff.push(format!("{id}: pinned, no longer a live row"));
            }
            panic!(
                "knob road coverage moved ({SNAPSHOT}). A `yes` that became `no` is a \
                 carriage regression; a `no` that became `yes` is progress to record. \
                 Regenerate with NROS_KNOB_ROADS_WRITE=1:\n  {}",
                diff.join("\n  ")
            );
        }
    }

    /// Issue 2496's rows: a configure-time cmake reader the image reaches, the
    /// app stack in `cmake/` and the Cyclone descriptor cap beside its package.
    #[test]
    fn the_cmake_road_carries_its_configure_time_readers() {
        let m = measure(&corpus(&repo_root()));
        for id in ["FREERTOS_APP_STACK_KB", "CYCLONEDDS_MAX_DESCRIPTOR_TYPES"] {
            assert_eq!(m[id][1], "yes", "{id}: {:?}", m[id]);
        }
        // The Cyclone cap also runs on the Zephyr road, where it reaches the
        // image's row through `nros_image_env_value`'s west branch.
        assert_eq!(m["CYCLONEDDS_MAX_DESCRIPTOR_TYPES"][2], "yes");
    }

    /// The rule's two halves on fixed text, so a change in a file's wording
    /// cannot quietly turn the measurement into one that never says `no`.
    #[test]
    fn the_measurement_tells_a_reached_reader_from_an_unreached_one() {
        assert!(names_word("if(NROS_X_KB)", "NROS_X_KB"));
        assert!(!names_word("NROS_X_KB_MAX", "NROS_X_KB"));
        let used = cmake_uses("message(\"NROS_X_KB is ${NROS_Y}\")\nif(DEFINED NROS_Z)\n");
        assert!(!names_word(&used, "NROS_X_KB"), "{used}");
        assert!(
            names_word(&used, "NROS_Y") && names_word(&used, "NROS_Z"),
            "{used}"
        );
        assert_eq!(cell(true, false), "no");
        assert_eq!(cell(false, true), "?");
        assert_eq!(
            code(
                "a\n# NROS_Y\nb\n#[cfg(test)]\nmod t {\n NROS_Z\n}\nc\n",
                "#"
            ),
            "a\nb\nc\n"
        );
    }
}
