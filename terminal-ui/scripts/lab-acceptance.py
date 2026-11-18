"""Exercise Lab through the real CLI and two RPC agents using only a loopback model."""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import queue
import sqlite3
import subprocess
import tempfile
import threading
import time

parser = argparse.ArgumentParser()
parser.add_argument('--binary', required=True, type=Path)
parser.add_argument('--output', required=True, type=Path)
args = parser.parse_args()
binary = args.binary.resolve()
requests, results, failures, checks = [], {}, [], []


def call(name, arguments):
    return {'type': 'function_call', 'call_id': str(time.monotonic_ns()),
            'name': name, 'arguments': json.dumps(arguments)}


class Model(http.server.BaseHTTPRequestHandler):
    def log_message(self, *unused):
        pass

    def do_POST(self):
        try:
            body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            requests.append(body)
            items = body['input']
            index = max(i for i, item in enumerate(items) if item.get('role') == 'user')
            prompt = str(items[index]['content'])
            if 'LAB_REQUEST ' not in prompt:
                self.natural(body, items, index, prompt)
                return
            raw = prompt.split('LAB_REQUEST ', 1)[1]
            request = json.loads(raw)
            outputs = [json.loads(item['output']) for item in items[index + 1:]
                       if item.get('type') == 'function_call_output']
            if request.get('edit'):
                if not outputs:
                    output = [call('read_file', {'path': 'app.py'})]
                elif len(outputs) == 1:
                    output = [call('write_file', {'path': 'app.py', 'content': request['edit']})]
                else:
                    assert 'error' not in outputs[-1], outputs[-1]
                    results[request['label']] = outputs[-1]
                    output = []
            elif not any(tool['name'] == 'lab' for tool in body['tools']):
                output = [call('tool_search', {'query': 'lab'})]
            elif not outputs or outputs[-1].get('tools'):
                output = [call('lab', request['request'])]
            else:
                assert 'error' not in outputs[-1], outputs[-1]
                results[request['label']] = outputs[-1]
                output = []
            if not output:
                output = [{'type': 'message', 'role': 'assistant', 'content': [
                    {'type': 'output_text', 'text': 'Fixture action finished.'}]}]
            data = json.dumps({'output': output, 'usage': {'input_tokens': 31, 'output_tokens': 7}}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)
        except Exception as error:
            failures.append(repr(error))
            self.send_error(500, str(error))

    def natural(self, body, items, index, prompt):
        cases = {
            'hello': ('greeting', None),
            'Why are you assuming that? Check the context you have.': ('context', 'context provenance'),
            'Work with Agent B on the shared interface.': ('contract', 'teammate interface contract'),
            'Compare the saved Value fix A and Value fix B candidates; prepare the comparison without running or applying it.': ('comparison', 'compare alternative fixes'),
            'Prepare failure cases for the saved Value fix A bug, but do not run them.': ('scenario', 'fault testing'),
            'Prepare a repeatable check for the iPhone app using the existing dedicated simulator named test; do not run it yet.': ('behavior', 'iphone ui flows'),
        }
        label, query = cases[prompt]
        outputs = [json.loads(item['output']) for item in items[index + 1:]
                   if item.get('type') == 'function_call_output']
        if label == 'greeting':
            assert not body['tools']
            output = []
        elif not outputs:
            descriptor = next(tool for tool in body['tools'] if tool['name'] == 'tool_search')
            assert 'context provenance' in descriptor['description']
            assert not any(tool['name'] == 'lab' for tool in body['tools'])
            output = [call('tool_search', {'query': query})]
        else:
            guidance = outputs[0]['workflow']
            kinds = {workflow['kind'] for workflow in guidance['workflows']}
            assert label in kinds, (label, guidance)
            assert 'grants no execution' in guidance['policy']
            assert 'error' not in outputs[-1], outputs[-1]
            if label == 'context' and len(outputs) == 1:
                assert guidance['current_request_metadata']['persistent_record_created'] is False
                assert guidance['current_request_metadata']['provenance']['items']
                output = [call('lab', {'action': 'current_context'})]
            elif label == 'contract' and len(outputs) == 1:
                target = next(peer for peer in guidance['live_routing']['open_peers'] if peer['role'] == 'Agent B')
                output = [call('team', {'action': 'send', 'recipient': target['session'],
                                       'body': 'NATURAL_COORDINATION: review the shared interface within the existing task.',
                                       'message_id': 'natural-work-with-peer'})]
            elif label == 'comparison' and len(outputs) <= 2:
                title = 'Value fix A' if len(outputs) == 1 else 'Value fix B'
                target = next(record for record in guidance['recent_records']
                              if record['kind'] == 'replay' and record['title'] == title)
                output = [call('lab', {'action': 'show', 'id': target['id']})]
            elif label == 'comparison' and len(outputs) == 3:
                output = [call('lab', {'action': 'plan_comparison', 'title': 'Natural comparison',
                                      'data': {'left': outputs[1]['id'], 'right': outputs[2]['id']}})]
            elif label == 'scenario' and len(outputs) == 1:
                target = next(record for record in guidance['recent_records']
                              if record['kind'] == 'replay' and record['title'] == 'Value fix A')
                output = [call('lab', {'action': 'show', 'id': target['id']})]
            elif label == 'scenario' and len(outputs) == 2:
                output = [call('lab', {'action': 'plan_scenario', 'title': 'Natural failure cases',
                                      'data': {'replay': outputs[1]['id'], 'cases': [
                                          {'name': 'UTC', 'environment': {'TZ': 'UTC'}}]}})]
            elif label == 'behavior' and len(outputs) == 1:
                output = [call('lab', {'action': 'plan_flow', 'title': 'Natural iPhone check',
                                      'data': {'platform': 'iphone', 'target': 'test', 'sources': ['app.py'],
                                               'steps': [{'tool': 'phone_observe', 'arguments': {'includeImage': True}}]}})]
            else:
                results['natural-' + label] = outputs[-1]
                output = []
        if not output:
            results.setdefault('natural-' + label, {})
            output = [{'type': 'message', 'role': 'assistant', 'content': [
                {'type': 'output_text', 'text': 'The requested step is complete; working source is preserved.'}]}]
        data = json.dumps({'output': output, 'usage': {'input_tokens': 31, 'output_tokens': 7}}).encode()
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)


