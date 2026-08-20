//! issue 0727 — compile the weak host log stubs (see c/host_log_stub.c for
//! the full account). Host-only: a cross build must NOT get a fallback — an
//! embedded image missing its platform port has to fail at link.

fn main() {
    println!("cargo:rerun-if-changed=c/host_log_stub.c");
    let target = std::env::var("TARGET").unwrap_or_default();
    let host = std::env::var("HOST").unwrap_or_default();
    if target == host {
        cc::Build::new()
            .file("c/host_log_stub.c")
            .compile("nros_log_host_stub");
    }
}
