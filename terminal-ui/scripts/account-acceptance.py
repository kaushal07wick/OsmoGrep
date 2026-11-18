"""Exercise TUI-first device login/cancellation with a local server and fake browser.
No real browser opens, model calls, successful grants or credential writes.
"""
import argparse
import fcntl
import hashlib
import http.server
import json
import os
from pathlib import Path
import pty
import select
import shlex
import signal
import struct
import subprocess
import tempfile
import termios
import threading
import time
from urllib.parse import parse_qs

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
binary = args.binary.resolve()
root = Path(tempfile.mkdtemp(prefix='osmogrep-account-tui-')).resolve()
home = root/'home'
(home/'.codex').mkdir(parents=True)
opened = root/'browser-url'
launcher = home/'.codex/open-personal-link'
launcher.write_text('#!/bin/sh\nprintf "%s" "$1" > '+shlex.quote(str(opened))+'\n')
launcher.chmod(0o700)
requests, failures = [], []

class Account(http.server.BaseHTTPRequestHandler):
    def log_message(self, *unused):
        pass

    def do_POST(self):
        try:
            form = parse_qs(self.rfile.read(int(self.headers['Content-Length'])).decode())
            requests.append((self.path, form))
            assert form['client_id'] == ['osmogrep-cli'], form
            assert not self.headers.get('Authorization')
            if self.path == '/api/cli/auth/device':
                assert form['scope'] == ['account:read']
                uri = f'http://127.0.0.1:{self.server.server_port}/cli/authorize'
                value = {'device_code':'private-device-fixture','user_code':'ABCD-EFGH',
                         'verification_uri':uri,'verification_uri_complete':uri+'?user_code=ABCD-EFGH',
                         'interval':1,'expires_in':60}
                status = 200
            else:
                assert self.path == '/api/cli/auth/token', self.path
                assert form['device_code'] == ['private-device-fixture']
                assert form['grant_type'] == ['urn:ietf:params:oauth:grant-type:device_code']
                value, status = {'error':'authorization_pending'}, 400
            body = json.dumps(value).encode()
            self.send_response(status)
            self.send_header('Content-Type','application/json')
            self.send_header('Content-Length',str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        except Exception as error:
            failures.append(repr(error))
            self.send_error(500)

server = http.server.ThreadingHTTPServer(('127.0.0.1',0), Account)
threading.Thread(target=server.serve_forever,daemon=True).start()
environment = {k:v for k,v in os.environ.items() if not k.startswith((
    'OSMOGREP_','OPENAI_','ANTHROPIC_','OLLAMA_','VLLM_REALTIME_'))}
environment.update(HOME=str(home),XDG_CONFIG_HOME=str(root/'config'),TERM='xterm-ghostty',
                   TERM_PROGRAM='ghostty',COLORTERM='truecolor',OSMOGREP_UI='typescript',
                   OSMOGREP_DISABLE_UPDATE_CHECK='1',OSMOGREP_SANDBOX='off',
                   OSMOGREP_ACCOUNT_URL=f'http://127.0.0.1:{server.server_port}',
                   NO_PROXY='127.0.0.1,localhost')

def controlling():
    os.setsid()
    fcntl.ioctl(0,termios.TIOCSCTTY,0)

class Terminal:
    def __init__(self):
        self.master, slave = pty.openpty()
        fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',36,120,1200,864))
        self.process = subprocess.Popen([str(binary)],cwd=root,env=environment,
                                        stdin=slave,stdout=slave,stderr=slave,preexec_fn=controlling)
        os.close(slave)
        self.output = bytearray()

    def drain(self, seconds=.1):
        deadline = time.monotonic()+seconds
        while time.monotonic()<deadline:
            if select.select([self.master],[],[],min(.05,max(0,deadline-time.monotonic())))[0]:
                try:
                    data = os.read(self.master,1000000)
                except OSError:
                    return
                self.output.extend(data)
                if b'\x1b[6n' in data:
                    os.write(self.master,b'\x1b[1;1R')

    def wait(self, condition, seconds=12):
        deadline = time.monotonic()+seconds
        while not condition() and time.monotonic()<deadline:
            self.drain()
            assert self.process.poll() is None, self.output[-3000:].decode(errors='replace')
        assert condition(), self.output[-4000:].decode(errors='replace')

    def send(self, text):
        os.write(self.master,text)

    def close(self):
        if self.process.poll() is None:
            self.send(b'\x15/exit\r')
            deadline = time.monotonic()+3
            while self.process.poll() is None and time.monotonic()<deadline:
                self.drain()
        if self.process.poll() is None:
            os.killpg(self.process.pid,signal.SIGTERM)
        self.process.wait(timeout=5)
        os.close(self.master)

checks=[]
terminal=None
try:
    terminal=Terminal()
    terminal.wait(lambda:b'Welcome to OsmoGrep' in terminal.output)
    terminal.drain(.4)
    assert not requests, requests
    assert b'Sign in with browser' in terminal.output
    checks.append('fresh_tui_offers_login_without_network_or_forced_account')
    terminal.send(b'\r')
    terminal.wait(lambda:opened.exists() and b'ABCD-EFGH' in terminal.output)
    assert opened.read_text()==f'http://127.0.0.1:{server.server_port}/cli/authorize?user_code=ABCD-EFGH'
    assert b'private-device-fixture' not in terminal.output
    checks.append('tui_login_displays_code_and_uses_browser_launcher_without_private_device_code')
    terminal.wait(lambda:any(path.endswith('/token') for path,_ in requests))
    terminal.send(b'\x1b')
    terminal.drain(.3)
    terminal.send(b'account_cancel_kept_input')
    terminal.wait(lambda:b'account_cancel_kept_input' in terminal.output)
    terminal.drain(1.2)
    count=len(requests)
    terminal.drain(1.2)
    assert len(requests)==count, requests
    checks.append('escape_cancels_polling_and_restores_composer_without_credential_writes')
    (root/'first-terminal.ansi').write_bytes(terminal.output)
    terminal.close()
    terminal=Terminal()
    terminal.drain(1.5)
    assert b'Welcome to OsmoGrep' not in terminal.output
    terminal.send(b'existing_install_opens_chat')
    terminal.wait(lambda:b'existing_install_opens_chat' in terminal.output)
    assert len(requests)==count
    checks.append('existing_install_reopens_chat_without_account_checks')
    before = len(requests)
    terminal.send(b'\x15/login\r')
    terminal.wait(lambda:any(path.endswith('/device') for path,_ in requests[before:]) and b'ABCD-EFGH' in terminal.output)
    terminal.send(b'\x1b')
    terminal.drain(.2)
    terminal.send(b'\x1b')
    terminal.drain(.2)
    terminal.send(b'slash_login_returned_to_chat')
    terminal.wait(lambda:b'slash_login_returned_to_chat' in terminal.output)
    checks.append('slash_login_opens_tui_authorization_without_a_model_turn')
    assert not failures, failures
    report={'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'checks':checks,
            'account_requests':len(requests),'real_browser_opens':0,'credential_writes':0,
            'model_requests':0,'paid_calls':0,'fixture':str(root),
            'scope':'Actual TUI/process/browser-launcher boundary with a deterministic local account server; no successful token grant or live website login.'}
    args.output.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'checks':len(checks),'account_requests':len(requests),'paid_calls':0}))
finally:
    if terminal:terminal.close()
    server.shutdown()
