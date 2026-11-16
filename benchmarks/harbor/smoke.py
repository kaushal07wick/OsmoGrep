"""Exercise the real headless agent through Harbor against localhost fixtures; no paid API.

Run from the repository root with PYTHONPATH=. and --binary target/debug/osmogrep.
"""
import argparse
import asyncio, http.server, json, os, tempfile, threading
from pathlib import Path, PurePosixPath
from types import SimpleNamespace
from unittest.mock import patch
from harbor.models.agent.context import AgentContext
from benchmarks.harbor.osmogrep import OsmoGrepAgent
parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
args = parser.parse_args()
binary = args.binary.resolve(strict=True)
requests = []

class Model(http.server.BaseHTTPRequestHandler):

    def log_message(self, *args):
        pass

    def do_POST(self):
        assert self.path == '/v1/responses', self.path
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        assert self.headers['Authorization'] == 'Bearer fixture-key'
        requests.append(body)
        if len(requests) == 1:
            output = [{'type': 'function_call', 'name': 'read_file', 'call_id': 'benchmark-read', 'arguments': json.dumps({'path': 'notes.txt'})}]
        else:
            assert len(requests) == 2 and 'BENCHMARK_SOURCE_724' in json.dumps(body)
            output = [{'type': 'message', 'role': 'assistant', 'content': [{'type': 'output_text', 'text': 'BENCHMARK_SOURCE_724'}]}]
        response = {'id': f'fixture-{len(requests)}', 'output': output, 'usage': {'input_tokens': 120, 'output_tokens': 30, 'input_tokens_details': {'cached_tokens': 40}}}
        data = ('data: ' + json.dumps({'type': 'response.completed', 'response': response}) + '\n\ndata: [DONE]\n\n').encode()
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)

class LocalEnvironment:
    default_user = None

    def __init__(self, root):
        self.root = root

    async def upload_file(self, source, target):
        Path(target).write_bytes(Path(source).read_bytes())

    async def download_file(self, source, target):
        Path(target).write_bytes(Path(source).read_bytes())

    async def exec(self, command, user=None, env=None, cwd=None, timeout_sec=None):
        clean = {'PATH': str(self.root / 'bin') + ':' + os.defpath, 'HOME': str(self.root / 'home'), 'LITELLM_LOCAL_MODEL_COST_MAP': 'True', **(env or {})}
        process = await asyncio.create_subprocess_exec('/bin/bash', '-c', command, cwd=self.root / 'task', env=clean, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE)
        try:
            out, err = await asyncio.wait_for(process.communicate(), 30)
        except BaseException:
            if process.returncode is None:
                process.kill()
            await process.wait()
            raise
        return SimpleNamespace(return_code=process.returncode, stdout=out.decode(), stderr=err.decode())

async def main():
    server = http.server.HTTPServer(('127.0.0.1', 0), Model)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        with tempfile.TemporaryDirectory(prefix='osmogrep-benchmark-smoke-') as folder:
            root = Path(folder)
            for part in ['bin', 'home', 'task', 'logs', 'remote']:
                (root / part).mkdir()
            (root / 'bin/osmogrep').symlink_to(binary)
            (root / 'task/notes.txt').write_text('BENCHMARK_SOURCE_724\n')
            with patch.dict(os.environ, {'OSMOGREP_API_KEY': 'fixture-key', 'LITELLM_LOCAL_MODEL_COST_MAP': 'True'}, clear=True):
                agent = OsmoGrepAgent(logs_dir=root / 'logs', environment_logs_dir=PurePosixPath(root / 'remote'), model_name='openai/fixture', base_url=f'http://127.0.0.1:{server.server_port}/v1')
                context = AgentContext()
                try:
                    await agent.run('Read notes.txt and report its text.', LocalEnvironment(root), context)
                except Exception:
                    print('STDERR', (root / 'logs/stderr.log').read_text()[-2000:])
                    print('EVENTS', (root / 'logs/events.jsonl').read_text()[-3000:])
                    raise
            assert len(requests) == 2
            assert (context.n_input_tokens, context.n_output_tokens, context.n_cache_tokens) == (240, 60, 80)
            events = (root / 'logs/events.jsonl').read_text()
            assert 'BENCHMARK_SOURCE_724' in events and 'read_file' in events
            assert context.cost_usd is None
            print(json.dumps({'real_osmogrep_loop': True, 'harbor_adapter': True, 'local_fixture_requests': len(requests), 'reported_tokens': [240, 60, 80], 'paid_model_requests': 0}))
    finally:
        server.shutdown()
asyncio.run(main())
