# Changelog

## 0.1.1

- Replace `--print` with `--permission-mode plan` for Claude CLI invocation
- Remove `--no-plan` / `-P` flag (plan mode is now always enabled via `--permission-mode`)
- Add `combust show <task>` command to display a task's content regardless of state
- Add `combust add <task>` command to create tasks from standard input
- Support optional directory argument for `combust init <url> [dir]` (like `git clone`)
- Update taskwarrior-tui shortcut scripts to use task IDs instead of derived names
- Run tasks in tmux windows from `combust-run.py` shortcut
- Add error logging to shortcut scripts
- Add `combust-show.py` shortcut for taskwarrior-tui
- Add `install-shortcuts` Makefile target

## 0.1.0

- Initial release
