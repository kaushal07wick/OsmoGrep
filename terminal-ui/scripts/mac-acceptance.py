"""Exercise the real app/TypeScript process/editor with a local model endpoint.

macOS only. No real model credentials or clipboard mutations. Artifacts are local.
"""
import argparse
import base64
import errno
import fcntl
import http.server
import hashlib
import json
import os
from pathlib import Path
import pty
import re
import select
import signal
import struct
import subprocess
import tempfile
import termios
import threading
import time
import zlib

p = argparse.ArgumentParser()
p.add_argument('--binary', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
args = p.parse_args()
binary = args.binary.resolve()
args.output.mkdir(parents=True, exist_ok=True)
repo = Path(tempfile.mkdtemp(prefix='osmogrep-ui-acceptance-')).resolve()
subprocess.run(['git', 'init', '-q', str(repo)], check=True)

def chunk(kind, data):
    return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind+data))
png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB',24,12,8,2,0,0,0))
png += chunk(b'IDAT', zlib.compress((b'\x00'+bytes((42,120,190))*24)*12)) + chunk(b'IEND', b'')
image = repo/'test image.png'
image.write_bytes(png)
requests = []
skill_dir=repo/'skills/ui-probe'
skill_dir.mkdir(parents=True)
(skill_dir/'SKILL.md').write_text('---\nname: ui-probe\ndescription: Controlled UI acceptance skill\n---\nUI_SKILL_INSTRUCTION: inspect only the requested local image.\n')

class Model(http.server.BaseHTTPRequestHandler):
    permission_issued=False
    def log_message(self, *unused):
        pass
    def do_GET(self):
        assert self.path == '/models'
        assert self.headers.get('Authorization') == 'Bearer local-ui-test-key'
        data=json.dumps({'data':[{'id':'local-ui-test-model'},{'id':'local-ui-alternate'}]}).encode()
        self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        requests.append(body)
        latest=next((m.get('content','') for m in reversed(body['input']) if m.get('role')=='user'),'')
        latest=latest if isinstance(latest,str) else json.dumps(latest)
        value = {'id':'local_ui_response','usage':{'input_tokens':1234,'output_tokens':56,'input_tokens_details':{'cached_tokens':200}},'output':[{'type':'message','role':'assistant','content':[{'type':'output_text','text':'IMAGE_REQUEST_OK'}]}]}
        def message(text): return [{'type':'message','role':'assistant','content':[{'type':'output_text','text':text}]}]
        prefix=b'';delay=0
        if latest=='hello':
            assert body['tool_choice']=='none' and body['tools']==[]
            assert 'obsolete instructions' not in json.dumps(body['input'][0]).lower()
            value.pop('usage');value['output']=message('GREETING_ONLY_OK')
        elif latest.startswith('QUEUE_'):
            value.pop('usage');value['output']=message(latest+'_COMPLETE')
            if latest=='QUEUE_FIRST':
                prefix=('data: '+json.dumps({'type':'response.output_text.delta','delta':'QUEUE_FIRST_'})+'\n\n').encode();delay=3
                tail=('data: '+json.dumps({'type':'response.output_text.delta','delta':'COMPLETE'})+'\n\n').encode()
            else: tail=b''
        elif latest=='STREAM_SCROLL_PROBE':
            value.pop('usage')
            beginning='\n\n'.join(f'STREAM_LINE_{i:03}' for i in range(80))+'\n\n'
            ending='\n\n'.join(f'STREAM_LINE_{i:03}' for i in range(80,100))+'\n'
            prefix=('data: '+json.dumps({'type':'response.output_text.delta','delta':beginning})+'\n\n').encode();delay=3
            tail=('data: '+json.dumps({'type':'response.output_text.delta','delta':ending})+'\n\n').encode()
            value['output']=message(beginning+ending)
        elif latest=='READ_PROBE':
            value.pop('usage')
            if any(m.get('type')=='function_call_output' for m in body['input']):
                assert 'CONVERSATION_SOURCE' in json.dumps(body['input'])
                value['output']=message('READ_PROBE_OK')
            else:
                value['output']=message('I will read the requested file.')+[{'type':'function_call','name':'read_file','call_id':'read_probe','arguments':json.dumps({'path':'conversation.txt'})}]
        elif latest=='SCROLL_PROBE':
            value.pop('usage');value['output']=message('\n\n'.join(f'SCROLL_LINE_{i:03}' for i in range(200)))
        elif latest=='UI_PERMISSION_PROBE':
            if not Model.permission_issued:
                Model.permission_issued=True
                value['output']=[{'type':'function_call','name':'edit_file','call_id':'permission_probe','arguments':json.dumps({'path':'permission.txt','old':'before','new':'after'})}]
            else: value['output']=message('PERMISSION_FLOW_OK')
        data = (('data: '+json.dumps({'type':'response.completed','response':value})+'\n\ndata: [DONE]\n\n').encode())
        if latest in ['QUEUE_FIRST','STREAM_SCROLL_PROBE']: data=tail+data
        self.send_response(200);self.send_header('Content-Type','text/event-stream');self.send_header('Content-Length',str(len(prefix)+len(data)));self.end_headers()
        if prefix: self.wfile.write(prefix);self.wfile.flush();time.sleep(delay)
        self.wfile.write(data)

