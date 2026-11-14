"""Run real Rust agent/tool turns against isolated protocol fixtures; no paid API calls."""
import argparse
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading

p = argparse.ArgumentParser()
p.add_argument('--binary', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
args = p.parse_args()
binary = args.binary.resolve()
results = []

with tempfile.TemporaryDirectory(prefix='osmogrep-provider-') as folder:
    root = Path(folder)
    (root/'a.txt').write_text('ALPHA_REAL_FILE_123\n')
    (root/'b.txt').write_text('BETA_REAL_FILE_456\n')
    for protocol in ['chat-completions', 'anthropic-messages']:
        for streaming in [False, True]:
            requests = []
            failures = []

            class Model(http.server.BaseHTTPRequestHandler):
                def log_message(self, *unused):
                    pass

                def do_POST(self):
                    try:
                        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                        requests.append(body)
                        anthropic = protocol == 'anthropic-messages'
                        assert self.path == ('/v1/messages' if anthropic else '/v1/chat/completions')
                        assert body['stream'] == streaming
                        assert 'input' not in body and 'store' not in body and 'reasoning' not in body
                        if anthropic:
                            assert self.headers.get('x-api-key') == 'fixture-key'
                            assert self.headers.get('anthropic-version') == '2023-06-01'
                        else:
                            assert self.headers.get('Authorization') == 'Bearer fixture-key'
                        first = len(requests) == 1
                        if first:
                            assert body['tools']
                        else:
                            assert len(requests) == 2
                            wire = json.dumps(body['messages'])
                            assert 'ALPHA_REAL_FILE_123' in wire and 'BETA_REAL_FILE_456' in wire
                            if anthropic:
                                assistant = next(m for m in body['messages'] if m['role'] == 'assistant')
                                assert [b['type'] for b in assistant['content']] == ['thinking', 'text', 'tool_use', 'tool_use']
                                assert assistant['content'][0]['signature'] == 'fixture-signature'
                                tool_results = [b for m in body['messages'] for b in m['content'] if b['type'] == 'tool_result']
                                assert [b['tool_use_id'] for b in tool_results] == ['read_a', 'read_b']
                            else:
                                assistant = next(m for m in body['messages'] if m['role'] == 'assistant')
                                assert assistant['reasoning_content'] == 'Inspect both files.'
                                assert len(assistant['tool_calls']) == 2
                                assert [m['tool_call_id'] for m in body['messages'] if m['role'] == 'tool'] == ['read_a', 'read_b']
                        text = 'Reading the requested files.' if first else 'ALPHA_REAL_FILE_123 BETA_REAL_FILE_456 — complete 🐛'
                        if anthropic:
                            blocks = [{'type':'thinking','thinking':'Inspect both files.','signature':'fixture-signature'}] if first else []
                            blocks += [{'type':'text','text':text}]
                            if first:
                                blocks += [{'type':'tool_use','id':'read_'+name,'name':'read_file','input':{'path':name+'.txt'}} for name in ['a','b']]
                            raw = {'content':blocks,'stop_reason':'tool_use' if first else 'end_turn','usage':{'input_tokens':100,'output_tokens':30,'cache_read_input_tokens':20}}
                        else:
                            message = {'role':'assistant','content':text}
                            if first:
                                message['reasoning_content'] = 'Inspect both files.'
                                message['tool_calls'] = [{'id':'read_'+name,'type':'function','function':{'name':'read_file','arguments':json.dumps({'path':name+'.txt'})}} for name in ['a','b']]
                            raw = {'choices':[{'message':message,'finish_reason':'tool_calls' if first else 'stop'}],'usage':{'prompt_tokens':120,'completion_tokens':30,'total_tokens':150}}
                        if not streaming:
                            data = json.dumps(raw).encode()
                        else:
                            events = []
                            if anthropic:
                                events.append({'type':'message_start','message':{'usage':raw['usage']}})
                                for i, block in enumerate(blocks):
                                    initial = dict(block, text='') if block['type'] == 'text' else block
                                    events.append({'type':'content_block_start','index':i,'content_block':initial})
                                    if block['type'] == 'text':
                                        events.append({'type':'content_block_delta','index':i,'delta':{'type':'text_delta','text':block['text']}})
                                    events.append({'type':'content_block_stop','index':i})
                                events += [{'type':'message_delta','delta':{'stop_reason':raw['stop_reason']},'usage':{'output_tokens':30}}, {'type':'message_stop'}]
                            else:
                                delta = raw['choices'][0]['message'].copy()
                                if first:
                                    for i, call in enumerate(delta['tool_calls']):
                                        call['index'] = i
                                events += [{'choices':[{'delta':delta,'finish_reason':raw['choices'][0]['finish_reason']}]} , {'choices':[],'usage':raw['usage']}]
                            data = ''.join('data:'+json.dumps(e, ensure_ascii=False)+'\r\n\r\n' for e in events).encode()
                            if not anthropic:
                                data += b'data:[DONE]\r\n\r\n'
                        self.send_response(200)
                        self.send_header('Content-Type','text/event-stream' if streaming else 'application/json')
                        self.send_header('Content-Length',str(len(data)))
                        self.end_headers()
                        self.wfile.write(data)
                    except Exception as error:
                        failures.append(repr(error))
                        self.send_error(500)

            server = http.server.ThreadingHTTPServer(('127.0.0.1',0),Model)
            thread = threading.Thread(target=server.serve_forever,daemon=True)
            thread.start()
            env = os.environ.copy()
            env.update(HOME=str(root),OSMOGREP_PROVIDER='anthropic' if protocol == 'anthropic-messages' else 'groq', OSMOGREP_API=protocol,OSMOGREP_MODEL='fixture-model',OSMOGREP_BASE_URL=f'http://127.0.0.1:{server.server_port}/v1',ANTHROPIC_API_KEY='fixture-key',GROQ_API_KEY='fixture-key',OSMOGREP_NO_STREAM='0' if streaming else '1',OSMOGREP_MAX_ITERATIONS='4',OSMOGREP_DISABLE_UPDATE_CHECK='1')
            run = subprocess.run([str(binary),'run','--repo-root',str(root),'--prompt','Read a.txt and b.txt and return their exact contents. Use read_file only; do not edit files or run shell commands or tests.','--json-events','--permission-profile','read-only'],env=env,cwd=root,capture_output=True,text=True,timeout=30)
            server.shutdown()
            server.server_close()
            events = [json.loads(line) for line in run.stdout.splitlines() if line.startswith('{')]
            errors = [e for e in events if e.get('type') == 'error']
            # Headless notifications intentionally omit private conversation contents.
            history = [e for e in events if e.get('type') == 'conversation_update']
            tools = [e['name'] for e in events if e.get('type') == 'tool_call']
            displayed = ''.join(e.get('delta',e.get('text','')) for e in events if e.get('type') in ['stream_delta','output_text'])
            result = {'protocol':protocol,'streaming':streaming,'passed':run.returncode == 0 and not failures and not errors and len(requests) == 2 and len(history) == 2 and tools == ['read_file','read_file'] and all(marker in displayed for marker in ['ALPHA_REAL_FILE_123','BETA_REAL_FILE_456']),'model_requests':len(requests),'tools':tools,'fixture_errors':failures,'agent_errors':errors}
            results.append(result)
            print(json.dumps(result),flush=True)

args.output.parent.mkdir(parents=True,exist_ok=True)
args.output.write_text(json.dumps({'scope':'Isolated local HTTP fixtures with actual file tools; no paid provider calls','cases':results},indent=2)+'\n')
assert all(case['passed'] for case in results)
