"""Exercise the real Neovim workspace over RPC in an isolated fixture; no model calls."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser()
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
source = Path(__file__).resolve().parents[2]
root = Path(tempfile.mkdtemp(prefix='osmo-editor-')).resolve()
(root / 'src').mkdir()
(root / 'README.md').write_text('Editor fixture.\n')
live = root / 'src/live.py'
manual = root / 'src/manual.py'
live.write_text('first = 1\nsecond = 2\n')
manual.write_text('manual = 1\n')
bridge = root / 'focus.json'
socket = root / 'editor.sock'
script = (source / 'src/editor.lua').read_text() + '\n' + (source / 'src/editor/workspace.lua').read_text()
env = dict(os.environ, OSMOGREP_EDITOR_BRIDGE=str(bridge), NVIM_LOG_FILE=str(root / 'nvim.log'))
log = (root / 'output.log').open('w')
process = subprocess.Popen(['nvim', '--headless', '--clean', '-i', 'NONE', '--listen', str(socket), '--cmd', 'lua ' + script, 'README.md'], cwd=root, env=env, stdout=log, stderr=log)
checks = []


def expression(text):
    return subprocess.check_output(['nvim', '--server', str(socket), '--remote-expr', text], text=True, stderr=subprocess.PIPE, timeout=5).strip()


def lua(text):
    return json.loads(expression("luaeval('vim.json.encode((function() " + text.replace("'", "''") + " end)())')"))


def wait(predicate):
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        try:
            if predicate():
                return
        except subprocess.CalledProcessError:
            pass
        time.sleep(.05)
    raise AssertionError((root / 'output.log').read_text())


def state():
    return lua('return osmogrep_workspace.status()')


def focus(sequence):
    temporary = bridge.with_suffix('.next')
    temporary.write_text(json.dumps({'path': str(live), 'line': 2, 'phase': 'edited', 'sequence': sequence}))
    temporary.replace(bridge)


try:
    wait(lambda: socket.exists() and state().get('tree'))
    assert lua('return vim.api.nvim_buf_get_lines(vim.api.nvim_win_get_buf(osmogrep_workspace.status().tree),0,-1,false)')
    checks.append('compact_project_tree_is_a_separate_neovim_window')
    focus('initial')
    wait(lambda: state().get('live') and lua('return vim.api.nvim_buf_get_name(vim.api.nvim_win_get_buf(osmogrep_workspace.status().main))') == str(live))
    checks.append('live_file_opens_and_ticker_records_its_line')
    assert state()['line'] == 2
    lua('osmogrep_workspace.open_path(' + json.dumps(str(manual)) + '); return true')
    assert not state()['following']
    assert lua('return osmogrep_workspace.tabs()').count('.py') == 2
    live.write_text('first = 1\nsecond = 3\n')
    focus('while-inspecting')
    time.sleep(.6)
    assert lua('return vim.api.nvim_buf_get_name(0)') == str(manual)
    checks.append('manual_tabs_stay_open_and_following_does_not_steal_focus')
    lua('osmogrep_workspace.jump_click(); return true')
    assert lua('return vim.api.nvim_get_current_line()') == 'second = 3'
    checks.append('ticker_jump_returns_to_the_latest_saved_live_file')
    lua('osmogrep_workspace.open_path(' + json.dumps(str(manual)) + '); osmogrep_workspace.split(); return true')
    assert state()['inspection']
    assert lua('return vim.api.nvim_buf_get_name(vim.api.nvim_win_get_buf(osmogrep_workspace.status().main))') == str(live)
    assert lua('return vim.api.nvim_buf_get_name(vim.api.nvim_win_get_buf(osmogrep_workspace.status().inspection))') == str(manual)
    lua("vim.api.nvim_buf_set_lines(0,0,-1,false,{'UNSAVED MANUAL'}); return true")
    inspection_window = lua('return vim.api.nvim_get_current_win()')
    live.write_text('first = 1\nsecond = 4\n')
    focus('parallel-window')
    wait(lambda: lua('return vim.api.nvim_buf_get_lines(osmogrep_workspace.status().live,1,2,false)[1]') == 'second = 4')
    assert lua('return vim.api.nvim_get_current_win()') == inspection_window
    assert lua('return vim.api.nvim_buf_get_lines(0,0,1,false)[1]') == 'UNSAVED MANUAL'
    checks.append('live_updates_continue_beside_unsaved_inspection_without_stealing_focus')
    lua('vim.api.nvim_set_current_win(osmogrep_workspace.status().main); vim.api.nvim_buf_set_lines(0,0,-1,false,{"UNSAVED LIVE"}); return true')
    live.write_text('EXTERNAL SAVE\n')
    focus('protect-live')
    time.sleep(.6)
    assert lua('return vim.api.nvim_buf_get_lines(0,0,1,false)[1]') == 'UNSAVED LIVE'
    checks.append('agent_follow_never_overwrites_an_unsaved_live_buffer')
finally:
    if process.poll() is None:
        try:
            expression("execute('qall!')")
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired):
            pass
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
    log.close()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps({'checks': checks, 'scope': 'Real headless Neovim, isolated files and RPC; no model or device calls', 'diagnostics': str(root / 'output.log')}, indent=2) + '\n')
print(json.dumps({'passed': len(checks), 'checks': checks}, indent=2))
