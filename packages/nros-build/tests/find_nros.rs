use nros_build::find_nros_binary;
use std::{fs, os::unix::fs::PermissionsExt};

/// Helper: build an executable stub at `path`.
fn make_stub(path: &std::path::Path) {
    fs::write(path, "#!/bin/sh\necho stub\n").unwrap();
    let mut perm = fs::metadata(path).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(path, perm).unwrap();
}

#[test]
fn find_nros_resolves_via_env_var() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = tmp.path().join("nros");
    make_stub(&stub);

    // Isolate the environment so PATH / HOME don't leak a real binary in.
    let original_path = std::env::var_os("PATH");
    let original_home = std::env::var_os("HOME");
    let original_nros_home = std::env::var_os("NROS_HOME");
    let original_nros_bin = std::env::var_os("NROS_BIN");

    // SAFETY: tests in this file are not parallel with other env mutators
    // because each lives in its own integration-test binary.
    unsafe {
        std::env::set_var("NROS_BIN", &stub);
        std::env::set_var("PATH", "");
        std::env::set_var("HOME", tmp.path());
        std::env::remove_var("NROS_HOME");
    }

    let resolved = find_nros_binary().expect("env-var resolution");
    assert_eq!(resolved, stub);

    unsafe {
        if let Some(v) = original_path {
            std::env::set_var("PATH", v);
        } else {
            std::env::remove_var("PATH");
        }
        if let Some(v) = original_home {
            std::env::set_var("HOME", v);
        } else {
            std::env::remove_var("HOME");
        }
        if let Some(v) = original_nros_home {
            std::env::set_var("NROS_HOME", v);
        }
        if let Some(v) = original_nros_bin {
            std::env::set_var("NROS_BIN", v);
        } else {
            std::env::remove_var("NROS_BIN");
        }
    }
}
