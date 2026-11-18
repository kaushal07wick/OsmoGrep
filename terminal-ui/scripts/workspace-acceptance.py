"""Exercise workspace controls through a real PTY using only an in-process fixture model."""
import argparse, fcntl, hashlib, http.server, json, os, pty, select, signal, struct, subprocess, sys, tempfile, termios, threading, time
from pathlib import Path
parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
binary = args.binary.resolve()
root = Path(tempfile.mkdtemp(prefix='osmogrep-workspace-')).resolve()
home = root / 'home'
home.mkdir()
(root / 'src').mkdir()
original = 'first\nselected two\nselected three\nfour\nfive\nsix\nlast\n'
changed = 'FIRST\nselected two\nselected three\nfour\nfive\nsix\nLAST\n'
(root / 'src/main.rs').write_text(original)
subprocess.run(['git', 'init', '-q', str(root)], check=True)
requests = []
gate = threading.Event()
slow = threading.Event()

class Model(http.server.BaseHTTPRequestHandler):

    def log_message(self, *args):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        requests.append(body)
        latest = max((i for i, m in enumerate(body['input']) if m.get('role') == 'user' and isinstance(m.get('content'), str)))
        prompt = body['input'][latest]['content']
        complete = sum((m.get('type') == 'function_call_output' for m in body['input'][latest + 1:]))
        if prompt == 'slow response':
            slow.set()
            gate.wait(15)
        if prompt == 'make two edits' and complete == 0:
            output = [{'type': 'function_call', 'call_id': 'read', 'name': 'read_file', 'arguments': json.dumps({'path': 'src/main.rs'})}]
        elif prompt == 'make two edits' and complete == 1:
            output = [{'type': 'function_call', 'call_id': 'write', 'name': 'write_file', 'arguments': json.dumps({'path': 'src/main.rs', 'content': changed})}]
        else:
            output = [{'type': 'message', 'role': 'assistant', 'content': [{'type': 'output_text', 'text': 'ACK ' + prompt}]}]
        data = json.dumps({'output': output}).encode()
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)
server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Model)
threading.Thread(target=server.serve_forever, daemon=True).start()
env = {k: v for k, v in os.environ.items() if not k.startswith(('OSMOGREP_', 'OPENAI_', 'VLLM_REALTIME_'))}
env.update(HOME=str(home), XDG_CONFIG_HOME=str(root / 'config'), TERM='xterm-ghostty', TERM_PROGRAM='fixture', COLORTERM='truecolor', OPENAI_API_KEY='fixture', OSMOGREP_PROVIDER='openai', OSMOGREP_MODEL='fixture', OSMOGREP_API='responses', OSMOGREP_BASE_URL=f'http://127.0.0.1:{server.server_port}', OSMOGREP_UI='typescript', OSMOGREP_DISABLE_UPDATE_CHECK='1', OSMOGREP_CLIPBOARD_OSC52='1', OSMOGREP_SANDBOX='off', NO_PROXY='127.0.0.1,localhost')
config = home / 'Library/Application Support/osmogrep' if sys.platform == 'darwin' else root / 'config/osmogrep'
sessions = config / 'sessions'
sessions.mkdir(parents=True)
(sessions / (hashlib.sha256(str(root).encode()).hexdigest() + '.json')).write_text(json.dumps({'repo_root': str(root), 'permission_profile': 'full_access', 'preferences': {'reduce_motion': True}}))
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 38, 120, 1440, 912))

def terminal():
    os.setsid()
    fcntl.ioctl(0, termios.TIOCSCTTY, 0)
process = subprocess.Popen([str(binary), '--session', 'Workspace fixture'], cwd=root, env=env, stdin=slave, stdout=slave, stderr=slave, preexec_fn=terminal)
os.close(slave)
output = bytearray()
checks = []

def drain(seconds=0.2):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if select.select([master], [], [], 0.04)[0]:
            try:
                data = os.read(master, 1000000)
            except OSError:
                break
            output.extend(data)
            if b'\x1b[6n' in data:
                os.write(master, b'\x1b[1;1R')

