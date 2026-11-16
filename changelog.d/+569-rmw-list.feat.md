`nros rmw list` and `nros rmw show <name>` list the RMW backends: each one's
summary, what `nros run` starts on the host for it, the host packages
`nros setup` installs, and the boards a test runs it on (or only builds it
for). Both take `--json`. `nros new --rmw` now accepts the same names, aliases
included, and refuses an unknown one by listing them.
