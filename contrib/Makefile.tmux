# Makefile.tmux — tmux helpers for running combust tasks in parallel
#
# Combust sets the xterm title for each command it runs, which tmux picks up
# automatically as the window name. This Makefile takes advantage of that by
# spawning a new tmux window per task so you can watch every task's progress
# in your tmux status bar at a glance.
#
# Usage:
#   Copy this file into your combust project directory (next to .combust/) and run
#   targets from inside a tmux session:
#
#     cp contrib/Makefile.tmux ./Makefile    # or: make -f contrib/Makefile.tmux run-all
#
#     make run-all       # spawn a tmux window for every pending task
#     make review-all    # spawn a tmux window for every task in review state
#     make merge-all     # spawn a tmux window for every task in merge state
#     make test-all      # spawn a tmux window for every task in review state (test mode)
#
#   Each window is named with a prefix (run:, review:, merge:, test:) followed
#   by the task name, making it easy to jump between tasks with tmux's window
#   switcher (prefix + w).

SHELL := /bin/bash

# Spawn a tmux window for each task in merge state and run combust merge run.
merge-all:
	@tasks=$$(combust review list 2>/dev/null); \
	if [ -z "$$tasks" ]; then \
		echo "No tasks in merge state."; \
		exit 0; \
	fi; \
	echo "$$tasks" | while read -r task; do \
		echo "Spawning merge window for: $$task"; \
		tmux new-window -n "merge:$$task" "combust merge run $$task; echo 'Press enter to close'; read"; \
	done

# Spawn a tmux window for each task in review state and run combust review run.
review-all:
	@tasks=$$(combust review list 2>/dev/null); \
	if [ -z "$$tasks" ]; then \
		echo "No tasks in review state."; \
		exit 0; \
	fi; \
	echo "$$tasks" | while read -r task; do \
		echo "Spawning review window for: $$task"; \
		tmux new-window -n "review:$$task" "combust review run $$task; echo 'Press enter to close'; read"; \
	done

# Spawn a tmux window for each pending task and run combust run.
run-all:
	@tasks=$$(combust list 2>/dev/null); \
	if [ -z "$$tasks" ]; then \
		echo "No pending tasks."; \
		exit 0; \
	fi; \
	echo "$$tasks" | while read -r task; do \
		echo "Spawning run window for: $$task"; \
		tmux new-window -n "run:$$task" "combust run $$task; echo 'Press enter to close'; read"; \
	done

# Spawn a tmux window for each task in review state and run combust test.
test-all:
	@tasks=$$(combust review list 2>/dev/null); \
	if [ -z "$$tasks" ]; then \
		echo "No tasks in test state."; \
		exit 0; \
	fi; \
	echo "$$tasks" | while read -r task; do \
		echo "Spawning test window for: $$task"; \
		tmux new-window -n "test:$$task" "combust test $$task; echo 'Press enter to close'; read"; \
	done

.PHONY: merge-all review-all run-all test-all
