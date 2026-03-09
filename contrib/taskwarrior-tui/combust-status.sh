#!/usr/bin/env bash
set -euo pipefail

uuid="$1"

project="$(task "$uuid" export | python3 -c "
import json, sys
task = json.load(sys.stdin)[0]
print(task.get('project', ''))
")"

dir="$HOME/src/combust/$(echo "$project" | tr '[:upper:]' '[:lower:]')"

if [ ! -d "$dir" ]; then
    echo "Error: directory $dir does not exist" >&2
    read -n 1 -s -r -p "Press any key to continue..."
    exit 1
fi

cd "$dir"
combust status
read -n 1 -s -r -p "Press any key to continue..."