class Peer:
    def __init__(self, root, environment, team=True, profile="workspace-auto"):
        command = [str(binary)] + (['--team'] if team else []) + [
            'rpc', '--repo-root', str(root), '--permission-profile', profile, '--auto-approve']
        self.process = subprocess.Popen(command, cwd=root, env=environment,
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.PIPE, text=True)
        self.events, self.errors = queue.Queue(), []
        def read():
            for line in self.process.stdout:
                try:
                    self.events.put(json.loads(line))
                except ValueError:
                    self.errors.append(line)
        threading.Thread(target=read, daemon=True).start()
        threading.Thread(target=lambda: self.errors.extend(self.process.stderr.readlines()), daemon=True).start()
        self.session = self.wait(lambda event: event.get('type') == 'ready')['session_id']

    def wait(self, predicate):
        deadline = time.monotonic() + 25
        while time.monotonic() < deadline:
            try:
                event = self.events.get(timeout=.1)
            except queue.Empty:
                assert self.process.poll() is None, self.errors
                continue
            assert event.get('type') != 'error', event
            if predicate(event):
                return event
        raise AssertionError(f'RPC timeout: {self.errors}')

    def request(self, method, params=None):
        identity = str(time.monotonic_ns())
        self.process.stdin.write(json.dumps({'jsonrpc': '2.0', 'id': identity,
                                             'method': method, 'params': params or {}}) + '\n')
        self.process.stdin.flush()
        response = self.wait(lambda event: event.get('id') == identity)
        assert 'error' not in response, response
        return response['result']

    def act(self, label, request=None, edit=None, no_message=False):
        prefix = 'Do not tell them anything. ' if no_message else ''
        self.request('prompt', {'prompt': prefix + 'LAB_REQUEST ' + json.dumps(
            {'label': label, 'request': request, 'edit': edit})})
        self.wait(lambda event: event.get('type') == 'done')
        assert label in results, (label, failures)
        return results[label]

    def natural(self, prompt):
        self.request('prompt', {'prompt': prompt})
        self.wait(lambda event: event.get('type') == 'done')

    def close(self):
        if self.process.poll() is None:
            self.request('shutdown')
            self.process.wait(timeout=10)


