"""Native Ghostty + real OsmoGrep/Neovim acceptance in a dedicated window; local fake model."""
import argparse
import http.server
import json
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
import threading
import time

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
root = Path(tempfile.mkdtemp(prefix='osmo-native-')).resolve()
(root / 'README.md').write_text('Native editor acceptance.\n')
(root / 'live.py').write_text('first = 1\nsecond = 2\n')
(root / 'manual.py').write_text('manual = True\n')
requests, failures, checks = [], [], []


class Model(http.server.BaseHTTPRequestHandler):
    def log_message(self, *unused):
        pass

    def do_POST(self):
        try:
            request = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            requests.append(request)
            if len(requests) == 2:
                message = {'role': 'assistant', 'content': 'Reading the requested file.', 'tool_calls': [
                    {'id': 'native-read', 'type': 'function', 'function': {'name': 'read_file', 'arguments': '{"path":"live.py"}'}}]}
                finish = 'tool_calls'
            else:
                message = {'role': 'assistant', 'content': 'NATIVE_GREETING_OK' if len(requests) == 1 else 'NATIVE_READ_OK'}
                finish = 'stop'
            body = json.dumps({'choices': [{'message': message, 'finish_reason': finish}]}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        except Exception as error:
            failures.append(repr(error))
            self.send_error(500)


server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Model)
threading.Thread(target=server.serve_forever, daemon=True).start()


def apple(script, *arguments):
    return subprocess.check_output(['/usr/bin/osascript', '-e', script, *map(str, arguments)], text=True, stderr=subprocess.PIPE, timeout=25).strip()


def wait(predicate):
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        try:
            if predicate():
                return
        except (subprocess.CalledProcessError, FileNotFoundError, json.JSONDecodeError):
            pass
        time.sleep(.1)
    diagnostic = None
    if socket:
        try:
            diagnostic = lua('{mode=vim.fn.mode(), error=vim.v.errmsg, buffers=vim.tbl_map(function(b) return {name=vim.api.nvim_buf_get_name(b),modified=vim.bo[b].modified,type=vim.bo[b].buftype} end,vim.api.nvim_list_bufs())}')
        except Exception as error:
            diagnostic = str(error)
    raise AssertionError({'neovim': diagnostic, 'checks': checks, 'requests': len(requests), 'failures': failures, 'fixture': str(root)})


create = '''on run argv
 tell application "Ghostty"
  set settings to new surface configuration
  set initial working directory of settings to item 1 of argv
  set command of settings to item 2 of argv
  set wait after command of settings to false
  return id of (new window with configuration settings)
 end tell
end run'''
count = '''on run argv
 tell application "Ghostty"
  return count of terminals of window id (item 1 of argv)
 end tell
end run'''
send = '''on run argv
 tell application "Ghostty"
  set targetPane to first terminal of window id (item 1 of argv)
  input text (item 2 of argv) to targetPane
  send key "enter" to targetPane
 end tell
end run'''
close = '''on run argv
 tell application "Ghostty"
  close window (window id (item 1 of argv))
 end tell
end run'''
environment = {
    'PATH': os.environ.get('PATH','/usr/bin:/bin:/opt/homebrew/bin'),
    'HOME': str(root / 'home'), 'XDG_CONFIG_HOME': str(root / 'config'),
    'OSMOGREP_DISABLE_UPDATE_CHECK': '1', 'OSMOGREP_PROVIDER': 'ollama',
    'OSMOGREP_MODEL': 'native-fixture', 'OSMOGREP_API': 'chat-completions',
    'OSMOGREP_BASE_URL': f'http://127.0.0.1:{server.server_port}/v1',
    'OLLAMA_API_KEY': 'fixture', 'OSMOGREP_NO_STREAM': '1', 'OSMOGREP_UI': 'typescript',
    'VLLM_REALTIME_AUTOCONNECT': '0',
}
Path(environment['HOME']).mkdir()
prefix = ' '.join(shlex.quote(key + '=' + value) for key, value in environment.items())
agent_command = (f'printf "%s" "$$" > {shlex.quote(str(root / "agent.pid"))}; '
    f'exec {shlex.quote(str(args.binary.resolve()))} --session "Native pane check"')
