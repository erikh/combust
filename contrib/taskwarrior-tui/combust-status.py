#!/usr/bin/env python3

import json
import os
import subprocess
import sys

uuid = sys.argv[1]

task = json.loads(subprocess.check_output(["task", uuid, "export"]))[0]
project = task.get("project", "")

dir = os.path.join(os.path.expanduser("~"), "src", "combust", project.lower())

if not os.path.isdir(dir):
    print(f"Error: directory {dir} does not exist", file=sys.stderr)
    input("Press any key to continue...")
    sys.exit(1)

os.chdir(dir)
subprocess.run(["combust", "status"], check=True)
input("Press any key to continue...")
