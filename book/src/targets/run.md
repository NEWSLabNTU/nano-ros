# Running an Image: `nros run`

<!-- type: how-to · level: 2 -->

`nros run` builds the image if it is stale, starts what its link needs, and
streams the console, for host images and emulators. `nros run --expect
'<line>'` exits successfully when that line appears, which makes it a
one-line smoke test. On a real board it refuses and names the flashing step
instead.

*This page is coming soon.* Until then, see `nros run` in the
[`nros` CLI reference](../reference/cli.md) and
[Deployment Workflow](../guide/deploy/deploy-to-production.md).

## Design background

- [RFC-0118: target execution model](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0118-target-execution-model.md)
