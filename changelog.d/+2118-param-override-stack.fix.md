An argv parameter override (`-p` / `--params-file`) is typed into a slot the
caller holds instead of being returned, so declaring an overridden parameter
and listing overrides each hold one parameter value on the stack instead of
two (17 KiB to under 9 KiB per frame on Cortex-M3, issue 2118).
