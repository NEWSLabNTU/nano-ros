`nros knob list` and `nros knob show <knob>` list the build-time knobs a
project can state: each one's scope, default and purpose, and for one knob
where to state it in `system.toml` (and in a board's `[knobs]`), the values
it takes and its help. A default that depends on the image is shown with
each case. `nros knob list --platform <p>` adds the value each knob resolves
to on that platform and which rung set it. Both take `--json`.
