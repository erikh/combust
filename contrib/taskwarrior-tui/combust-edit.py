#!/usr/bin/env python3

import json
import os
import re
import subprocess
import sys

uuid = sys.argv[1]

task = json.loads(subprocess.check_output(["task", uuid, "export"]))[0]
project = task.get("project", "")
desc = task.get("description", "")
words = re.split(r"[\s_\-/]+", desc.strip())[:2]
name = "_".join(w.lower() for w in words if w)

dir = os.path.join(os.path.expanduser("~"), "src", "combust", project.lower())

if not os.path.isdir(dir):
    print(f"Error: directory {dir} does not exist", file=sys.stderr)
    input("Press any key to continue...")
    sys.exit(1)

os.chdir(dir)
subprocess.run(["combust", "edit", name], check=True)
subprocess.run(
    ["task", "rc.confirmation=off", uuid, "modify", "combust:X"], check=True
)