server = http.server.HTTPServer(('127.0.0.1',0),Model)
thread = threading.Thread(target=server.serve_forever,daemon=True)
thread.start()
master, slave = pty.openpty()
fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',44,160,0,0))
before = termios.tcgetattr(slave)
test_home=repo/'test-home';test_home.mkdir()
env = os.environ.copy()
env['HOME']=str(test_home)
env.update({'TERM':'xterm-ghostty','TERM_PROGRAM':'ghostty','COLORTERM':'truecolor',
            'OPENAI_API_KEY':'local-ui-test-key','OSMOGREP_BASE_URL':f'http://127.0.0.1:{server.server_port}',
            'OSMOGREP_PROVIDER':'openai','OSMOGREP_MODEL':'local-ui-test-model','OSMOGREP_MAX_ITERATIONS':'3',
            'OSMOGREP_UI':'typescript','OSMOGREP_DISABLE_UPDATE_CHECK':'1','VLLM_REALTIME_AUTOCONNECT':'0',
            'NO_PROXY':'127.0.0.1,localhost'})
env.pop('VLLM_REALTIME_PROXY_LISTEN',None)
env.pop('OSMOGREP_MOUSE',None)
env.pop('TMUX',None);env.pop('STY',None)
env['OSMOGREP_CLIPBOARD_OSC52']='1'
state_path=test_home/'Library/Application Support/osmogrep/sessions'/(hashlib.sha256(str(repo).encode()).hexdigest()+'.json')
state_path.parent.mkdir(parents=True,exist_ok=True)
state_path.write_text(json.dumps({'auto_eval':True,'session_changes':[{'tool':'edit_file','target':'old.rs','before':'old','after':'new'}],'conversation':[{'role':'system','content':'Obsolete instructions: always inspect the repository'},{'role':'user','content':'An old task'},{'role':'assistant','content':'Old task finished'}]}))
(repo/'.context').mkdir(exist_ok=True)
(repo/'.context/osmogrep-verification.jsonl').write_text(json.dumps({'type':'workspace_edited','id':'old-source-edit','edited_at':'2020-01-01T00:00:00Z','root':str(repo),'changed_paths':['old.rs']})+'\n')

def controlling_terminal():
    os.setsid()
    fcntl.ioctl(0,termios.TIOCSCTTY,0)

wrapper = '"$1"; status=$?; printf "\\nOSMOGREP_UI_EXIT=%s\\n" "$status"; IFS= read -r ignored; exit "$status"'
process = subprocess.Popen(['sh','-c',wrapper,'osmogrep-ui-check',str(binary)],cwd=repo,env=env,
                           stdin=slave,stdout=slave,stderr=slave,preexec_fn=controlling_terminal)
transcript = bytearray()

def drain(seconds=.3):
    end = time.monotonic()+seconds
    while time.monotonic()<end:
        if select.select([master],[],[],.02)[0]:
            try:
                data = os.read(master,65536)
            except OSError as e:
                if e.errno==errno.EIO: break
                raise
            if not data: break
            transcript.extend(data)
            if b'\x1b[6n' in data: os.write(master,b'\x1b[1;1R')
        if process.poll() is not None: break

