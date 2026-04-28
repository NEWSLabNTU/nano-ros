fn main() {
    println!("cargo:rerun-if-env-changed=NUTTX_DIR");
}
