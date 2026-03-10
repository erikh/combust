#!/bin/python3

import json
import os
import subprocess
import sys
import tty
import termios

def wait_for_keypress():
    fd = os.open("/dev/tty", os.O_RDONLY)
    old = termios.tcgetattr(fd)
    try:
        tty.setraw(fd)
        os.read(fd, 1)
    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, old)
        os.close(fd)

try:
    uuid = sys.argv[1]

    task = json.loads(subprocess.check_output(["task", uuid, "export"]))[0]
    project = task.get("project", "")

    dir = os.path.join(os.path.expanduser("~"), "src", "combust", project.lower())

    if not os.path.isdir(dir):
        print(f"Error: directory {dir} does not exist", file=sys.stderr)
        sys.exit(1)

    os.chdir(dir)
    subprocess.run(["combust", "status", "-a"], check=True)
except Exception as e:
    print(f"\nError: {e}", file=sys.stderr)
finally:
    print("Press any key to continue...", end="", flush=True)
    wait_for_keypress()
