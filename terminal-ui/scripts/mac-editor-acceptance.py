"""Real isolated tmux/Neovim lifecycle test. Never touches the user's tmux server/config."""
import argparse,fcntl,json,os,pty,select,signal,struct,subprocess,tempfile,termios,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--output',type=Path,required=True);args=p.parse_args();args.output.mkdir(parents=True,exist_ok=True)
repo=Path(tempfile.mkdtemp(prefix='osmogrep-editor-test-')).resolve();(repo/'README.md').write_text('Controlled editor fixture.\n')
config=repo/'config/nvim';config.mkdir(parents=True);(config/'init.lua').write_text('vim.opt.number=true\nvim.opt.swapfile=false\n')
env=os.environ.copy();env.pop('TMUX',None);env.pop('TMUX_PANE',None);env.update(OSMOGREP_UI='typescript',OSMOGREP_DISABLE_UPDATE_CHECK='1',VLLM_REALTIME_AUTOCONNECT='0',TERM='xterm-256color',OSMOGREP_MODEL='editor-test',XDG_CONFIG_HOME=str(repo/'config'),XDG_DATA_HOME=str(repo/'data'),XDG_STATE_HOME=str(repo/'state'))
folder=Path('/private/tmp')/f'tmux-{os.getuid()}';before=set(folder.glob('osmogrep-*'));socket=None
master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',32,100,0,0));flags=termios.tcgetattr(slave)
def controlling():os.setsid();fcntl.ioctl(0,termios.TIOCSCTTY,0)
gate=repo/'release-test-shell'
wrapper='"$1"; code=$?; printf "\\nEDITOR_EXIT=%s\\n" "$code"; editor_wait_attempt=0; while [ ! -e "$2" ] && [ "$editor_wait_attempt" -lt 200 ]; do editor_wait_attempt=$((editor_wait_attempt+1)); sleep .05; done; exit "$code"'
process=subprocess.Popen(['sh','-c',wrapper,'osmogrep-editor-check',str(args.binary.resolve()),str(gate)],cwd=repo,env=env,stdin=slave,stdout=slave,stderr=slave,preexec_fn=controlling);transcript=bytearray();result={}
def drain(seconds=.2):
 end=time.monotonic()+seconds
 while time.monotonic()<end:
  if select.select([master],[],[],.02)[0]:
   try:data=os.read(master,65536)
   except OSError:return
   transcript.extend(data)
   if b'\x1b[6n' in data:os.write(master,b'\x1b[1;1R')
def wait(check,seconds=15):
 end=time.monotonic()+seconds
 while time.monotonic()<end:
  drain(.1)
  if check():return
 raise AssertionError(bytes(transcript[-1000:]))
def tmux(*words):return subprocess.check_output(['tmux','-L',socket,*words],text=True,stderr=subprocess.PIPE).strip()
def panes():
 try:return [line.split('|') for line in tmux('list-panes','-a','-F','#{pane_id}|#{@osmogrep-editor}|#{pane_width}|#{window_id}|#{pane_current_command}').splitlines()]
 except subprocess.CalledProcessError:return []
def command(pane,text):tmux('send-keys','-t',pane,'-l',text);tmux('send-keys','-t',pane,'Enter');drain(.3)
try:
 wait(lambda:b'osmogrep' in transcript)
 os.write(master,b'\x1b[200~/nv README.md\x1b[201~\r')
 wait(lambda:bool(set(folder.glob('osmogrep-*'))-before))
 created=set(folder.glob('osmogrep-*'))-before;assert len(created)==1;socket=next(iter(created)).name
 wait(lambda:any(row[1] and row[4]=='nvim' for row in panes()))
 editor=next(row for row in panes() if row[1]);owner=editor[1]
 assert len(panes())==2 and len({p[3] for p in panes()})==2,panes()
 assert all(int(p[2])==100 for p in panes()),panes();assert tmux('show-options','-v','-t','osmogrep','status')=='off';result['narrow_full_width_windows_no_status_bar']=True
 tmux('send-keys','-t',editor[0],'i','unsaved acceptance change','Escape');drain(.3)
 command(owner,'/nv toggle')
 wait(lambda:'Save changes' in tmux('capture-pane','-p','-t',editor[0]))
 assert any(p[0]==editor[0] for p in panes());result['toggle_preserves_unsaved_changes']=True
 tmux('send-keys','-t',editor[0],'c');command(editor[0],':wq')
 wait(lambda:len(panes())==1)
 assert panes()[0][0]==owner;result['closing_editor_restores_full_agent']=True
 fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',44,180,0,0));os.killpg(process.pid,signal.SIGWINCH)
 wait(lambda:panes() and int(panes()[0][2])==180)
 command(owner,'/nv README.md');wait(lambda:len(panes())==2)
 assert len({p[3] for p in panes()})==1;assert all(int(p[2])>=64 for p in panes());result['wide_split_keeps_agent_readable']=True
 editor=next(p for p in panes() if p[1]);command(editor[0],':q');wait(lambda:len(panes())==1)
 assert int(panes()[0][2])==180;assert b'Native UI restored' not in transcript;result['resize_and_renderer_survive_editor_close']=True
 command(owner,'/exit');wait(lambda:b'EDITOR_EXIT=0' in transcript)
 assert termios.tcgetattr(slave)[3]==flags[3];gate.touch();process.wait(timeout=5);assert process.returncode==0;result['clean_exit_restores_terminal']=True
finally:
 gate.touch()
 if socket:subprocess.run(['tmux','-L',socket,'kill-server'],capture_output=True)
 if process.poll() is None:
  try:process.kill();process.wait(timeout=3)
  except (ProcessLookupError,PermissionError,subprocess.TimeoutExpired):pass
 os.close(master);os.close(slave);(args.output/'terminal.ansi').write_bytes(transcript);(args.output/'result.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
