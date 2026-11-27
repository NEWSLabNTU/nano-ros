`just setup-worktree`, `just post-rebase`, the provisioning preflight and the
submodule drift check no longer report a submodule as checked out when its
checkout never happened: a HEAD that names no commit, a missing or empty index.
Each now reads as an incomplete checkout, says which of these it is, and names a
repair that works for it. It used to print "already checked out" over an empty
directory, and every gate that builds from that submodule failed later.
