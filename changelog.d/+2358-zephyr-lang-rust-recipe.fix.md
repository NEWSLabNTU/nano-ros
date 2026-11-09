`nros setup board <zephyr-board> --zephyr-workspace` now tells a Rust user how
to add `zephyr-lang-rust` with a west `projects:` entry at the revision
nano-ros pins for the board's Zephyr line. It used to name a per-board
fragment that did not exist, by an absolute import path west refuses.
`nros build` of a Rust Zephyr image whose workspace lacks the module now stops
before west runs and names the module, instead of failing on a Kconfig warning.
