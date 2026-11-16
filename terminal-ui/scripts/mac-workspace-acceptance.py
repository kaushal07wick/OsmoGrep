"""Test native companion splits in a dedicated Ghostty window; no device boot or input."""
import argparse
import json
import shlex
import subprocess
import tempfile
import time
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
source = Path(__file__).resolve().parents[2]
fixture = Path(tempfile.mkdtemp(prefix="osmogrep-native-panes-"))
result = {"scope": "Dedicated Ghostty window; actual companion missing-target errors; no OS download or device control"}


def apple(script, *arguments):
    return subprocess.check_output(
        ["/usr/bin/osascript", "-e", script, *map(str, arguments)],
        text=True, stderr=subprocess.PIPE, timeout=30,
    ).strip()


def wait_for(predicate):
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(0.1)
    raise AssertionError("Timed out waiting for the isolated terminal pane")


count_script = '''on run argv
 tell application "Ghostty"
  return count of terminals of window id (item 1 of argv)
 end tell
end run'''
close_script = '''on run argv
 tell application "Ghostty"
  close window (window id (item 1 of argv))
 end tell
end run'''
create_script = '''on run argv
 tell application "Ghostty"
  set settings to new surface configuration
  set initial working directory of settings to item 1 of argv
  set command of settings to item 2 of argv
  set wait after command of settings to false
  set testWindow to new window with configuration settings
  return id of testWindow
 end tell
end run'''
owner_command = "sh -c " + shlex.quote(
    f"tty > {shlex.quote(str(fixture / 'owner-tty'))}; "
    "printf 'OsmoGrep native pane acceptance — agent side remains visible\\n'; "
    f"while [ ! -e {shlex.quote(str(fixture / 'finish'))} ]; do sleep .2; done"
)
window = apple(create_script, fixture, owner_command)
try:
    wait_for(lambda: (fixture / "owner-tty").exists())
    tty = (fixture / "owner-tty").read_text().strip()
    for index, device in enumerate(["mac", "phone"], start=2):
        program = source.parent / ("terminal-" + device) / "dist" / ("terminal-" + device)
        assert program.is_file(), program
        log = fixture / (device + ".log")
        # An intentionally absent target verifies error visibility without starting a VM or simulator.
        name = "osmogrep-absent-" + fixture.name.rsplit("-", 1)[-1]
        shell = (
            f"env -u TMUX -u TMUX_PANE {shlex.quote(str(program))} view {name} --protocol kitty "
            f"> {shlex.quote(str(log))} 2>&1; "
            f"cat {shlex.quote(str(log))}; "
            f"while [ ! -e {shlex.quote(str(fixture / 'finish'))} ]; do sleep .2; done"
        )
        subprocess.run(
            ["/usr/bin/osascript", "-l", "JavaScript", str(source / "src/workspace/ghostty-split.js"),
             tty, str(fixture), "GSrt", "sh -c " + shlex.quote(shell)],
            check=True, capture_output=True, text=True, timeout=30,
        )
        wait_for(lambda: int(apple(count_script, window)) == index)
        wait_for(lambda: log.exists() and bool(log.read_text()))
        result[device + "_viewer_opens_beside_owner"] = True
        result[device + "_missing_target_report"] = log.read_text()[:2000]
    result["owner_and_two_companions_visible"] = int(apple(count_script, window)) == 3
finally:
    (fixture / "finish").touch()
    time.sleep(0.5)
    try:
        apple(close_script, window)
    except subprocess.CalledProcessError:
        pass  # The dedicated window may already have closed when its three commands exited.
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps(result, indent=2))