def wait(check, seconds=12):
    end = time.monotonic() + seconds
    while not check() and time.monotonic() < end:
        drain()
        assert process.poll() is None, output[-3000:].decode(errors='replace')
    assert check(), output[-4000:].decode(errors='replace')

def send(text):
    os.write(master, text.encode() if isinstance(text, str) else text)
    drain(0.25)

def expect(text):
    wait(lambda: text.encode() in output)

def snapshots():
    return [json.loads(p.read_text()) for p in sessions.glob('*.json') if '-' in p.stem]

def queued():
    return next((s.get('queued_prompts', []) for s in snapshots() if s.get('session_name') == 'Workspace fixture'), [])
try:
    drain(2)
    send('@src/main.rs:2-3')
    expect('Selected lines')
    send('\r')
    expect('[Context #1]')
    send('explain selection\r')
    expect('ACK')
    wait(lambda: bool(requests))
    payload = json.dumps(requests[0])
    assert 'selected two' in payload and 'selected three' in payload and ('first\\n' not in payload)
    checks.append('explicit_line_reference_is_sent_only_after_selection')
    send('slow response\r')
    wait(slow.is_set)
    send('queued one\t')
    send('/queue\r')
    expect('Queue paused')
    send(b'\x1b1')
    expect('Edit queued message')
    send('\x15edited one\r')
    wait(lambda: queued() == ['edited one'])
    checks.append('queue_edit_persists_without_losing_draft_or_starting_turn')
    send(b'\x1b')
    gate.set()
    expect('ACK slow response')
    drain(1)
    assert len(requests) == 2
    send('/queue\r')
    send(b'\x1b6')
    wait(lambda: len(requests) >= 3)
    expect('ACK edited one')
    checks.append('paused_queue_resumes_explicitly')
    send('/find\r')
    send('ACK edited')
    expect('Current conversation')
    send('\r')
    checks.append('find_jumps_to_transcript_without_model_call')
    assert len(requests) == 3
    send('/context\r')
    send('ACK edited')
    send(b'\x1b2')
    expect('Pinned')
    send(b'\x1b')
    send('/pins\r')
    expect('ACK edited')
    send('\r')
    expect('[Context #2]')
    send('\x15')
    checks.append('pins_are_local_and_attach_explicitly')
    send('/timing\r')
    expect('First visible output')
    expect('Provider requests')
    send(b'\x1b')
    assert len(requests) == 3
    checks.append('timing_drawer_uses_client_observations_without_extra_calls')
    send('make two edits\r')
    wait(lambda: (root / 'src/main.rs').read_text() == changed)
    expect('ACK make two edits')
    send('/changes\r')
    expect('Revert lines')
    send(b'\x1b2')
    expect('Revert the selected lines')
    send(b'\x1b2')
    wait(lambda: (root / 'src/main.rs').read_text() == 'first\nselected two\nselected three\nfour\nfive\nsix\nLAST\n')
    checks.append('selected_hunk_reverts_with_other_hunk_and_checkpoint_preserved')
    assert list((root / '.context/osmogrep-checkpoints').glob('*.json'))
    send(b'\x1b')
    before = {p.name: p.read_text() for p in sessions.glob('*.json') if '-' in p.stem}
    send('/retry\r')
    expect('Edit and retry')
    send('\r')
    expect('Branched conversation')
    drain(0.5)
    assert len(snapshots()) > len(before)
    assert (root / 'src/main.rs').read_text().endswith('LAST\n')
    checks.append('retry_creates_branch_and_preserves_workspace')
    assert b'Native UI restored' not in output
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / 'terminal.ansi').write_bytes(output)
    (args.output / 'result.json').write_text(json.dumps({'checks': checks, 'model_requests': len(requests), 'paid_calls': 0, 'root': str(root)}, indent=2))
    print(json.dumps({'checks': checks, 'root': str(root)}, indent=2))
finally:
    gate.set()
    process.terminate()
    try:
        process.wait(timeout=4)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()
    (root / 'terminal.ansi').write_bytes(output)
    os.close(master)
    server.shutdown()