def send(data):
    os.write(master,data)
    drain()

def paste(text):
    send(b'\x1b[200~'+text.encode()+b'\x1b[201~')

def wait_for(check, seconds=12):
    end=time.monotonic()+seconds
    while not check() and time.monotonic()<end: drain(.1)
    assert check(),bytes(transcript[-2000:])

def graphics(data):
    frames=[]
    pending=b''
    displaying=False
    for controls,payload in re.findall(rb'\x1b_G([^;\x1b]*);([^\x1b]*)\x1b\\',data):
        if b'a=' in controls:
            displaying=b'a=T' in controls
            pending=b''
        if not displaying: continue
        if payload: pending+=payload
        if pending and b'm=1' not in controls:
            frames.append(base64.b64decode(pending))
            pending=b''
    return frames

def plain(data):
    return re.sub(rb'\x1b\[[0-?]*[ -/]*[@-~]',b'',data)

result = {}
try:
    wait_for(lambda:b'osmogrep' in transcript and b'local-ui-test-model' in transcript)
    assert b'Native UI restored' not in transcript
    result['typescript_renderer_started'] = True
    first = len(transcript)
    drain(.7)
    assert graphics(transcript[first:]), 'fine startup frames must use terminal graphics'
    assert b'\x1b[?1002h' in transcript and b'\x1b[?1003h' not in transcript, 'idle artwork must request buttons, not hover events'
    drain(10)
    assert len(set(graphics(transcript)))>3, 'startup must advance through distinct morph poses automatically'
    settled = len(transcript)
    drain(.4)
    assert not graphics(transcript[settled:]), 'central animation must settle after ten seconds'
    assert b'a=t,' in transcript and b'z=-1073741825' in transcript, 'native paper must be uploaded behind text'
    assert b'a=t,' not in transcript[settled:], 'stationary paper pixels must be retained rather than re-uploaded'
    result['native_paper_pixels_cached'] = True
    settled_image=graphics(transcript)[-1]
    assert struct.unpack('>II',settled_image[16:24])==(60*6,30*12), 'fine image must have three times the linear dot resolution'
    result['central_morph_animates_then_settles'] = True
    before_hover=len(transcript)
    send(b'\x1b[<35;85;21M')
    assert all(frame==settled_image for frame in graphics(transcript[before_hover:])), 'hover must not change the pose'
    after_hover=len(transcript)
    drain(.4)
    assert not graphics(transcript[after_hover:]), 'hover must not start playback'
    before_click=len(transcript)
    send(b'\x1b[<0;80;21M')
    drain(.7)
    assert graphics(transcript[before_click:]), 'click must replay the morph'
    result['central_morph_ignores_hover_and_click_replays'] = True
    send(b'\x1b\x1b')
    assert process.poll() is None and b'OSMOGREP_UI_EXIT' not in transcript
    result['escape_does_not_exit_idle_app']=True
    at=len(transcript);paste('hello');send(b'\r')
    wait_for(lambda:b'GREETING_ONLY_OK' in plain(transcript[at:]))
    drain(.3)
    assert len(requests)==1 and requests[0]['tool_choice']=='none'
    assert b'Auto-review queued' not in transcript[at:] and b'Auto-eval queued' not in transcript[at:]
    assert not json.loads(state_path.read_text()).get('auto_eval',True)
    result['greeting_has_no_tools_or_stale_background_jobs']=True
    requests.clear()
    paste('/')
    wait_for(lambda: b'Commands' in transcript and b'move' in transcript)
    assert b'\x1b[?1003h' not in transcript, 'selectors must capture buttons without hover tracking'
    send(b'\x1b[B' * 2)
    assert '›'.encode() in transcript
    send(b'\x1b[A' * 2)
    send(b'\r')
    wait_for(lambda: b'WORKSPACE' in transcript and b'Search' in transcript)
    paste('permissions')
    send(b'\r')
    wait_for(lambda: b'Tool classifications govern access' in transcript or b'Ask for approval' in transcript)
    send(b'\x1b')
    drain(.1)
    assert not requests, 'selecting /help must execute locally'
    result['command_picker_navigation_and_execution'] = True
    # Footer hit regions open independent model/provider selectors and use real catalog metadata.
    at=len(transcript)
    send(b'\x1b[<0;5;44M')
    wait_for(lambda:b'Choose model' in plain(transcript[at:]) and b'local-ui-alternate' in plain(transcript[at:]))
    paste('local-ui-alternate')
    send(b'\x1b[<0;8;10M')
    wait_for(lambda:b'Switched model: provider=openai model=local-ui-alternate' in plain(transcript[at:]))
    # The new shorter model label places its provider chip at columns 27 onward.
    at=len(transcript)
    send(b'\x1b[<0;29;44M')
    wait_for(lambda:b'Choose provider' in plain(transcript[at:]))
    paste('openai');send(b'\r')
    wait_for(lambda:b'Choose model' in plain(transcript[at:]))
    paste('local-ui-test-model');send(b'\r')
    wait_for(lambda:b'Switched model: provider=openai model=local-ui-test-model' in plain(transcript[at:]))
    assert not requests,'selector metadata requests must not run the agent'
    result['footer_provider_model_selectors'] = True
    # Appearance changes use the existing persisted workspace setting.
    send(b'\x1bOQ')  # F2
    paste('Light')
    send(b'\r')
    wait_for(lambda:b'\x1b[48;2;237;235;230m' in transcript)
    assert b'\x1b]12;#53331c\x07' in transcript
    result['light_cursor_has_explicit_contrast']=True
    send(b'\x15')
    paste('Dark')
    send(b'\r')
    send(b'\x1b')
    # Remap a real key and prove the new chord opens the command center.
    send(b'\x1b[19~')  # F8
    paste('Command center')
    send(b'\r')
    send(b'\x10')  # Ctrl+P
    wait_for(lambda:b'Shortcut saved' in transcript)
    send(b'\x1b')
    at=len(transcript)
    send(b'\x10')
    wait_for(lambda:b'WORKSPACE' in transcript[at:])
    send(b'\x1b')
    result['settings_and_real_key_remapping'] = True
    # Skill insertion preserves the draft and does not call the model.
    send(b'\x1bOR')  # F3
    paste('ui-probe')
    send(b'\r')
    wait_for(lambda:b'$ui-probe' in plain(transcript))
    send(b'\x15')
    paste('/statusline')
    send(b'\r')
    paste('Git branch')
    send(b' ')
    send(b'\x1b')
    saved=test_home/'Library/Application Support/osmogrep/sessions'/(hashlib.sha256(str(repo).encode()).hexdigest()+'.json')
    settings=json.loads(saved.read_text())
    assert settings['theme']=='dark'
    assert settings['preferences']['key_bindings']['commands']=='ctrl+p'
    assert 'branch' not in settings['preferences']['status_fields']
    assert not requests, 'workspace controls and skill insertion must not call a model'
    result['statusline_and_skill_picker'] = True
    paste('hello world')
    send(b'\x1b[D'*5)
    send(b'\x1b[1;2C'*5)
    paste('image ')
    send(b'\x1b[1;5H')
    paste('START ')
    send(b'\x1b[1;5F')
    paste(' END')
    send(b'\x01')
    paste('long line\n'*20_000)
    send(b'\x1b[1;5H')
    paste('LONG_BEGIN ')
    send(b'\x1b[1;5F')
    paste(' LONG_END')
    assert not requests,'pasted newlines must not submit a message'
    assert process.poll() is None
    result['large_multiline_draft_editing'] = True
    send(b'\x01')
    paste('inspect image ')
    before_attachment=len(transcript)
    paste(str(image).replace(' ','\\ '))
    wait_for(lambda:b'\x1b_G' in transcript[before_attachment:])
    assert b'[Image #1]' in plain(transcript)
    result['real_thumbnail_sequence'] = True
    paste(' $ui-probe please')
    send(b'\r')
    wait_for(lambda:b'IMAGE_REQUEST_OK' in transcript)
    assert len(requests)==1, len(requests)
    content=[part for message in requests[0]['input'] if isinstance(message,dict)
             for part in (message.get('content') if isinstance(message.get('content'),list) else [])]
    images=[part for part in content if part.get('type')=='input_image']
    assert len(images)==1
    assert base64.b64decode(images[0]['image_url'].split(',',1)[1])==png
    assert 'inspect image [Image #1]' in json.dumps(requests[0]['input'])
    assert 'UI_SKILL_INSTRUCTION' in json.dumps(requests[0]['input']), 'selected skill must reach the existing model path'
    result['original_image_bytes_reached_provider'] = True
    result['selected_skill_reached_provider'] = True
    paste('/usage')
    send(b'\r')
    wait_for(lambda:b'Reported input' in transcript and b'1234' in transcript)
    send(b'\x1b')
    paste('/stats')
    send(b'\r')
    wait_for(lambda:b'SAMPLE DATA' in transcript and b'WEEKLY USAGE' in transcript)
    filling=len(transcript);drain(.9)
    assert b'\x1b[38;2;122;119;109m' in transcript[filling:], 'weekly bars must keep filling without key input'
    result['weekly_chart_entry_animation']=True
    send(b'd')
    wait_for(lambda:b'RECORDED ACTIVITY' in transcript and b'1,290 tokens' in transcript)
    send(b'\x1b')
    result['reported_usage_and_pixel_history'] = True
    (repo/'permission.txt').write_text('before\n')
    at=len(transcript)
    paste('UI_PERMISSION_PROBE')
    send(b'\r')
    wait_for(lambda:b'Allow once' in transcript[at:] and b'permission.txt' in transcript[at:])
    assert (repo/'permission.txt').read_text()=='before\n', 'permission must precede the edit'
    send(b'y')
    wait_for(lambda:b'PERMISSION_FLOW_OK' in transcript[at:])
    assert (repo/'permission.txt').read_text()=='after\n'
    assert b'48;2;9;30;20m' in transcript[at:], 'applied diff must use the shaded addition surface'
    result['real_permission_dock_and_numbered_diff'] = True
    # Ordinary Enter messages remain separate FIFO turns while a response streams.
    at=len(requests);paste('QUEUE_FIRST');send(b'\r');paste('QUEUE_SECOND');send(b'\r');paste('QUEUE_THIRD');send(b'\r')
    wait_for(lambda:b'QUEUE_THIRD_COMPLETE' in plain(transcript),seconds=15)
    delivered=[next(m['content'] for m in reversed(request['input']) if m.get('role')=='user') for request in requests[at:]]
    assert delivered==['QUEUE_FIRST','QUEUE_SECOND','QUEUE_THIRD'],delivered
    assert all('[steer-now]' not in json.dumps(r['input']) for r in requests[at:])
    result['submitted_followups_are_distinct_fifo_turns']=True
    # A response preface must not prevent subsequent tool calls from executing.
    (repo/'conversation.txt').write_text('CONVERSATION_SOURCE\n')
    at=len(requests);paste('READ_PROBE');send(b'\r');wait_for(lambda:b'READ_PROBE_OK' in plain(transcript))
    assert len(requests[at:])==2
    result['assistant_preface_does_not_skip_requested_tools']=True
    at=len(transcript);paste('SCROLL_PROBE');send(b'\r');wait_for(lambda:b'SCROLL_LINE_199' in plain(transcript[at:]))
    visible=[int(n) for n in re.findall(rb'SCROLL_LINE_(\d+)',plain(transcript[at:]))][-20:]
    moved=len(transcript);send(b'\x1b[<64;40;12M'*3)
    older=[int(n) for n in re.findall(rb'SCROLL_LINE_(\d+)',plain(transcript[moved:]))]
    assert older and min(older)<min(visible),(older,visible)
    copied_at=len(transcript);send(b'\x1b[<0;4;6M');send(b'\x1b[<32;100;30M');send(b'\x1b[<0;100;30m')
    wait_for(lambda:b'\x1b]52;c;' in transcript[copied_at:])
    copies=re.findall(rb'\x1b]52;c;([A-Za-z0-9+/=]+)\x07',transcript[copied_at:])
    copied=base64.b64decode(copies[-1]).decode()
    assert 'SCROLL_LINE_' in copied and '\x1b' not in copied
    result['wheel_scrolling_and_drag_copy_plain_output']=True
    send(b'\x1b');paste('STREAM_SCROLL_PROBE');send(b'\r')
    wait_for(lambda:b'STREAM_LINE_079' in plain(transcript))
    send(b'\x1b[<64;40;12M'*4)
    held=len(transcript);drain(3.5)
    assert b'STREAM_LINE_099' not in plain(transcript[held:]), 'streaming must not pull a scrolled transcript back to the bottom'
    send(b'\x1b[F')
    wait_for(lambda:b'STREAM_LINE_099' in plain(transcript[held:]))
    result['streaming_preserves_user_scroll_position']=True

    send(b'\x1b')
    assert process.poll() is None

    for cols,rows in [(100,30),(60,18),(180,50)]:
        fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',rows,cols,0,0))
        os.killpg(process.pid,signal.SIGWINCH)
        drain(.3)
    assert b'Native UI restored' not in transcript
    result['terminal_resize'] = True
    paste('!printf UI_SHELL_OK > shell-result.txt')
    send(b'\r')
    wait_for(lambda:(repo/'shell-result.txt').exists())
    assert (repo/'shell-result.txt').read_text()=='UI_SHELL_OK'
    result['existing_shell_command'] = True
    # Kill only the renderer descended from this test's process group.
    table=[]
    for line in subprocess.check_output(['ps','-axo','pid=,ppid=,command='],text=True).splitlines():
        parts=line.strip().split(None,2)
        if len(parts)==3: table.append((int(parts[0]),int(parts[1]),parts[2]))
    descendants={process.pid}
    for _ in range(4): descendants.update(pid for pid,parent,cmd in table if parent in descendants)
    nodes=[pid for pid,parent,cmd in table if pid in descendants and 'osmogrep-ui-' in cmd and 'renderer.mjs' in cmd]
    assert len(nodes)==1,nodes
    os.kill(nodes[0],signal.SIGKILL)
    send(b'x')
    wait_for(lambda:b'Native UI restored' in transcript)
    assert process.poll() is None
    result['renderer_crash_recovers_to_native_ui'] = True
    send(b'\x01')
    paste('/exit')
    send(b'\r')
    wait_for(lambda:b'OSMOGREP_UI_EXIT=0' in transcript)
    assert termios.tcgetattr(slave)[3]==before[3]
    assert b'\x1b[?1049l' in transcript and b'\x1b[<1u' in transcript
    assert b'\x1b]112\x07' in transcript and b'\x1b[0 q' in transcript
    result['terminal_state_restored'] = True
    send(b'\n')
    assert process.wait(timeout=3)==0
    # A second real process must load the saved shortcut and appearance.
    # macOS binds a controlling PTY to its session; use a fresh PTY for the restart.
    os.close(master)
    os.close(slave)
    master,slave=pty.openpty()
    fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',44,160,0,0))
    restart_flags=termios.tcgetattr(slave)
    at=len(transcript)
    process = subprocess.Popen(['sh','-c',wrapper,'osmogrep-ui-check',str(binary)],cwd=repo,env=env,
                               stdin=slave,stdout=slave,stderr=slave,preexec_fn=controlling_terminal)
    wait_for(lambda:b'osmogrep' in transcript[at:] and b'local-ui-test-model' in transcript[at:])
    send(b'\x10')
    wait_for(lambda:b'WORKSPACE' in transcript[at:])
    send(b'\x1b')
    paste('/exit')
    send(b'\r')
    wait_for(lambda:b'OSMOGREP_UI_EXIT=0' in transcript[at:])
    assert termios.tcgetattr(slave)[3]==restart_flags[3]
    send(b'\n')
    assert process.wait(timeout=3)==0
    result['preferences_survive_real_restart'] = True
finally:
    (args.output/'terminal.ansi').write_bytes(transcript)
    (args.output/'result.json').write_text(json.dumps(result,indent=2)+'\n')
    if process.poll() is None:
        os.killpg(process.pid,signal.SIGKILL)
        process.wait(timeout=3)
    os.close(master)
    os.close(slave)
    server.shutdown()
    server.server_close()
    thread.join(timeout=3)
print(json.dumps(result,indent=2))
