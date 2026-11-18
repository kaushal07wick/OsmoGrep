"""Exercise the real harness against deterministic local models, never paid APIs.

Run after cargo build: python3 terminal-ui/scripts/harness-acceptance.py
  --binary target/debug/osmogrep --output .context/harness-foundations/acceptance.json
"""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import queue
import subprocess
import tempfile
import threading
import time

parser = argparse.ArgumentParser()
parser.add_argument('--binary', required=True, type=Path)
parser.add_argument('--output', required=True, type=Path)
args = parser.parse_args()
binary = args.binary.resolve()
results = []
failures = []
requests = []
phases = {}
worker = {}


def call(name, arguments):
    return {'type': 'function_call', 'call_id': f'call_{time.monotonic_ns()}',
            'name': name, 'arguments': json.dumps(arguments)}


def text(value):
    return {'type': 'message', 'role': 'assistant',
            'content': [{'type': 'output_text', 'text': value}]}


class Model(http.server.BaseHTTPRequestHandler):
    def log_message(self, *unused):
        pass

    def do_POST(self):
        try:
            body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            assert self.headers['Authorization'] == 'Bearer fixture-key'
            assert self.path == '/v1/responses'
            assert '_osmogrep_compaction' not in body
            requests.append(body)
            inputs = body['input']
            if any('Produce a factual handoff summary' in str(m.get('content', '')) for m in inputs if m.get('role') == 'system'):
                output = [text('The user requires CASE_CONTEXT to preserve REQUIREMENT_947. Reading large.txt completed. Continue the current task; no tests or delegation were requested.')]
            else:
                index = max(i for i, m in enumerate(inputs) if m.get('role') == 'user')
                prompt = str(inputs[index]['content'])
                outputs = [json.loads(m['output']) for m in inputs[index + 1:] if m.get('type') == 'function_call_output']
                if 'Delegated scope:' in prompt:
                    assert not any(t['name'] in ['subagent', 'worktree_swarm'] for t in body['tools'])
                    if not outputs:
                        output = [call('read_file', {'path': 'notes.txt'})]
                    elif len(outputs) == 1:
                        assert 'USER_DIRTY' in outputs[0]['text']
                        output = [call('edit_file', {'path': 'notes.txt', 'old': 'USER_DIRTY', 'new': 'WORKER_EDIT'})]
                    else:
                        assert 'error' not in outputs[-1], outputs[-1]
                        output = [text('Updated notes.txt within the delegated scope.')]
                elif 'CASE_WORKER' in prompt:
                    if not outputs:
                        output = [call('subagent', {'action': 'spawn', 'task': 'Change USER_DIRTY to WORKER_EDIT in notes.txt.', 'scope': 'notes.txt only', 'writable': True, 'max_iterations': 6, 'timeout_secs': 30})]
                    else:
                        if 'id' in outputs[-1]:
                            worker['id'] = outputs[-1]['id']
                            if 'workspace' in outputs[-1]:
                                worker['root'] = outputs[-1]['workspace']
                        last = outputs[-1]
                        if 'patch_file' in last:
                            assert 'USER_DIRTY' in last['patch'] and 'WORKER_EDIT' in last['patch'], last
                            worker['patch'] = last['patch']
                            output = [text('Worker changes are ready for review; the parent file is unchanged.')]
                        elif last.get('status') == 'waiting_permission':
                            permission = next(e for e in reversed(last['events']) if e.get('type') == 'permission_request')
                            output = [call('subagent', {'action': 'permission', 'id': worker['id'], 'request_id': permission['request_id'], 'allow': True})]
                        elif last.get('status') == 'idle':
                            output = [call('subagent', {'action': 'changes', 'id': worker['id']})]
                        else:
                            output = [call('subagent', {'action': 'status', 'id': worker['id'], 'wait_ms': 1000})]
                elif 'CASE_EDIT' in prompt or 'CASE_CRASH' in prompt:
                    if not outputs:
                        output = [call('read_file', {'path': 'notes.txt'})]
                    elif len(outputs) == 1:
                        output = [call('edit_file', {'path': 'notes.txt', 'old': 'USER_DIRTY', 'new': 'AGENT_EDIT'})]
                    else:
                        if 'CASE_CRASH' in prompt:
                            phases['crash_waiting'] = True
                            time.sleep(3)
                        output = [text('Edit denied; file unchanged.' if 'error' in outputs[-1] else 'Updated notes.txt.')]
                elif 'CASE_SHELL' in prompt:
                    if not outputs:
                        output = [call('run_shell', {'cmd': "printf early; read answer; printf ':%s' \"$answer\"", 'session_id': None, 'input': None, 'yield_ms': 100, 'timeout_secs': 10})]
                    elif len(outputs) == 1:
                        assert outputs[-1]['running'] and outputs[-1]['stdout'] == 'early'
                        output = [call('run_shell', {'session_id': outputs[-1]['session_id'], 'input': 'late\n', 'stdout_cursor': outputs[-1]['stdout_cursor'], 'stderr_cursor': outputs[-1]['stderr_cursor'], 'yield_ms': 1000})]
                    else:
                        assert outputs[-1]['stdout'] == ':late' and outputs[-1]['exit_code'] == 0, outputs[-1]
                        output = [text('Interactive command completed.')]
                elif 'CASE_CONTEXT' in prompt:
                    if any('Reading large.txt completed' in str(m.get('content', '')) for m in inputs if m.get('role') == 'assistant'):
                        assert 'REQUIREMENT_947' in prompt
                        output = [text('Completed CASE_CONTEXT while preserving REQUIREMENT_947.')]
                    elif not outputs:
                        output = [call('read_file', {'path': 'large.txt'})]
                    else:
                        raise AssertionError('Expected in-loop compaction before the next model request')
                elif 'CASE_OVERFLOW' in prompt:
                    phase = phases.get('overflow', 0)
                    phases['overflow'] = phase + 1
                    if phase == 0:
                        output = [call('read_file', {'path': 'large.txt'})]
                    elif phase == 1:
                        self.reply({'error': {'code': 'context_length_exceeded', 'message': 'maximum context length exceeded'}}, 400)
                        return
                    else:
                        output = [text('Recovered from provider context overflow.')]
                elif 'CASE_WAIT' in prompt:
                    phases['waiting'] = True
                    time.sleep(3)
                    output = [text('Late response must not complete the cancelled turn.')]
                else:
                    if 'CASE_FIRST' in prompt:
                        time.sleep(.15)
                    if 'CASE_SECOND' in prompt:
                        assert 'Completed CASE_FIRST' in json.dumps(inputs)
                    output = [text('Completed CASE_FIRST' if 'CASE_FIRST' in prompt else 'Direct answer; no tools needed.')]
            self.reply({'status': 'completed', 'output': output,
                        'usage': {'input_tokens': 100, 'output_tokens': 20, 'total_tokens': 120}})
        except (BrokenPipeError, ConnectionResetError):
            pass
        except Exception as error:
            failures.append(repr(error))
            self.reply({'error': {'message': repr(error)}}, 500)

    def reply(self, value, status=200):
        data = json.dumps(value).encode()
        self.send_response(status)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)


