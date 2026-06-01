//! Byte-identical golden test. Every `<pkg>__<Name>.msg` under
//! `tests/fixtures/` MUST produce the matching `<pkg>__<Name>.idl`
//! string emitted by `scripts/cyclonedds/msg_to_cyclone_idl.py`.

use std::{fs, path::PathBuf};

use nros_msg_to_idl::Converter;

#[derive(Debug)]
struct Fixture {
    package: String,
    message: String,
    msg_path: PathBuf,
    idl_path: PathBuf,
}

fn collect_fixtures() -> Vec<Fixture> {
    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut out = Vec::new();
    for entry in fs::read_dir(&fixtures_dir).expect("fixtures dir present") {
        let entry = entry.expect("readdir entry");
        let path = entry.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if path.extension().and_then(|s| s.to_str()) != Some("msg") {
            continue;
        }
        let (package, message) = stem
            .split_once("__")
            .expect("fixture name must follow <pkg>__<Msg> shape");
        let idl_path = fixtures_dir.join(format!("{stem}.idl"));
        assert!(idl_path.is_file(), "missing golden {}", idl_path.display());
        out.push(Fixture {
            package: package.to_string(),
            message: message.to_string(),
            msg_path: path,
            idl_path,
        });
    }
    out.sort_by(|a, b| a.msg_path.cmp(&b.msg_path));
    out
}

#[test]
fn at_least_five_fixtures_present() {
    let n = collect_fixtures().len();
    assert!(
        n >= 5,
        "expected ≥5 golden fixture pairs under tests/fixtures/, found {n}"
    );
}

#[test]
fn golden_byte_identical() {
    let fixtures = collect_fixtures();
    assert!(!fixtures.is_empty(), "no golden fixtures found");

    let mut failures: Vec<String> = Vec::new();
    let mut passes = 0;
    for fx in &fixtures {
        let src = fs::read_to_string(&fx.msg_path).expect("read .msg");
        let expected = fs::read_to_string(&fx.idl_path).expect("read .idl");
        let actual = match Converter::new(&fx.package, &fx.message).convert(&src) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{}/{}: convert error: {e}", fx.package, fx.message));
                continue;
            }
        };
        if actual != expected {
            failures.push(diff_summary(&fx.package, &fx.message, &expected, &actual));
        } else {
            passes += 1;
        }
    }

    if !failures.is_empty() {
        panic!(
            "{} of {} fixtures matched; failures:\n{}",
            passes,
            fixtures.len(),
            failures.join("\n--------\n")
        );
    }
    eprintln!("[golden] {passes}/{} fixtures matched", fixtures.len());
}

fn diff_summary(pkg: &str, msg: &str, expected: &str, actual: &str) -> String {
    let mut out = format!("{pkg}/{msg}: byte-mismatch.\n");
    let e_lines: Vec<&str> = expected.split('\n').collect();
    let a_lines: Vec<&str> = actual.split('\n').collect();
    let n = e_lines.len().max(a_lines.len());
    for i in 0..n {
        let el = e_lines.get(i).copied().unwrap_or("<eof>");
        let al = a_lines.get(i).copied().unwrap_or("<eof>");
        if el != al {
            out.push_str(&format!(
                "  line {}:\n    expected: {el:?}\n    actual:   {al:?}\n",
                i + 1
            ));
        }
    }
    out
}
