"""Fast deterministic supervision fixtures; not a multi-hour endurance benchmark."""
import argparse
from collections import Counter
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
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
binary = args.binary.resolve()
counts, issued, checks, errors, calls = Counter(), Counter(), [], [], []
crash_wait, unblock, stop_wait = threading.Event(), threading.Event(), threading.Event()


def message(text):
    return {'type': 'message', 'role': 'assistant', 'content': [{'type': 'output_text', 'text': text}]}


def call(name, arguments):
    issued[name] += 1
    return {'type': 'function_call', 'call_id': str(time.monotonic_ns()), 'name': name, 'arguments': json.dumps(arguments)}


class Model(http.server.BaseHTTPRequestHandler):
    def log_message(self, *unused):
        pass

    def do_POST(self):
        try:
            body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            calls.append(body)
            if any('Produce a factual handoff summary' in str(m.get('content', '')) for m in body['input'] if m.get('role') == 'system'):
                counts['summaries'] += 1
                output = [message('The active durable task keeps its original objective and acceptance criteria. Completed reads and milestones are recorded. No delegation or unrelated tests are authorized. Continue the remaining work.')]
            else:
                prompt = next(str(m['content']) for m in reversed(body['input']) if m.get('role') == 'user')
                case = next(k for k in ['CASE_LONG', 'CASE_PREMATURE', 'CASE_BLOCKED', 'CASE_BUDGET', 'CASE_DEADLINE', 'CASE_CRASH', 'CASE_RETRY', 'CASE_LOOP', 'CASE_STOP'] if k in prompt)
                counts[case] += 1
                n = counts[case]
                system = '\n'.join(str(m.get('content', '')) for m in body['input'] if m.get('role') == 'system')
                assert '[durable-task]' in system and 'CRITERION_947' in system, system
                assert any(t['name'] == 'task_progress' for t in body['tools'])
                assert not any(t['name'] in ['subagent', 'worktree_swarm', 'dynamic_workflow'] for t in body['tools'])
                if case == 'CASE_LONG':
                    if n <= 104:
                        output = [call('task_progress', {'action': 'checkpoint', 'summary': f'Completed milestone {n}'})] if n % 4 == 0 else [call('read_file', {'path': f'chunk-{n}.txt'})]
                    elif n == 105:
                        output = [call('task_progress', {'action': 'complete', 'summary': 'Finished the bounded objective', 'evidence': ['CRITERION_947: all requested fixture reads completed']})]
                    else:
                        output = [message('Completed all requested reads and milestones.')]
                elif case == 'CASE_PREMATURE':
                    output = [message('Done.')]
                elif case == 'CASE_BLOCKED':
                    output = [call('task_progress', {'action': 'blocked', 'summary': 'Missing user-supplied credential; no workaround authorized'})]
                elif case == 'CASE_DEADLINE':
                    output = [call('run_shell', {'cmd': 'sleep 5; printf late > late.txt', 'yield_ms': 10000, 'timeout_secs': 30})]
                elif case == 'CASE_STOP':
                    stop_wait.set()
                    time.sleep(5)
                    output = [message('Delayed provider response')]
                elif case == 'CASE_CRASH':
                    if n == 1:
                        output = [call('read_file', {'path': 'notes.txt'})]
                    elif n == 2:
                        output = [call('edit_file', {'path': 'notes.txt', 'old': 'ORIGINAL', 'new': 'TASK_EDIT'})]
                    elif n == 3:
                        crash_wait.set()
                        unblock.wait(15)
                        output = [message('Interrupted request response')]
                    elif n == 4:
                        assert 'TASK_EDIT' in json.dumps(body['input']), 'Completed tool history was lost'
                        output = [call('task_progress', {'action': 'complete', 'summary': 'Recovered without replaying edits', 'evidence': ['CRITERION_947: edit was completed before interruption and is preserved']})]
                    else:
                        output = [message('The completed edit was recovered; no edit was replayed.')]
                elif case == 'CASE_LOOP':
                    output = [call('read_file', {'path': 'missing-file'})]
                elif case == 'CASE_RETRY':
                    self.send_error(503, 'Temporary fixture provider interruption')
                    return
                else:
                    output = [call('task_progress', {'action': 'checkpoint', 'summary': f'Budget milestone {n}'})]
            data = json.dumps({'output': output, 'usage': {'input_tokens': 200, 'output_tokens': 20}}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)
        except (BrokenPipeError, ConnectionResetError):
            pass  # The cancellation fixture deliberately closes its provider connection.
        except Exception as error:
            errors.append(repr(error))
            self.send_error(500, str(error))


