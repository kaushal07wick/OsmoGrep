"""Opt-in paid-provider checks using only supplied text and a temporary read-only fixture."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import uuid

p = argparse.ArgumentParser()
p.add_argument('--binary', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
args = p.parse_args()
binary = args.binary.resolve()
cases = []
with tempfile.TemporaryDirectory(prefix='osmogrep-live-intent-') as folder:
    marker = 'READ_CONFIRMED_'+uuid.uuid4().hex
    (Path(folder)/'hello.txt').write_text(marker+'\n')
    env = os.environ.copy()
    env.update(OSMOGREP_DISABLE_UPDATE_CHECK='1',OSMOGREP_MAX_ITERATIONS='4')
    for name, prompt, expected in [
        ('greeting','hello',None),
        ('reasoning','What is 17 times 23? Reply with just the answer.','391'),
        ('supplied_code','Explain this JavaScript in one sentence: const doubled = [1, 2, 3].map(n => n * 2);',None),
        ('writing','Rewrite this sentence more politely: "Fix your broken code." Return only the rewrite.',None),
        ('workspace_read','Read hello.txt and report its exact marker. Do not edit files, run shell commands, or run tests.',marker),
    ]:
        run = subprocess.run([str(binary),'run','--repo-root',folder,'--prompt',prompt,'--json-events','--permission-profile','read-only'],cwd=folder,env=env,capture_output=True,text=True,timeout=75)
        events = []
        for line in run.stdout.splitlines():
            try:
                events.append(json.loads(line))
            except ValueError:
                pass
        tools = [e['name'] for e in events if e.get('type') == 'tool_call']
        response = ''.join(e.get('delta',e.get('text','')) for e in events if e.get('type') in ['stream_delta','output_text'])
        errors = [e.get('message','error') for e in events if e.get('type') == 'error']
        passed = run.returncode == 0 and not errors and bool(response.strip()) and (expected is None or expected in response)
        passed = passed and (tools == ['read_file'] if name == 'workspace_read' else not tools)
        result = {'case':name,'passed':passed,'tools':tools,'exit_code':run.returncode,'errors':errors}
        cases.append(result)
        print(json.dumps(result),flush=True)
args.output.parent.mkdir(parents=True,exist_ok=True)
args.output.write_text(json.dumps({'scope':'Opt-in live model calls: conversation, reasoning, supplied code, rewriting, and an actual temporary file read','proprietary_repository_sent':False,'cases':cases},indent=2)+'\n')
assert all(case['passed'] for case in cases)