with tempfile.TemporaryDirectory(prefix='osmogrep-lab-acceptance-') as folder:
    root = Path(folder)
    project, home = root/'project', root/'home'
    project.mkdir(); home.mkdir()
    (project/'app.py').write_text('VALUE = 1\n')
    (project/'CONTRACT.md').write_text('The public value is one.\n')
    environment = os.environ.copy()
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Model)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    environment.update(HOME=str(home), XDG_CONFIG_HOME=str(home/'.config'),
                       OPENAI_API_KEY='fixture-key', OSMOGREP_PROVIDER='openai',
                       OSMOGREP_MODEL='fixture-model', OSMOGREP_API='responses',
                       OSMOGREP_BASE_URL=f'http://127.0.0.1:{server.server_port}/v1',
                       OSMOGREP_NO_STREAM='1', OSMOGREP_DISABLE_UPDATE_CHECK='1',
                       OSMOGREP_MAX_ITERATIONS='8', GIT_CONFIG_NOSYSTEM='1',
                       GIT_CONFIG_GLOBAL='/dev/null')
    def cli(*command, fail=False):
        result = subprocess.run([str(binary), 'lab', '--repo-root', str(project), *command],
                                cwd=project, env=environment, capture_output=True, text=True, timeout=30)
        assert (result.returncode != 0) == fail, (command, result.stdout, result.stderr)
        return json.loads(result.stdout) if result.stdout.strip() else None
    def execute(request, create=False, fail=False):
        spec = root/'request.json'; spec.write_text(json.dumps(request))
        return cli('create' if create else 'execute', str(spec), fail=fail)
    for command in [['init', '--template='], ['add', '--all'],
                    ['-c', 'user.name=Kaushal Choudhary', '-c', 'user.email=kaushalc64@gmail.com',
                     'commit', '-qm', 'Fixture baseline']]:
        subprocess.run(['git', '-C', str(project), *command], env=environment,
                       check=True, capture_output=True)
    peers = []
    try:
        assert 'context' in cli('guide')
        assert not list(home.rglob('lab.sqlite3'))
        a, b = Peer(project, environment), Peer(project, environment)
        peers += [a, b]
        assert not list(home.rglob('lab.sqlite3'))
        checks.append('help_and_idle_agents_do_not_create_lab_storage')
        observer = Peer(project, environment, team=False, profile='read-only'); peers.append(observer)
        observer.natural('hello')
        observer.natural('Why are you assuming that? Check the context you have.')
        assert results['natural-context']['persistent_record_created'] is False
        assert results['natural-context']['provenance']['items']
        assert not list(home.rglob('lab.sqlite3'))
        checks.append('natural_context_discovery_and_inspection_work_read_only_without_recording_or_storage')

        contract = a.act('create', {'action': 'create_contract', 'title': 'Value interface',
                                    'data': {'definition': 'VALUE: int = 1', 'sources': ['app.py'],
                                             'participants': [b.session], 'consumers': {b.session: ['client.py']}}})
        identity = contract['id']
        fingerprint = cli('check-contract', identity)['definition_sha256']
        for peer, label in [(a, 'accept-a'), (b, 'accept-b')]:
            peer.act(label, {'action': 'accept_contract', 'id': identity,
                             'data': {'definition_sha256': fingerprint, 'subscribe': True}})
        a.act('edit', edit='VALUE = 9\n')
        inbox = b.request('team', {'action': 'inbox'})['messages']
        assert len(inbox) == 1 and identity in inbox[0]['body'], inbox
        assert 'reference' in inbox[0]['body'].lower()
        count = len(requests)
        b.request('team', {'action': 'history'})
        assert len(requests) == count
        checks.append('accepted_contract_drift_is_delivered_once_to_the_subscriber_without_an_idle_model_call')
        a.act('amend', {'action': 'amend_contract', 'id': identity,
                        'data': {'definition': 'VALUE: int; revision two'}}, no_message=True)
        fingerprint = cli('check-contract', identity)['definition_sha256']
        a.act('accept-new', {'action': 'accept_contract', 'id': identity,
                             'data': {'definition_sha256': fingerprint, 'subscribe': True}})
        a.act('private-edit', edit='VALUE = 10\n', no_message=True)
        assert len(b.request('team', {'action': 'inbox'})['messages']) == 1
        checks.append('contract_subscriptions_respect_explicit_no_messaging')
        execute({'action': 'configure_context', 'data': {'enabled': True}})
        secret = 'sk-context-fixture-only-do-not-store'
        a.act('capture-context', {'action': 'help', 'kind': 'context',
                                 'data': {'note': f'api_key={secret}'}})
        database = next(home.rglob('lab.sqlite3'))
        def context_rows():
            with sqlite3.connect(database) as connection:
                return [json.loads(row[0]) for row in connection.execute(
                    "SELECT data FROM lab_records WHERE kind='context' AND json_extract(data,'$.type')='request'")]
        evidence = context_rows()
        assert evidence and all(item['state'] == 'finished' for item in evidence), [item.get('state') for item in evidence]
        assert all(item['usage']['input_tokens'] == 31 for item in evidence)
        assert secret not in json.dumps(evidence)
        assert all(item['provenance']['items'] for item in evidence)
        execute({'action': 'configure_context', 'data': {'enabled': False}})
        count = len(evidence)
        a.act('capture-off', {'action': 'help', 'kind': 'context'})
        assert len(context_rows()) == count
        assert not (project/'.context/osmogrep-lab/context.json').exists()
        checks.append('opt_in_context_captures_actual_requests_usage_and_provenance_redacts_secrets_and_turns_off')
        spec = {'action': 'capture_replay', 'title': 'Value fix A', 'data': {
            'paths': ['app.py'], 'files': {'app.py': 'VALUE = 2\n'},
            'experiment': {'path': 'osmogrep_repro_value.py',
                           'source': 'from app import VALUE\nassert VALUE == 1\n',
                           'command': 'python3 osmogrep_repro_value.py',
                           'oracle': 'The pre-existing public value contract',
                           'oracle_sources': ['CONTRACT.md:1']}}}
        replay = execute(spec, create=True)
        cli('export', replay['id'], 'replay.json')
        imported = cli('import', 'replay.json')
        assert imported['id'] != replay['id']
        package = json.loads((project/'replay.json').read_text())
        package['report']['title'] = 'tampered'; (project/'tampered.json').write_text(json.dumps(package))
        cli('import', 'tampered.json', fail=True)
        spec['title'] = 'Value fix B'
        spec['data']['files']['app.py'] = 'VALUE = 3\n'
        second = execute(spec, create=True)
        plan = cli('compare', replay['id'], second['id'])
        assert plan['data']['state'] == 'planned'
        (project/'app.py').write_text('USER_EDIT = True\n')
        cli('apply', replay['id'], fail=True)
        assert (project/'app.py').read_text() == 'USER_EDIT = True\n'
        execution = cli('replay', replay['id'], '--server', 'unavailable', '--timeout-seconds', '1', fail=True)
        assert execution['data']['state'] == 'failed'
        assert (project/'app.py').read_text() == 'USER_EDIT = True\n'
        checks.append('cli_capture_import_tamper_rejection_comparison_and_stale_apply_preserve_source')
        checks.append('unavailable_vm_saves_failure_without_executing_the_reproducer_on_host')
        collaborator = Peer(project, environment, team=False); peers.append(collaborator)
        before = (project/'app.py').read_bytes()
        collaborator.natural('Work with Agent B on the shared interface.')
        assert any('NATURAL_COORDINATION' in message['body']
                   for message in b.request('team', {'action': 'inbox'})['messages'])
        collaborator.natural('Compare the saved Value fix A and Value fix B candidates; prepare the comparison without running or applying it.')
        assert results['natural-comparison']['data']['state'] == 'planned'
        collaborator.natural('Prepare failure cases for the saved Value fix A bug, but do not run them.')
        assert results['natural-scenario']['data']['state'] == 'planned'
        collaborator.natural('Prepare a repeatable check for the iPhone app using the existing dedicated simulator named test; do not run it yet.')
        assert results['natural-behavior']['data']['state'] == 'planned'
        assert (project/'app.py').read_bytes() == before
        checks.append('natural_requests_chain_saved_ids_and_guides_without_slash_commands_or_preparation_side_effects')
        checks.append('work_with_named_peer_routes_without_persistent_team_mode')
        assert not failures, failures
        args.output.write_text(json.dumps({'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                                          'checks': checks, 'fixture_model_requests': len(requests),
                                          'paid_calls': 0, 'real_device_runs': 0,
                                          'real_vm_runs': 0}, indent=2) + '\n')
        print(json.dumps({'checks': len(checks), 'fixture_model_requests': len(requests), 'paid_calls': 0}))
    finally:
        for peer in peers:
            peer.close()
        server.shutdown(); server.server_close()
