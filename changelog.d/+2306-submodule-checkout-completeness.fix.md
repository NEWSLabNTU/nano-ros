The provisioning preflight, `just setup-worktree` and the submodule drift check
now read a submodule's working tree, not only its HEAD. An interrupted checkout
(tracked files missing or staged as deleted) or a checkout on an unrelated
branch is reported by path, with its repair, before any gate runs. It used to
read as provisioned and fail three gates later on a missing source file.
