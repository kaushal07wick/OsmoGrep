"""Exercise the real device tool and MCP lifecycle with fake companions; no OS download or boot."""
import json
import os
from pathlib import Path
import platform
import subprocess
import tempfile

if platform.system() != 'Darwin' or platform.machine() != 'arm64':
    print('SKIP: native companion preparation requires an Apple silicon Mac')
    raise SystemExit(0)

source = Path(__file__).resolve().parents[2]
root = Path(tempfile.mkdtemp(prefix='osmo-device-tool-')).resolve()
(root / 'fake-companion-fixture').touch()
(root / 'home').mkdir()
program = '''#!/usr/bin/env python3
import json,sys
from pathlib import Path
here=Path(__file__).resolve()
if sys.argv[1]=='up':
 assert '--no-view' in sys.argv
 marker=here.with_suffix('.device')
 print('reused' if marker.exists() else 'created',flush=True)
 marker.touch()
 print('ready',flush=True)
elif sys.argv[1]=='mcp':
 for line in sys.stdin:
  request=json.loads(line)
  if 'id' not in request: continue
  method=request['method']
  if method=='initialize':
   result={'protocolVersion':request['params']['protocolVersion'],'capabilities':{'tools':{}},'serverInfo':{'name':'fixture','version':'1'}}
  elif method=='tools/list':
   result={'tools':[{'name':name,'description':'Controlled fixture','inputSchema':{'type':'object','properties':{}}} for name in ['screenshot','phone_observe']]}
  else: result={}
  print(json.dumps({'jsonrpc':'2.0','id':request['id'],'result':result}),flush=True)
else: raise SystemExit('Open must use up, never view')
'''
environment = dict(os.environ, HOME=str(root / 'home'), TERM_PROGRAM='fixture',
                   OSMOGREP_DEVICE_TEST_ROOT=str(root), OSMOGREP_SANDBOX='off')
environment.pop('GHOSTTY_RESOURCES_DIR', None)
environment.pop('TMUX', None)
environment.pop('TMUX_PANE', None)
for device, variable in [('mac', 'OSMOGREP_TERMINAL_MAC'), ('phone', 'OSMOGREP_TERMINAL_PHONE')]:
    path = root / (device + "'s companion")
    path.write_text(program)
    path.chmod(0o700)
    environment[variable] = str(path)

build = subprocess.run(['cargo', 'test', '--locked', '--no-run', '--message-format=json'],
                       cwd=source, check=True, capture_output=True, text=True)
artifacts = [json.loads(line) for line in build.stdout.splitlines() if line.startswith('{')]
binary = next(a['executable'] for a in reversed(artifacts)
              if a.get('reason') == 'compiler-artifact' and a.get('executable') and a.get('profile', {}).get('test'))
subprocess.run([binary, '--ignored', '--exact',
                'tools::device_workspace::tests::automatic_preparation_reuses_devices_and_registers_agent_controls',
                '--nocapture'], cwd=source, env=environment, check=True)
print('Fixture:', root)
