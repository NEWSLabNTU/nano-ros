# Rust API

<!-- type: reference · level: 2 -->

The nano-ros Rust API is the **`nros`** crate. It re-exports every type a
user application needs: `Executor`, `Node`, `Publisher<M>`,
`Subscription<M>`, `Service<S>`, `Client<S>`, `ActionServer<A>`,
`ActionClient<A>`, `Timer` and `GuardCondition`, with `nros::prelude` as the
glob import. It is `no_std`, and its heap use is behind the `alloc` feature,
so the same node code builds for a host and for a board. Message types
are generated crates, one per interface package your node depends on
([Message Generation](message-generation.md)).

## API reference

> [**`nros` rustdoc**](../api/rust/nros/index.html) — start here.

The published rustdoc is built from the current `main` branch with the
`alloc`, `log`, `env`, `rmw-cffi` and `ros-humble` features on.

## Where to start

- New to nano-ros? → [`nros::prelude`](../api/rust/nros/prelude/index.html)
- Executor-driven app? → [`nros::Executor`](../api/rust/nros/struct.Executor.html)
- Polling or callbacks? → [Two ways to receive](../background/execution-model.md#two-ways-to-receive),
  the two layers of the API and when to use each.

## How to read the rustdoc

The published pages show the crate with the features listed above turned on.
A board build turns on only the features its image asks for, so an item
behind a feature your image does not enable is absent from that build even
though the published page shows it; the item's source, linked from its page,
shows the `#[cfg(feature = ...)]` that gates it. Your own package documents
`nros` at the version and features it builds with: run `cargo doc --open` in
that package. That is also where your message crates are documented: they
are generated for your workspace, so the published site does not have them.

## Porting only

These crates are not part of the user API. They surface only when you write a
custom RMW backend or port nano-ros to a new platform; see
[Custom RMW Backend](../extending/custom-rmw.md) and
[Add a platform](../extending/add-a-platform.md).

- [`nros_rmw`](../api/rust/nros_rmw/index.html): the RMW trait surface.
- [`nros_platform_api`](../api/rust/nros_platform_api/index.html): the platform abstraction traits.
- [`nros-rmw-zenoh`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/rmw/zenoh/nros-rmw-zenoh): the zenoh-pico backend, in the source tree.
