#!/usr/bin/env bash
set -euo pipefail

uuid="$1"

eval "$(task "$uuid" export | python3 -c "
import json, sys, re
task = json.load(sys.stdin)[0]
project = task.get('project', '')
desc = task.get('description', '')
words = re.split(r'[\s_\-/]+', desc.strip())[:2]
name = '_'.join(w.lower() for w in words if w)
print(f'project={project}')
print(f'name={name}')
")"

dir="$HOME/src/combust/$(echo "$project" | tr '[:upper:]' '[:lower:]')"

if [ ! -d "$dir" ]; then
    echo "Error: directory $dir does not exist" >&2
    read -n 1 -s -r -p "Press any key to continue..."
    exit 1
fi

cd "$dir"
combust run "$name"
