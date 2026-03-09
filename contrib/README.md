# contrib

> **Alpha software.** Combust is under active development and may drift heavily over time. APIs, commands, and file formats are subject to change without notice.

Community-contributed helpers and integrations for Combust.

## Makefile.tmux

A Makefile for running combust tasks in parallel using tmux windows. Each target spawns a new tmux window per task, letting you monitor all running tasks from your tmux status bar.

Combust sets the xterm title during every command it runs. Tmux automatically picks this up as the window name, so each window in your session is labeled with what combust is doing (e.g. `run:add-feature`, `merge:fix-bug`).

### Installation

Pass `--tmux` when initializing a project to copy `Makefile.tmux` into your repository automatically:

```sh
combust init --tmux
```

Or copy it manually:

```sh
cp contrib/Makefile.tmux ./Makefile.tmux
```

### Targets

| Target | Description |
|--------|-------------|
| `run-all` | Spawn a window for every pending task and run `combust run` |
| `review-all` | Spawn a window for every review-state task and run `combust review run` |
| `merge-all` | Spawn a window for every review/merge-state task and run `combust merge run` |
| `test-all` | Spawn a window for every review-state task and run `combust test` |

### Usage

From inside a tmux session:

```sh
make -f Makefile.tmux run-all
```
