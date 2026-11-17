"""Check typing and folding through the real terminal UI with a local fixture model."""
import argparse
import fcntl
import hashlib
import http.server
import json
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import threading
import time

parser = argparse.ArgumentParser()
parser.add_argument('--binary', required=True, type=Path)
parser.add_argument('--output', required=True, type=Path)
args = parser.parse_args()
binary = args.binary.resolve()
root = Path(tempfile.mkdtemp(prefix='osmogrep-input-')).resolve()
home = root/'home'
home.mkdir()
requests = []
typing_window = threading.Event()
source = '.rule{color:var(--accent);padding:12px}' * 600 + 'END_OF_CSS\n'

class Model(http.server.BaseHTTPRequestHandler):
    def log_message(self, *unused):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        requests.append(body)
        latest = max(i for i, item in enumerate(body['input']) if item.get('role') == 'user')
        prompt = body['input'][latest]['content']
        completed = sum(item.get('type') == 'function_call_output' for item in body['input'][latest+1:])
        if prompt in ['Explain a closure in one sentence.', 'Now update that file to say RESUMED_VALUE.']:
            assert body['input'][0] == requests[0]['input'][0], 'Stable instructions changed across turns'
            assert body['tools'] == requests[0]['tools'], 'Tool definitions or ordering changed'
            assert any(item.get('type') == 'function_call' and item.get('name') == 'write_file' for item in body['input'][:latest]), 'Coding history was dropped'
            if prompt.startswith('Explain'):
                output = [{'type':'message','role':'assistant','content':[{'type':'output_text','text':'INTERLUDE_FINISHED: A closure retains its surrounding bindings.'}]}]
            elif completed == 0:
                output = [{'type':'function_call','call_id':'resume-read','name':'read_file','arguments':json.dumps({'path':'site.css'})}]
            elif completed == 1:
                output = [{'type':'function_call','call_id':'resume-edit','name':'edit_file','arguments':json.dumps({'path':'site.css','old':'END_OF_CSS','new':'RESUMED_VALUE'})}]
            else:
                output = [{'type':'message','role':'assistant','content':[{'type':'output_text','text':'RESUME_FINISHED'}]}]
        elif completed == 0:
            output = [{'type': 'function_call', 'call_id': 'write', 'name': 'write_file',
                       'arguments': json.dumps({'path': 'site.css', 'content': source})}]
        elif completed == 1:
            output = [{'type': 'function_call', 'call_id': 'command', 'name': 'run_shell',
                       'arguments': json.dumps({'command': 'printf UI_CHILD_OK'})}]
        else:
            typing_window.set()
            time.sleep(2)
            output = [{'type': 'message', 'role': 'assistant', 'content': [
                {'type': 'output_text', 'text': 'INPUT_TEST_FINISHED\n\nDev server: http://127.0.0.1:4173'}]}]
        encoded = json.dumps({'output': output}).encode()
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Model)
threading.Thread(target=server.serve_forever, daemon=True).start()
environment = {key: value for key, value in os.environ.items()
               if not key.startswith(('OSMOGREP_', 'OPENAI_', 'VLLM_REALTIME_'))}
environment.update(HOME=str(home), XDG_CONFIG_HOME=str(root/'config'), TERM='xterm-ghostty',
                   TERM_PROGRAM='ghostty', COLORTERM='truecolor', OPENAI_API_KEY='fixture-only',
                   OSMOGREP_PROVIDER='openai', OSMOGREP_MODEL='input-fixture', OSMOGREP_API='responses',
                   OSMOGREP_BASE_URL=f'http://127.0.0.1:{server.server_port}',
                   OSMOGREP_UI='typescript', OSMOGREP_DISABLE_UPDATE_CHECK='1',
                   OSMOGREP_CLIPBOARD_OSC52='1',
                   NO_PROXY='127.0.0.1,localhost', OSMOGREP_SANDBOX='off')
config = home/'Library/Application Support/osmogrep' if sys.platform == 'darwin' else root/'config/osmogrep'
sessions = config/'sessions'
sessions.mkdir(parents=True)
legacy = sessions/(hashlib.sha256(str(root).encode()).hexdigest()+'.json')
legacy.write_text(json.dumps({'repo_root': str(root), 'permission_profile': 'full_access',
                              'preferences': {'reduce_motion': True}}))
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 32, 80, 960, 768))

def controlling_terminal():
    os.setsid()
    fcntl.ioctl(0, termios.TIOCSCTTY, 0)

process = subprocess.Popen([str(binary), '--session', 'Input fixture'], cwd=root, env=environment,
                           stdin=slave, stdout=slave, stderr=slave, preexec_fn=controlling_terminal)
os.close(slave)
terminal = bytearray()

def drain(seconds=.1):
    end = time.monotonic()+seconds
    while time.monotonic() < end:
        if select.select([master], [], [], min(.05, max(0, end-time.monotonic())))[0]:
            try:
                data = os.read(master, 1_000_000)
            except OSError:
                break
            terminal.extend(data)
            if b'\x1b[6n' in data:
                os.write(master, b'\x1b[1;1R')

def wait_for(check, seconds=15):
    end = time.monotonic()+seconds
    while not check() and time.monotonic() < end:
        drain()
        assert process.poll() is None, terminal[-3000:].decode(errors='replace')
    assert check(), terminal[-3000:].decode(errors='replace')

try:
    drain(2)
    os.write(master, b'Create site.css as requested and run the command.\r')
    wait_for(typing_window.is_set)
    os.write(master, b'live_draft_probe')
    wait_for(lambda: b'live_draft_probe' in terminal, 2)
    wait_for(lambda: b'INPUT_TEST_FINISHED' in terminal)
    wait_for(lambda: b'Done' in terminal)
    os.write(master, b'_after_done')
    wait_for(lambda: b'live_draft_probe_after_done' in terminal, 2)
    os.write(master, b'\x1b[<64;40;8M')
    wait_for(lambda: b'Latest' in terminal, 3)
    assert b'Show more' in terminal
    os.write(master, b'\x15/copy\r')
    wait_for(lambda: b'characters to clipboard' in terminal, 3)
    assert b'\x1b]52;c;' in terminal
    assert b'\x1b[38;2;95;175;255mCopied ' in terminal
    assert (root/'site.css').read_text() == source
    os.write(master, b'Explain a closure in one sentence.\r')
    wait_for(lambda: b'INTERLUDE_FINISHED' in terminal)
    os.write(master, b'Now update that file to say RESUMED_VALUE.\r')
    wait_for(lambda: b'RESUME_FINISHED' in terminal)
    assert (root/'site.css').read_text() == source.replace('END_OF_CSS', 'RESUMED_VALUE')
    assert b'Native UI restored' not in terminal
    (root/'terminal.ansi').write_bytes(terminal)
    report = {'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
              'checks': ['minified_diff_is_folded', 'typing_during_active_model_request',
                         'typing_after_completion', 'history_exposes_latest_action',
                         'native_accepts_renderer_frames', 'copy_feedback_uses_isolated_osc52_without_host_clipboard_writes',
                         'casual_interlude_preserves_coding_history_and_resumes_editing',
                         'stable_instructions_and_tool_order_across_turns'],
              'initial_instruction_bytes': len(requests[0]['input'][0]['content'].encode()),
              'initial_tool_schema_bytes': len(json.dumps(requests[0]['tools'], separators=(',', ':')).encode()),
              'fixture_model_requests': len(requests), 'paid_calls': 0, 'fixture': str(root)}
    args.output.write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(report, indent=2))
finally:
    if process.poll() is None:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=5)
    os.close(master)
    server.shutdown()
