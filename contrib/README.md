# contrib

> **Alpha software.** Combust is under active development and may drift heavily over time. APIs, commands, and file formats are subject to change without notice.

Community-contributed helpers and integrations for Combust.

## Contents

| Integration | Description |
|-------------|-------------|
| [Makefile.tmux](#makefiletmux) | Run combust tasks in parallel tmux windows |
| [taskwarrior-tui](#taskwarrior-tui) | Drive combust from taskwarrior-tui shortcuts |

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

## taskwarrior-tui

Shortcut scripts that let you drive combust directly from [taskwarrior-tui](https://github.com/kdheepak/taskwarrior-tui). Each script receives a task UUID from taskwarrior-tui, extracts the project and task name, then runs the corresponding combust command.

| Script | Shortcut | Description |
|--------|----------|-------------|
| `combust-edit.py` | `1` | Open the task spec in your editor and mark the combust UDA |
| `combust-run.py` | `2` | Run the task with combust |
| `combust-status.py` | `3` | Show combust status for the task's project |

The scripts derive the combust task name from the first two words of the taskwarrior description (lowercased, joined with `_`), and locate the project directory at `~/src/combust/<project>` using the taskwarrior `project` field.

### Installation

Copy the scripts to your taskwarrior-tui shortcut directory:

```sh
mkdir -p ~/.config/taskwarrior-tui/shortcut-scripts
cp contrib/taskwarrior-tui/*.py ~/.config/taskwarrior-tui/shortcut-scripts/
chmod +x ~/.config/taskwarrior-tui/shortcut-scripts/*.py
```

### Configuration

Add the following to your `~/.taskrc` to register the shortcut scripts and set up the combust UDA:

```ini
# Combust UDA — tracks whether a task has a combust spec
uda.combust.type=string
uda.combust.label=Combust
uda.combust.values=true
uda.combust.default=
color.uda.combust.true=color2

# Show the combust column in the next report
report.next.columns=id,start.age,entry.age,depends,priority,project,tags,recur,scheduled.countdown,due.relative,until.remaining,description,urgency,combust
report.next.labels=ID,Active,Age,Deps,P,Project,Tag,Recur,S,Due,Until,Description,Urg,Combust

# Register shortcut scripts (triggered by pressing 1, 2, 3 in taskwarrior-tui)
uda.taskwarrior-tui.shortcuts.1=~/.config/taskwarrior-tui/shortcut-scripts/combust-edit.py
uda.taskwarrior-tui.shortcuts.2=~/.config/taskwarrior-tui/shortcut-scripts/combust-run.py
uda.taskwarrior-tui.shortcuts.3=~/.config/taskwarrior-tui/shortcut-scripts/combust-status.py
```

With this in place, select a task in taskwarrior-tui and press `1` to edit its combust spec, `2` to run it, or `3` to check status.
