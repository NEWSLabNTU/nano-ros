`nros-codegen.toml` no longer accepts the storage-mode tokens phase-390
renamed. `mode = "owned"` and `mode = "borrowed"` parsed with a deprecation
warning for one release; they are now refused, and code generation stops with
an error naming the file, the field and the replacement:

| retired | write instead |
| --- | --- |
| `mode = "owned"` | `mode = "inline"` |
| `mode = "borrowed"` | `mode = "view"` |

A config that already uses `inline` / `heap` / `view` is unaffected. The
single-package `cargo nano-ros` path also stops on a config it cannot read,
where it previously fell back to built-in capacities without saying so.
