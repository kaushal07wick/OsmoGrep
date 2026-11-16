"""Opt-in real-model coding checks in throwaway projects; never sends this repository."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import urllib.parse
import urllib.request
import coding_grade

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
mode = parser.add_mutually_exclusive_group(required=True)
mode.add_argument('--allow-paid', action='store_true')
mode.add_argument('--local-only', action='store_true', help='Require an installed, non-cloud Ollama model on loopback; isolate configuration and credentials')
parser.add_argument('--supervised', action='store_true', help='Exercise the durable task supervisor with the same independent result checks')
parser.add_argument('--case', choices=['documentation','bug_fix','multi_file_change'])
args = parser.parse_args()
binary = args.binary.resolve()
local = None
local_home = None
if args.local_only:
    base = os.environ.get('OSMOGREP_BASE_URL', 'http://127.0.0.1:11434/v1').rstrip('/')
    url = urllib.parse.urlsplit(base)
    model = os.environ.get('OSMOGREP_MODEL', '')
    if model and ':' not in model.rsplit('/', 1)[-1]:
        model += ':latest'
    if os.environ.get('OSMOGREP_PROVIDER') != 'ollama' or not model or url.scheme != 'http' or url.hostname not in ('localhost', '127.0.0.1', '::1') or url.username or url.password or url.query or url.fragment or url.path != '/v1':
        parser.error('--local-only requires OSMOGREP_PROVIDER=ollama, an explicit model, and an HTTP loopback /v1 endpoint')
    def metadata(path, value=None):
        request = urllib.request.Request(base[:-3] + path, data=json.dumps(value).encode() if value is not None else None, headers={'Content-Type':'application/json'})
        with urllib.request.urlopen(request, timeout=10) as response:
            return json.load(response)
    installed = next((m for m in metadata('/api/tags')['models'] if model in (m['name'], m.get('model'))), None)
    details = metadata('/api/show', {'model':model})
    if not installed or details.get('remote_model') or details.get('remote_host') or details.get('details', {}).get('format') != 'gguf':
        parser.error('--local-only requires installed local GGUF weights, not a cloud route')
    local = {'provider':'ollama', 'model':model, 'digest':installed['digest'], 'base_url':base, 'context_window':16384, 'max_output_tokens':2048, 'reasoning':'none'}
    local_home = tempfile.TemporaryDirectory(prefix='osmogrep-local-eval-home-')
    config = Path(local_home.name) / ('Library/Application Support/osmogrep' if sys.platform == 'darwin' else '.config/osmogrep')
    config.mkdir(parents=True)
    (config/'config.toml').write_text(f'[model_options.{json.dumps("ollama/" + model)}]\ncontext_window = 16384\nmax_output_tokens = 2048\nreasoning = "none"\n')
results = []
cases = [
    ('documentation', {'README.md': '# Example\n\nUSER_NOTE_KEEP_947\n'},
     'In README.md add a short section describing installation with python3 -m pip install example. Preserve all existing content exactly. Do not commit, delegate, or run tests. Finish with one short summary.',
     coding_grade.documentation),
    ('bug_fix', {'maths.py': 'def mean(values):\n    return sum(values)\n',
                 'test_maths.py': 'import unittest\nfrom maths import mean\nclass Tests(unittest.TestCase):\n def test_mean(self): self.assertEqual(mean([2, 4, 6]), 4)\nif __name__ == "__main__": unittest.main()\n'},
     'Fix mean in maths.py so the existing unittest passes. Preserve the test file unchanged. Run python3 -m unittest -q once and finish if it passes. Do not commit or delegate. Finish with a brief summary and actual check result.',
     coding_grade.bug_fix),
    ('multi_file_change', {'formatting.py': 'def greeting(name):\n    return "Hello " + name\n',
                           'app.py': 'from formatting import greeting\ndef render():\n    return greeting("world")\n',
                           'test_app.py': 'import unittest\nfrom app import render\nclass Tests(unittest.TestCase):\n def test_render(self): self.assertEqual(render(), "Hello world")\nif __name__ == "__main__": unittest.main()\n'},
     'Rename greeting to format_greeting in formatting.py and update app.py to call it. Keep behavior and test_app.py unchanged. Run python3 -m unittest -q once and finish if it passes. Do not commit or delegate. Finish with a short summary.',
     coding_grade.multi_file_change),
]
for name, files, prompt, validate in cases:
    if args.case and args.case != name: continue
    with tempfile.TemporaryDirectory(prefix='osmogrep-live-coding-') as directory:
        root = Path(directory)
        for path, value in files.items():
            (root/path).write_text(value)
        env = os.environ.copy()
        if local:
            env = {'PATH':os.environ.get('PATH', os.defpath), 'HOME':local_home.name,
                   'OSMOGREP_PROVIDER':'ollama', 'OSMOGREP_MODEL':local['model'],
                   'OSMOGREP_BASE_URL':local['base_url'], 'OSMOGREP_API':'chat-completions'}
        env.update(OSMOGREP_DISABLE_UPDATE_CHECK='1', OSMOGREP_MAX_ITERATIONS='10')
        command = [str(binary),'run','--repo-root',str(root),'--prompt',prompt,'--json-events','--permission-profile','workspace-auto','--auto-approve']
        if args.supervised:
            command = [str(binary),'task','--repo-root',str(root),'start','--objective',prompt,'--criterion','The requested change is complete, original content/tests are preserved, and any explicitly requested verification passed.','--max-requests','18','--max-tokens','300000','--max-seconds','150','--json-events','--permission-profile','workspace-auto','--auto-approve']
        started = time.monotonic()
        try:
            completed = subprocess.run(command, cwd=root, env=env, capture_output=True, text=True, timeout=180)
        except subprocess.TimeoutExpired as error:
            stdout = error.stdout.decode(errors='replace') if isinstance(error.stdout, bytes) else error.stdout or ''
            completed = subprocess.CompletedProcess(command, 124, stdout, 'Timed out after 180 seconds')
        events = []
        for line in completed.stdout.splitlines():
            try: events.append(json.loads(line))
            except ValueError: pass
        errors = [event.get('message','error') for event in events if event.get('type') == 'error']
        tools = [event['name'] for event in events if event.get('type') == 'tool_call']
        response = ''.join(event.get('delta',event.get('text','')) for event in events if event.get('type') in ['stream_delta','output_text'])
        unchanged_tests = all((root/path).is_file() and (root/path).read_bytes() == value.encode() for path,value in files.items() if path.startswith('test_'))
        try:
            result_correct = validate(root)
        except (OSError, ValueError, SyntaxError, subprocess.TimeoutExpired) as error:
            errors.append(f'Independent grader: {type(error).__name__}')
            result_correct = False
        task = next((event['task'] for event in events if event.get('type') == 'task_finished'),None)
        passed = (not args.supervised or (task is not None and task['status'] == 'completed' and len(task['evidence']) == 1))
        passed = passed and completed.returncode == 0 and not errors and bool(response.strip()) and unchanged_tests and result_correct
        passed = passed and not any('Use cmd to start OR session_id' in event.get('summary','') for event in events if event.get('type')=='tool_result')
        passed = passed and not any(tool in ['subagent','worktree_swarm','git_commit'] for tool in tools)
        result = {'supervised':args.supervised,'task':task,'case':name,'passed':passed,'tools':tools,'exit_code':completed.returncode,'errors':errors,'original_tests_preserved':unchanged_tests,'independent_result_passed':result_correct,'tool_calls':[{'name':event['name'],'args':event.get('args')} for event in events if event.get('type')=='tool_call'],'tool_results':[event.get('summary','') for event in events if event.get('type')=='tool_result']}
        result.update(elapsed_seconds=round(time.monotonic()-started,3), reported_usage=[e['usage'] for e in events if e.get('type')=='usage'], stderr=completed.stderr[-2000:])
        result['workspace_files'] = {path:(root/path).read_text(errors='replace') for path in files if (root/path).is_file()}
        result['grader_version'] = 2
        results.append(result)
        print(json.dumps(result),flush=True)
args.output.parent.mkdir(parents=True,exist_ok=True)
args.output.write_text(json.dumps({'scope':'Opt-in live coding tasks with independent result checks; three cases are not a general coding benchmark','local_model':local,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'proprietary_repository_sent':False,'cases':results},indent=2)+'\n')
assert all(result['passed'] for result in results)
