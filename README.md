# combust

> **Alpha software.** Combust is under active development and may drift heavily over time. APIs, commands, and file formats are subject to change without notice.

Combust is a rebuild of [hydra](https://github.com/erikh/hydra) in Rust, designed for future expansion. Hydra is no longer maintained and will be deleted soon, along with this message.

Combust turns markdown design documents into branches, code, and commits. It assembles context from your design docs, hands it to Claude, runs tests and linting, and pushes a branch ready for your review.

You don't need a CI system, VM infrastructure, web interfaces, or pull requests. Combust works entirely on your local machine: you write the spec, combust does the rest.

## Install

From source:

```sh
cargo install --path .
```

## Quick start

```sh
# Initialize a combust project in the current git repo
combust init

# Create a design task
combust edit add-auth

# Run the task — combust hands it to Claude, runs tests, commits, and pushes
combust run add-auth

# Check what's running
combust status
```

## Design directory structure

After `combust init`, a `.combust/` directory is created:

```
.combust/
├── config.json        # Project configuration
├── design/
│   ├── rules.md       # Global rules for Claude
│   ├── lint.md        # Lint/formatting rules
│   ├── functional.md  # Functional specification (ground truth)
│   ├── combust.yml    # Test/lint/clean commands
│   ├── tasks/         # Pending task files (markdown)
│   ├── state/         # Task state directories (review, merge, completed, abandoned)
│   ├── milestones/    # Milestone definitions
│   └── other/         # Supplemental design files
└── work/              # Git worktrees for in-progress tasks
```

## Command reference

### Project setup

| Command                  | Description                                                         |
| ------------------------ | ------------------------------------------------------------------- |
| `combust init`                  | Initialize a combust project                                        |
| `combust init <url> [dir]`      | Clone a repo and initialize (optional directory name, like git)     |
| `combust init --private`        | Store design data in `~/.local/share/combust/` (symlink `.combust`) |
| `combust init --tmux`           | Also copy `Makefile.tmux` for tmux-based parallel task execution    |

### Task lifecycle

| Command                      | Description                                                    |
| ---------------------------- | -------------------------------------------------------------- |
| `combust edit <task>`        | Create or edit a task's markdown spec                          |
| `combust add <task>`         | Add a task from standard input                                 |
| `combust run <task>`         | Execute a task — Claude implements, tests, commits, and pushes |
| `combust review list`        | List tasks in review                                           |
| `combust review view <task>` | View a task under review                                       |
| `combust review diff <task>` | Show the diff for a reviewed task                              |
| `combust review edit <task>` | Edit a task's spec during review                               |
| `combust review run <task>`  | Re-run a review session                                        |
| `combust review rm <task>`   | Abandon a task from review                                     |
| `combust test <task>`        | Add tests for a task in review                                 |
| `combust merge list`         | List tasks ready to merge                                      |
| `combust merge run <task>`   | Merge a task branch into the main branch                       |
| `combust merge rm <task>`    | Abandon a task from merge                                      |
| `combust reconcile`          | Merge completed task specs into `functional.md`                |
| `combust verify`             | Verify all `functional.md` requirements against the codebase   |

### Task organization

| Command                       | Description                                   |
| ----------------------------- | --------------------------------------------- |
| `combust list`                | List pending tasks                            |
| `combust status`              | Show all task states and running tasks (YAML) |
| `combust status -j`           | Output as JSON                                |
| `combust status -a`           | Include empty states in output                |
| `combust group list`          | List task groups                              |
| `combust group tasks <group>` | List tasks in a group                         |
| `combust group run <group>`   | Run all pending tasks in a group              |
| `combust group merge <group>` | Merge all tasks in a group                    |

### Design files

| Command                     | Description                    |
| --------------------------- | ------------------------------ |
| `combust other list`        | List supplemental design files |
| `combust other add <file>`  | Add a file to `other/`         |
| `combust other view <file>` | View a supplemental file       |
| `combust other edit <file>` | Edit a supplemental file       |
| `combust other rm <file>`   | Remove a supplemental file     |

### Milestones

| Command                            | Description                               |
| ---------------------------------- | ----------------------------------------- |
| `combust milestone new <name>`     | Create a new milestone                    |
| `combust milestone list`           | List milestones                           |
| `combust milestone view <name>`    | View a milestone                          |
| `combust milestone edit <name>`    | Edit a milestone                          |
| `combust milestone verify <name>`  | Verify milestone promises                 |
| `combust milestone repair <name>`  | Create missing task files for a milestone |
| `combust milestone deliver <name>` | Mark a milestone as delivered             |
| `combust milestone history <name>` | View delivery history                     |

### Utilities

| Command                        | Description                                                      |
| ------------------------------ | ---------------------------------------------------------------- |
| `combust fix`                  | Scan for and fix project issues                                  |
| `combust fix -y`               | Auto-confirm fixes                                               |
| `combust clean <task>`         | Run the clean command in a task's worktree                       |
| `combust sync`                 | Import issues from GitHub/Gitea as tasks                         |
| `combust sync --label <label>` | Filter imported issues by label                                  |
| `combust notify <message>`     | Send a desktop notification                                      |
| `combust completion <shell>`   | Generate shell completions (bash, zsh, fish, elvish, powershell) |

### Common flags

Most commands that invoke Claude accept these flags:

| Flag                     | Description                                   |
| ------------------------ | --------------------------------------------- |
| `--model <name>`         | Override the Claude model                     |
| `-Y`, `--no-auto-accept` | Disable auto-accept for tool calls            |
| `-N`, `--no-notify`      | Disable notifications                         |
| `-T`, `--tui`            | Force built-in TUI instead of Claude Code CLI |

## Configuration

Edit `.combust/config.json`:

```json
{
    "source_repo_url": "https://github.com/you/repo",
    "private": false,
    "theme": "base16-ocean.dark"
}
```

The `theme` field controls syntax highlighting for status output. Available themes are the syntect built-in set.

## Workflow integrations

The [contrib/](contrib/) directory contains additional tools for integrating combust into your workflow, including tmux parallel execution and taskwarrior-tui shortcuts. See [contrib/README.md](contrib/README.md) for details.

## License

See [LICENSE](LICENSE) for details.
