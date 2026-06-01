use nros_build::find_nros_binary;

#[test]
fn missing_nros_binary_returns_error_with_install_pointer() {
    let tmp = tempfile::tempdir().unwrap();

    let original_path = std::env::var_os("PATH");
    let original_home = std::env::var_os("HOME");
    let original_nros_home = std::env::var_os("NROS_HOME");
    let original_nros_bin = std::env::var_os("NROS_BIN");

    // SAFETY: this integration-test binary owns its env exclusively.
    unsafe {
        std::env::remove_var("NROS_BIN");
        std::env::set_var("PATH", tmp.path()); // empty dir, no `nros`
        std::env::set_var("HOME", tmp.path()); // no ~/.nros/bin/nros either
        std::env::remove_var("NROS_HOME");
    }

    let err = find_nros_binary().unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("install-nros.sh") || msg.to_lowercase().contains("install"),
        "expected install pointer in error: {msg}"
    );

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
        }
    }
}