class Client:
    def __init__(self, root, env, approve=True, resume=None):
        command = [str(binary), 'rpc', '--repo-root', str(root)]
        if approve:
            command += ['--auto-approve']
        if resume:
            command += ['--session-id', resume]
        self.process = subprocess.Popen(command, cwd=root, env=env, stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.queue = queue.Queue()
        self.events = []
        self.errors = []
        def read():
            for line in self.process.stdout:
                try:
                    self.queue.put(json.loads(line))
                except ValueError:
                    self.errors.append(line)
        threading.Thread(target=read, daemon=True).start()
        threading.Thread(target=lambda: self.errors.extend(self.process.stderr.readlines()), daemon=True).start()
        self.ready = self.wait(lambda e: e.get('type') == 'ready')

    def send(self, method, params=None):
        identity = f'{method}_{time.monotonic_ns()}'
        self.process.stdin.write(json.dumps({'jsonrpc': '2.0', 'id': identity, 'method': method, 'params': params or {}}) + '\n')
        self.process.stdin.flush()
        return identity

    def wait(self, predicate, timeout=15):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                event = self.queue.get(timeout=.1)
            except queue.Empty:
                if self.process.poll() is not None:
                    raise AssertionError(f'RPC exited: {self.process.returncode}, {self.errors}')
                continue
            self.events.append(event)
            if event.get('type') in ['error', 'protocol_error']:
                raise AssertionError(event)
            if predicate(event):
                return event
        raise AssertionError(f'RPC timeout; last events: {self.events[-8:]}; stderr: {self.errors}')

    def request(self, method, params=None):
        identity = self.send(method, params)
        result = self.wait(lambda e: e.get('id') == identity)
        assert 'error' not in result, result
        return result['result']

    def prompt(self, prompt):
        self.send('prompt', {'prompt': prompt})
        self.wait(lambda e: e.get('type') == 'done')

    def close(self):
        if self.process.poll() is None:
            self.send('shutdown')
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                raise


with tempfile.TemporaryDirectory(prefix='osmogrep-harness-') as folder:
    temporary = Path(folder)
    project = temporary / 'project'
    project.mkdir()
    home = temporary / 'home'
    config = home / ('Library/Application Support/osmogrep' if os.uname().sysname == 'Darwin' else '.config/osmogrep')
    config.mkdir(parents=True)
    (project / '.gitignore').write_text('.context/\n')
    (project / 'notes.txt').write_text('ORIGINAL\n')
    (project / 'large.txt').write_text('large line with relevant code context\n' * 1000)
    subprocess.run(['git', 'init', '-q', str(project)], check=True)
    subprocess.run(['git', '-C', str(project), 'add', '.'], check=True)
    subprocess.run(['git', '-C', str(project), '-c', 'user.name=Kaushal Choudhary', '-c', 'user.email=kaushalc64@gmail.com', 'commit', '-qm', 'fixture baseline'], check=True)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Model)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    env = os.environ.copy()
    env.update(HOME=str(home), XDG_CONFIG_HOME=str(home / '.config'), OPENAI_API_KEY='fixture-key', OSMOGREP_PROVIDER='openai',
               OSMOGREP_MODEL='fixture-model', OSMOGREP_BASE_URL=f'http://127.0.0.1:{server.server_port}/v1', OSMOGREP_API='responses',
               OSMOGREP_NO_STREAM='1', OSMOGREP_MAX_ITERATIONS='35', OSMOGREP_DISABLE_UPDATE_CHECK='1')
    clients = []
    try:
        first = Client(project, env)
        clients.append(first)
        first.prompt('hello')
        assert not any(e.get('type') == 'tool_call' for e in first.events)
        assert requests[-1]['tools'] == []
        results.append('greeting_without_tools')
        first.send('prompt', {'prompt': 'CASE_FIRST'})
        first.send('prompt', {'prompt': 'CASE_SECOND'})
        first.wait(lambda e: e.get('type') == 'done')
        first.wait(lambda e: e.get('type') == 'done')
        results.append('queued_turns_preserve_conversation')
        session = first.request('session')
        assert session['id'] == first.ready['session_id'] and len(session['messages']) >= 7
        assert any(isinstance(e.get('usage', {}).get('input'), int) or isinstance(e.get('usage', {}).get('input_tokens'), int) for e in first.events if e.get('type') == 'usage'), first.events
        first.close()
        resumed = Client(project, env, resume=session['id'])
        clients.append(resumed)
        assert resumed.request('session')['messages'] == session['messages']
        results.append('independent_session_resume_and_full_history')
        resumed.close()
        (project / 'notes.txt').write_text('USER_DIRTY\n')
        denied = Client(project, env, approve=False)
        clients.append(denied)
        denied.send('prompt', {'prompt': 'CASE_EDIT_DENY'})
        permission = denied.wait(lambda e: e.get('type') == 'permission_request')
        assert (project / 'notes.txt').read_text() == 'USER_DIRTY\n'
        denied.send('permission', {'request_id': permission['request_id'], 'allow': False})
        denied.wait(lambda e: e.get('type') == 'done')
        assert (project / 'notes.txt').read_text() == 'USER_DIRTY\n'
        results.append('permission_denial_preserves_files')
        denied.close()
        editor = Client(project, env)
        clients.append(editor)
        editor.prompt('CASE_EDIT_ALLOW')
        assert (project / 'notes.txt').read_text() == 'AGENT_EDIT\n'
        turns = editor.request('checkpoints')
        checkpoint = next(t[0] for t in turns if 'CASE_EDIT_ALLOW' in t[1])
        (project / 'notes.txt').write_text('LATER_USER_EDIT\n')
        identity = editor.send('rewind', {'checkpoint_id': checkpoint, 'mode': 'code'})
        refusal = editor.wait(lambda e: e.get('id') == identity)
        assert 'error' in refusal and (project / 'notes.txt').read_text() == 'LATER_USER_EDIT\n'
        (project / 'notes.txt').write_text('AGENT_EDIT\n')
        assert editor.request('rewind', {'checkpoint_id': checkpoint, 'mode': 'code'})['restored_files'] == 1
        assert (project / 'notes.txt').read_text() == 'USER_DIRTY\n'
        results.append('turn_checkpoint_restore_and_conflict_protection')
        editor.prompt('CASE_SHELL')
        results.append('background_command_incremental_output_and_stdin')
        editor.send('prompt', {'prompt': 'CASE_WAIT'})
        deadline = time.monotonic() + 5
        while not phases.get('waiting') and time.monotonic() < deadline:
            time.sleep(.01)
        start = time.monotonic()
        editor.send('cancel')
        editor.wait(lambda e: e.get('type') == 'cancelled')
        editor.wait(lambda e: e.get('type') == 'done')
        assert time.monotonic() - start < 1
        editor.prompt('hello')
        results.append('cancellation_is_responsive_and_rpc_remains_usable')
        editor.prompt('CASE_OVERFLOW')
        assert phases['overflow'] == 3
        results.append('provider_context_overflow_compacts_and_retries_once')
        editor.close()
        (config / 'config.toml').write_text('[model_options."openai/fixture-model"]\ncontext_window = 8000\nmax_output_tokens = 2048\nreasoning = "none"\n')
        compacted = Client(project, env)
        clients.append(compacted)
        first_request = len(requests)
        compacted.prompt('CASE_CONTEXT preserve REQUIREMENT_947')
        summaries = [body for body in requests[first_request:] if 'Produce a factual handoff summary' in str(body['input'])]
        assert summaries and all(body.get('reasoning') == {'effort': 'none'} for body in summaries)
        assert list((project / '.context/osmogrep-runs').glob('*.context-*.json'))
        assert any('conversation-summary' in str(m.get('content', '')) for m in compacted.request('session')['messages'])
        results.append('in_loop_model_aware_compaction_archives_original_context')
        compacted.close()
        (config / 'config.toml').write_text('')
        controller = Client(project, env)
        clients.append(controller)
        controller.prompt('CASE_WORKER: use a subagent to edit notes.txt in isolation, review its changes, and preserve my parent changes.')
        assert (project / 'notes.txt').read_text() == 'USER_DIRTY\n'
        assert (Path(worker['root']) / 'notes.txt').read_text() == 'WORKER_EDIT\n'
        assert 'USER_DIRTY' in worker['patch']
        results += ['explicit_worker_isolation_includes_uncommitted_parent_changes', 'worker_permissions_and_reviewable_changes', 'worker_recursion_disabled']
        controller.close()
        (project / 'notes.txt').write_text('USER_DIRTY\n')
        crashed = Client(project, env)
        clients.append(crashed)
        crashed.send('prompt', {'prompt': 'CASE_CRASH'})
        deadline = time.monotonic() + 8
        while not phases.get('crash_waiting') and time.monotonic() < deadline:
            time.sleep(.01)
        assert phases.get('crash_waiting')
        crashed.process.kill()
        crashed.process.wait(timeout=3)
        checkpoint = next(json.loads(p.read_text()) for p in (project / '.context/osmogrep-runs').glob('*.checkpoint.json') if 'CASE_CRASH' in p.read_text())
        recovered = Client(project, env)
        clients.append(recovered)
        recovered.request('recover', {'run_id': checkpoint['run_id']})
        messages = recovered.request('session')['messages']
        assert any(m.get('type') == 'function_call_output' for m in messages)
        assert not any(e.get('type') == 'tool_call' for e in recovered.events)
        results.append('crash_recovery_keeps_completed_tools_without_automatic_reexecution')
        assert not failures, failures
    finally:
        for client in clients:
            client.close()
        if worker.get('root'):
            owned = Path(worker['root'])
            common = subprocess.check_output(['git', '-C', str(owned), 'rev-parse', '--git-common-dir'], text=True).strip()
            assert Path(common).resolve() == (project / '.git').resolve()
            subprocess.run(['git', '-C', str(project), 'worktree', 'remove', '--force', str(owned)], check=True)
        server.shutdown()

report = {'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'passed': len(results),
          'checks': results, 'model_requests': len(requests), 'paid_provider_calls': 0,
          'scope': 'Local process, filesystem, JSONL RPC, and deterministic HTTP fixtures; no model-quality or ultra-long-run claim'}
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=2))
