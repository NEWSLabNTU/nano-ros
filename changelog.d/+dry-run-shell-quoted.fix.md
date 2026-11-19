`nros build --dry-run`, and every error message that shows a command, now
shell-quotes any word that needs it, so the printed line pastes into a shell
as the same command. Words with no spaces or shell characters print as before.
