"""Opt-in paid-model coding checks in throwaway projects; never sends this repository."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--allow-paid', action='store_true', required=True)
parser.add_argument('--case', choices=['documentation','bug_fix','multi_file_change'])
args = parser.parse_args()
binary = args.binary.resolve()
results = []
cases = [
    ('documentation', {'README.md': '# Example\n\nUSER_NOTE_KEEP_947\n'},
     'In README.md add a short section describing installation with python3 -m pip install example. Preserve all existing content exactly. Do not commit, delegate, or run tests. Finish with one short summary.',
     lambda root: 'USER_NOTE_KEEP_947' in (root/'README.md').read_text() and 'python3 -m pip install example' in (root/'README.md').read_text()),
    ('bug_fix', {'maths.py': 'def mean(values):\n    return sum(values)\n',
                 'test_maths.py': 'import unittest\nfrom maths import mean\nclass Tests(unittest.TestCase):\n def test_mean(self): self.assertEqual(mean([2, 4, 6]), 4)\nif __name__ == "__main__": unittest.main()\n'},
     'Fix mean in maths.py so the existing unittest passes. Preserve the test file unchanged. Run python3 -m unittest -q once and finish if it passes. Do not commit or delegate. Finish with a brief summary and actual check result.',
     lambda root: subprocess.run(['python3','-m','unittest','-q'], cwd=root, capture_output=True, timeout=10).returncode == 0),
    ('multi_file_change', {'formatting.py': 'def greeting(name):\n    return "Hello " + name\n',
                           'app.py': 'from formatting import greeting\ndef render():\n    return greeting("world")\n',
                           'test_app.py': 'import unittest\nfrom app import render\nclass Tests(unittest.TestCase):\n def test_render(self): self.assertEqual(render(), "Hello world")\nif __name__ == "__main__": unittest.main()\n'},
     'Rename greeting to format_greeting in formatting.py and update app.py to call it. Keep behavior and test_app.py unchanged. Run python3 -m unittest -q once and finish if it passes. Do not commit or delegate. Finish with a short summary.',
     lambda root: 'def format_greeting(' in (root/'formatting.py').read_text() and 'format_greeting(' in (root/'app.py').read_text() and subprocess.run(['python3','-m','unittest','-q'], cwd=root, capture_output=True, timeout=10).returncode == 0),
]
for name, files, prompt, validate in cases:
    if args.case and args.case != name: continue
    with tempfile.TemporaryDirectory(prefix='osmogrep-live-coding-') as directory:
        root = Path(directory)
        for path, value in files.items():
            (root/path).write_text(value)
        env = os.environ.copy()
        env.update(OSMOGREP_DISABLE_UPDATE_CHECK='1', OSMOGREP_MAX_ITERATIONS='10')
        completed = subprocess.run([str(binary),'run','--repo-root',str(root),'--prompt',prompt,'--json-events','--permission-profile','workspace-auto','--auto-approve'], cwd=root, env=env, capture_output=True, text=True, timeout=180)
        events = []
        for line in completed.stdout.splitlines():
            try: events.append(json.loads(line))
            except ValueError: pass
        errors = [event.get('message','error') for event in events if event.get('type') == 'error']
        tools = [event['name'] for event in events if event.get('type') == 'tool_call']
        response = ''.join(event.get('delta',event.get('text','')) for event in events if event.get('type') in ['stream_delta','output_text'])
        unchanged_tests = all((root/path).read_text() == value for path,value in files.items() if path.startswith('test_'))
        result_correct = validate(root)
        passed = completed.returncode == 0 and not errors and bool(response.strip()) and unchanged_tests and result_correct
        passed = passed and not any('Use cmd to start OR session_id' in event.get('summary','') for event in events if event.get('type')=='tool_result')
        passed = passed and not any(tool in ['subagent','worktree_swarm','git_commit'] for tool in tools)
        result = {'case':name,'passed':passed,'tools':tools,'exit_code':completed.returncode,'errors':errors,'original_tests_preserved':unchanged_tests,'independent_result_passed':result_correct,'tool_calls':[{'name':event['name'],'args':event.get('args')} for event in events if event.get('type')=='tool_call'],'tool_results':[event.get('summary','') for event in events if event.get('type')=='tool_result']}
        results.append(result)
        print(json.dumps(result),flush=True)
args.output.parent.mkdir(parents=True,exist_ok=True)
args.output.write_text(json.dumps({'scope':'Opt-in live coding tasks with independent result checks; three cases are not a general coding benchmark','binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'proprietary_repository_sent':False,'cases':results},indent=2)+'\n')
assert all(result['passed'] for result in results)