class Task:
    def __init__(self, project, env, case=None, resume=None, options=None):
        command = [str(binary), 'task', '--repo-root', str(project)]
        command += ['resume', resume, '--reviewed'] if resume else ['start', '--objective', f'{case}: inspect the requested local fixture only; no delegation or tests', '--criterion', 'CRITERION_947: requested work is complete', '--max-requests', '160', '--max-tokens', '10000000', '--max-seconds', '90']
        command += ['--json-events', '--auto-approve'] + (options or [])
        # Later option values replace defaults above without duplicate clap flags.
        for name in ['--max-requests', '--max-tokens', '--max-seconds']:
            while command.count(name) > 1:
                index = command.index(name)
                del command[index:index + 2]
        self.process = subprocess.Popen(command, cwd=project, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.queue, self.events, self.errors = queue.Queue(), [], []
        def read():
            for line in self.process.stdout:
                try:
                    event = json.loads(line)
                    self.events.append(event)
                    self.queue.put(event)
                except ValueError:
                    self.errors.append(line)
        threading.Thread(target=read, daemon=True).start()
        threading.Thread(target=lambda: self.errors.extend(self.process.stderr.readlines()), daemon=True).start()
        self.started = self.wait('task_started')

    def wait(self, kind, timeout=60):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                event = self.queue.get(timeout=.1)
            except queue.Empty:
                assert self.process.poll() is None, (self.process.returncode, self.errors, self.events[-5:])
                continue
            if event.get('type') == kind:
                return event
        raise AssertionError((kind, self.events[-5:], self.errors))

    def finish(self):
        record = self.wait('task_finished')['task']
        code = self.process.wait(timeout=5)
        assert (code == 0) == (record['status'] == 'completed'), (code, record, self.errors)
        return record


with tempfile.TemporaryDirectory(prefix='osmogrep-long-run-') as folder:
    root = Path(folder)
    project, home = root / 'project', root / 'home'
    project.mkdir(); home.mkdir()
    (project / 'notes.txt').write_text('ORIGINAL\n')
    for n in range(1, 105):
        (project / f'chunk-{n}.txt').write_text((f'Original fixture {n}: retain user constraints and independent evidence.\n' * 90))
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Model)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    env = os.environ.copy()
    env.update(HOME=str(home), XDG_CONFIG_HOME=str(home / '.config'), OPENAI_API_KEY='fixture-key', OSMOGREP_PROVIDER='openai', OSMOGREP_MODEL='fixture-model', OSMOGREP_API='responses', OSMOGREP_BASE_URL=f'http://127.0.0.1:{server.server_port}/v1', OSMOGREP_NO_STREAM='1', OSMOGREP_DISABLE_UPDATE_CHECK='1', OSMOGREP_CONTEXT_WINDOW='32768')
    config = home / ('Library/Application Support/osmogrep' if os.uname().sysname == 'Darwin' else '.config/osmogrep')
    config.mkdir(parents=True)
    (config / 'config.toml').write_text('[model_options."openai/fixture-model"]\ncontext_window = 32768\nmax_output_tokens = 512\n')
    processes = []
    def run(case, options=None):
        task = Task(project, env, case=case, options=options)
        processes.append(task.process)
        return task.finish()
    try:
        long = run('CASE_LONG')
        assert long['status'] == 'completed' and long['requests'] > 100 and len(long['evidence']) == 1, long
        assert counts['summaries'] > 0 and long['requests'] == counts['CASE_LONG'] + counts['summaries'], (long, counts)
        checks.append('over_100_model_rounds_with_repeated_compaction_and_durable_criteria')
        premature = run('CASE_PREMATURE')
        assert premature['status'] == 'paused' and 'Stalled' in premature['reason'] and premature['requests'] == 3
        blocked = run('CASE_BLOCKED')
        assert blocked['status'] == 'paused' and 'Missing user-supplied credential' in blocked['reason']
        checks.append('premature_answers_and_external_blockers_never_become_completion')
        limited = run('CASE_BUDGET', ['--max-requests', '3'])
        assert limited['status'] == 'paused' and limited['requests'] == 3
        token = run('CASE_BUDGET', ['--max-tokens', '1'])
        assert token['status'] == 'paused' and token['requests'] == 0
        retry = run('CASE_RETRY', ['--max-requests', '2'])
        assert retry['requests'] == 2 and counts['CASE_RETRY'] == 2
        checks.append('request_token_and_retry_budgets_enforced_before_provider_calls')
        started = time.monotonic()
        deadline = run('CASE_DEADLINE', ['--max-seconds', '1'])
        assert deadline['status'] == 'paused' and 'Wall-time' in deadline['reason'] and time.monotonic() - started < 4
        checks.append('wall_deadline_interrupts_running_process')
        stop_task = Task(project, env, case='CASE_STOP'); processes.append(stop_task.process)
        assert stop_wait.wait(10)
        stopped_at = time.monotonic()
        stop_command = subprocess.run([str(binary), 'task', '--repo-root', str(project), 'stop', stop_task.started['task_id']], env=env, capture_output=True, text=True, timeout=5)
        assert stop_command.returncode == 0, stop_command.stderr
        stopped = stop_task.finish()
        assert stopped['status'] == 'paused' and 'Stop requested' in stopped['reason'] and time.monotonic() - stopped_at < 3
        checks.append('cross_process_stop_interrupts_waiting_provider')
        loop = run('CASE_LOOP')
        assert loop['status'] == 'paused' and 'Stalled' in loop['reason'] and loop['requests'] <= 9
        checks.append('repeated_identical_tool_failures_stop_after_durable_results')
        task = Task(project, env, case='CASE_CRASH'); processes.append(task.process)
        assert crash_wait.wait(15)
        task_id = task.started['task_id']
        task_file = project / '.context/osmogrep-tasks' / (task_id + '.json')
        before = json.loads(task_file.read_text())
        duplicate = subprocess.run([str(binary), 'task', '--repo-root', str(project), 'resume', task_id, '--reviewed'], env=env, capture_output=True, text=True, timeout=5)
        assert duplicate.returncode != 0 and 'running supervisor' in duplicate.stderr
        task.process.kill(); task.process.wait()
        assert (project / 'notes.txt').read_text() == 'TASK_EDIT\n'
        (project / 'notes.txt').write_text('TASK_EDIT\nUSER_EDIT_AFTER_CRASH\n')
        unreviewed = subprocess.run([str(binary), 'task', '--repo-root', str(project), 'resume', task_id], env=env, capture_output=True, text=True, timeout=5)
        assert unreviewed.returncode != 0 and '--reviewed' in unreviewed.stderr
        resumed = Task(project, env, resume=task_id); processes.append(resumed.process)
        recovered = resumed.finish(); unblock.set()
        assert recovered['status'] == 'completed' and recovered['requests'] > before['requests']
        assert recovered['charged_tokens'] >= before['charged_tokens'] and recovered['used_ms'] >= before['used_ms']
        assert (project / 'notes.txt').read_text() == 'TASK_EDIT\nUSER_EDIT_AFTER_CRASH\n'
        assert issued['edit_file'] == 1, 'An already-completed edit was replayed'
        checks.append('exclusive_ownership_crash_review_and_recovery_preserve_budgets_and_user_edits')
        assert not (project / 'late.txt').exists()
        assert not errors, errors
    finally:
        unblock.set()
        for process in processes:
            if process.poll() is None:
                process.kill(); process.wait()
        server.shutdown(); server.server_close()
    report = {'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'checks': checks, 'provider_requests': len(calls), 'counts': dict(counts), 'paid_requests': 0, 'scope': 'Fast deterministic process acceptance, not a multi-hour endurance or model-quality guarantee'}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))
