"""Exercise CLI configuration, discovery, diagnostics and team inspection without paid calls."""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import queue
import shutil
import sqlite3
import subprocess
import tempfile
import threading

parser = argparse.ArgumentParser()
parser.add_argument('--binary', required=True, type=Path)
parser.add_argument('--output', required=True, type=Path)
args = parser.parse_args()
binary = args.binary.resolve()
root = Path(tempfile.mkdtemp(prefix='osmogrep-cli-'))
home = root / 'home'
home.mkdir()
requests = []
checks = []
completion_parsers = []

class Catalog(http.server.BaseHTTPRequestHandler):
    def log_message(self, *unused):
        pass

    def do_GET(self):
        requests.append(self.path)
        body = json.dumps({'data': [{'id': 'fixture'}, {'id': 'second-model'}]}).encode()
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Catalog)
threading.Thread(target=server.serve_forever, daemon=True).start()
environment = {key: value for key, value in os.environ.items()
               if not key.startswith(('OSMOGREP_', 'VLLM_REALTIME_', 'OPENAI_', 'ANTHROPIC_', 'OLLAMA_'))}
environment.update(HOME=str(home), XDG_CONFIG_HOME=str(root/'config'), OSMOGREP_PROVIDER='ollama',
                   OSMOGREP_MODEL='fixture', OLLAMA_API_KEY='local-fixture-only',
                   OSMOGREP_BASE_URL=f'http://127.0.0.1:{server.server_port}/v1',
                   OSMOGREP_DISABLE_UPDATE_CHECK='1', NO_PROXY='127.0.0.1,localhost')

def call(*command, code=0):
    result = subprocess.run([str(binary), *command], cwd=root, env=environment,
                            capture_output=True, text=True, timeout=30)
    assert result.returncode == code, (command, result.returncode, result.stdout, result.stderr)
    return result.stdout

class Agent:
    def __init__(self):
        self.process = subprocess.Popen([str(binary), '--team', 'rpc', '--repo-root', str(root)],
                                        cwd=root, env=environment, stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.events = queue.Queue()
        def read():
            for line in self.process.stdout:
                self.events.put(json.loads(line))
        threading.Thread(target=read, daemon=True).start()
        while True:
            event = self.events.get(timeout=10)
            if event.get('type') == 'ready':
                self.session = event['session_id']
                break

    def request(self, method, params=None):
        self.process.stdin.write(json.dumps({'id': method, 'method': method, 'params': params or {}})+'\n')
        self.process.stdin.flush()
        while True:
            event = self.events.get(timeout=10)
            if event.get('id') == method:
                assert 'error' not in event, event
                return event.get('result')

    def close(self):
        if self.process.poll() is None:
            self.request('shutdown')
            self.process.stdin.close()
            self.process.wait(timeout=10)

agents = []
try:
    config = Path(call('config', 'path').strip())
    config.parent.mkdir(parents=True, exist_ok=True)
    config.write_text("api_key='LEGACY_SECRET'\n[mcp]\nenabled=true\n")
    call('config', 'set', 'provider', 'ollama')
    call('config', 'set', 'model', 'fixture')
    call('config', 'set', 'reasoning', 'high')
    assert call('config', 'get', 'reasoning').strip() == 'high'
    visible = call('config', 'show')
    assert 'LEGACY_SECRET' not in visible
    assert json.loads(visible)['provider'] == 'ollama'
    assert 'enabled = true' in config.read_text()
    before = config.read_bytes()
    call('config', 'set', 'base-url', 'https://user:secret@example.com/v1', code=1)
    assert config.read_bytes() == before
    checks.append('atomic_validated_settings_and_secret_redaction')
    catalog = json.loads(call('models', '--json'))
    assert catalog['models'] == ['fixture', 'second-model']
    current = json.loads(call('models', '--current'))
    assert current['configured_options']['reasoning'] == 'high'
    checks.append('model_discovery_and_configured_reasoning')
    healthy = json.loads(call('doctor', '--deep', '--json'))
    assert not any(check['status'] == 'fail' for check in healthy['checks']), healthy
    assert next(check for check in healthy['checks'] if check['id'] == 'provider')['status'] == 'pass'
    assert 'LEGACY_SECRET' not in json.dumps(healthy)
    sessions = config.parent/'sessions'
    sessions.mkdir(exist_ok=True)
    broken = sessions/'damaged.json'
    broken.write_text('{')
    report = json.loads(call('doctor', '--deep', '--json', code=1))
    assert any(check['id'] == 'sessions' and check['status'] == 'fail' for check in report['checks'])
    assert str(broken) in json.dumps(report)
    call('doctor', '--fix', '--json')
    assert broken.read_text() == '{'
    if os.name == 'posix':
        assert config.stat().st_mode & 0o077 == 0
    broken.unlink()
    checks.append('deep_diagnostics_repair_permissions_and_preserve_damaged_data')
    for shell in ['bash', 'zsh', 'fish']:
        script = call('completions', shell)
        assert 'doctor' in script and 'models' in script and 'config' in script
        if shell in ['bash', 'zsh'] and shutil.which(shell):
            subprocess.run([shell, '-n'], input=script, text=True, check=True, capture_output=True)
            completion_parsers.append(shell)
    checks.append('completions_generated_and_available_shell_parsers_pass')
    agents = [Agent(), Agent()]
    peers = json.loads(call('team', '--json'))
    assert {peer['session'] for peer in peers} == {agent.session for agent in agents}
    body = 'Full message\n'+('    preserve indentation\n'*100)+'END_OF_MESSAGE'
    agents[0].request('team', {'action': 'send', 'recipient': agents[1].session, 'body': body, 'message_id': 'cli-test'})
    database = sqlite3.connect(f'file:{config.parent}/team.sqlite3?mode=ro', uri=True)
    before = database.execute('SELECT COUNT(*) FROM team_receipts').fetchone()[0]
    history = json.loads(call('team', 'history', '--session', agents[1].session, '--json'))
    assert history[-1]['message']['body'] == body
    assert database.execute('SELECT COUNT(*) FROM team_receipts').fetchone()[0] == before
    assert len(json.loads(call('team', '--json'))) == 2
    database.close()
    checks.append('team_inspection_does_not_register_phantom_agents_or_acknowledge_messages')
    report = {'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'checks': checks,
              'catalog_requests': len(requests), 'model_generation_requests': 0, 'paid_calls': 0,
              'completion_parsers': completion_parsers,
              'fixture': str(root)}
    args.output.write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(report, indent=2))
finally:
    for agent in agents:
        agent.close()
    server.shutdown()
