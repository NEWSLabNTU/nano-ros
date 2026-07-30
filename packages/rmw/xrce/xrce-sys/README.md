> **phase-321 W1.d — this directory is a SUBMODULE HOST, not a crate.**
> The `xrce-sys` crate (701 LoC + a 307-line build.rs) had zero dependents and
> was `--exclude`d from every workspace build; it was deleted. What remains is
> `micro-xrce-dds-client/` and `micro-cdr/`, the two vendored submodules that
> `nros-rmw-xrce-cffi/build.rs` and `nros-rmw-xrce/CMakeLists.txt` compile from
> (see `nros-sdk-index.toml`). Do not re-add a crate here.

# xrce-sys

FFI bindings + bundled C source for [Micro-XRCE-DDS-Client](https://github.com/eProsima/Micro-XRCE-DDS-Client) and Micro-CDR. Used by `nros-rmw-xrce`.

## License

Licensed under either of [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT](https://opensource.org/licenses/MIT) at your option (unless the crate header says otherwise — `nros`, `nros-c`, `nros-cpp`, `nros-sizes-build`, `zpico-alloc` are Apache-2.0 only).

Part of the [nano-ros](https://github.com/NEWSLabNTU/nano-ros) project.
