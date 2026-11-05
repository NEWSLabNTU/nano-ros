# Run Plans

<!-- type: how-to · level: 2 -->

Before it runs anything, `nros run` resolves a **run plan**: which launcher
or flasher, which console and which link, and where each fact came from. The
facts come from five levels, highest first: the command line, the image, your
machine's bench file, the board's descriptor, and the runner's defaults.
`--plan` prints the plan without running it.

*This page is coming soon.*

## Design background

- [RFC-0118: target execution model](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0118-target-execution-model.md)
