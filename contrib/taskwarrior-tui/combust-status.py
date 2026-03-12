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
    import traceback
    log = os.path.join(os.path.expanduser("~"), ".local", "share", "combust", "shortcut-errors.log")
    os.makedirs(os.path.dirname(log), exist_ok=True)
    with open(log, "a") as f:
        f.write(f"\n--- combust-status {sys.argv[1] if len(sys.argv) > 1 else '(no arg)'} ---\n")
        traceback.print_exc(file=f)
    print(f"\nError: {e}", file=sys.stderr)
    traceback.print_exc()
finally:
    print("Press any key to continue...", end="", flush=True)
    wait_for_keypress()