command = '/bin/sh -c ' + shlex.quote(
    f'env -u TMUX -u TMUX_PANE {prefix} /bin/sh -c {shlex.quote(agent_command)} '
    f'2>> {shlex.quote(str(root / "stderr.log"))} | /usr/bin/tee {shlex.quote(str(root / "terminal.ansi"))}'
)

window = apple(create, root, command)
socket = None


def rpc(expression):
    return subprocess.check_output(['nvim', '--server', str(socket), '--remote-expr', expression], text=True, stderr=subprocess.PIPE, timeout=5).strip()


def lua(expression):
    return json.loads(rpc("luaeval('vim.json.encode(" + expression.replace("'", "''") + ")')"))


def saved():
    paths = list(Path(environment['HOME']).rglob('sessions/*.json'))
    return json.loads(paths[0].read_text()) if paths else {}


try:
    wait(lambda: (root / 'agent.pid').exists())
    pid = int((root / 'agent.pid').read_text())
    time.sleep(2)
    apple(send, window, '/nv README.md')
    wait(lambda: int(apple(count, window)) == 2)
    directory = Path('/private/tmp') / f'osmogrep-editor-{os.getuid()}'
    wait(lambda: bool(list(directory.glob(f'{pid}-*.sock'))))
    socket = next(directory.glob(f'{pid}-*.sock'))
    wait(lambda: lua('osmogrep_workspace.status()').get('tree'))
    assert lua('vim.env.TMUX') is None
    os.kill(pid, 0)
    checks.append('native_editor_and_agent_share_a_window_without_tmux_or_restarting_chat')
    apple(send, window, 'hello')
    wait(lambda: saved().get('user_turns') == 1 and len(requests) == 1)
    checks.append('first_chat_turn_is_recorded_without_counting_the_editor_command')
    apple(send, window, 'Read live.py with read_file and summarize it.')
    wait(lambda: len(requests) >= 3 and saved().get('user_turns') == 2)
    wait(lambda: lua('vim.api.nvim_buf_get_name(osmogrep_workspace.status().live)') == str(root / 'live.py'))
    checks.append('actual_file_tool_focus_reaches_the_live_editor_and_second_turn_counter')
    rpc("luaeval('osmogrep_workspace.open_path(_A)', '" + str(root / 'manual.py').replace("'", "''") + "')")
    assert not lua('osmogrep_workspace.status()')['following']
    assert 'live.py' in lua('osmogrep_workspace.tabs()') and 'manual.py' in lua('osmogrep_workspace.tabs()')
    checks.append('native_editor_retains_live_and_inspected_file_tabs')
    rpc("luaeval('(function() osmogrep_workspace.split(); return true end)()')")
    assert lua('osmogrep_workspace.status()').get('inspection')
    checks.append('native_split_view_keeps_a_separate_inspection_window')
    editor_pid = int(rpc('getpid()'))
    rpc("luaeval('(function() vim.api.nvim_buf_set_lines(0,0,-1,false,{\"UNSAVED NATIVE\"}); return true end)()')")
    apple(send, window, '/nv toggle')
    time.sleep(1)
    assert int(apple(count, window)) == 2
    os.kill(editor_pid, 0)
    subprocess.run(['nvim', '--server', str(socket), '--remote-send', 'c'], check=True, capture_output=True, timeout=5)
    wait(lambda: lua('vim.bo.modified'))
    assert (root / 'manual.py').read_text() == 'manual = True\n'
    checks.append('native_close_preserves_unsaved_buffers_until_the_user_decides')
    rpc("luaeval('(function() vim.bo.modified=false; return true end)()')")
    apple(send, window, '/nv toggle')
    wait(lambda: int(apple(count, window)) == 1)
    os.kill(pid, 0)
    checks.append('closing_editor_expands_the_same_live_chat_process')
    assert not failures
    apple(send, window, '/exit')
finally:
    try:
        apple(close, window)
    except subprocess.CalledProcessError:
        pass
    server.shutdown()
    server.server_close()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps({'checks': checks, 'fixture_requests': len(requests), 'errors': failures,
        'scope': 'Dedicated native Ghostty window, real Rust UI and Neovim, isolated files, fake local model; no paid calls', 'fixture': str(root)}, indent=2) + '\n')
print(json.dumps({'passed': len(checks), 'checks': checks}, indent=2))
