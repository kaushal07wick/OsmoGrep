import {test} from 'node:test';import assert from 'node:assert/strict';
import {stripTerminalSequences} from '@earendil-works/pi-tui';
import {selectedTranscript,highlightTranscript,diffLines} from './transcript.ts';import {render} from './renderer.ts';import {fixture} from './test-fixture.ts';
test('minified diffs fold by visual rows and expanded content retains a reachable collapse action',()=>{
 const source='.selector{color:var(--accent);padding:12px}'.repeat(1500)+'END_OF_CSS';
 const diff={file:'style.css',lines:[{kind:'Added',text:source,newLine:1}]};
 const collapsed=diffLines(diff,76,false,false);
 assert.ok(collapsed.length<=18);
 assert.match(collapsed.map(stripTerminalSequences).join('\n'),/Show more/);
 const full=diffLines(diff,76,false,true).map(stripTerminalSequences).join('\n').replace(/\s/g,'');
 assert.ok(full.includes('END_OF_CSS')); assert.ok(full.includes('Showless'));
 const view={...fixture(),width:80,height:32,userTurns:2,diffs:[diff],logs:[],followTail:false,scrollTop:0,expandTools:true};
 const frame=render(view),action=frame.controls?.find(control=>control.id==='expand');
 assert.ok(action); assert.equal(action.label,'Show less');
 assert.ok(action.y<frame.input.y);
});
test('reading history exposes a quiet Latest action and new output does not move the viewport',()=>{
 const view={...fixture(),width:80,height:32,userTurns:2,followTail:false,scrollTop:5,input:'my draft',logs:Array.from({length:80},(_,i)=>({text:`Entry ${i}`,level:'Success'}))};
 const before=render(view),after=render({...view,newActivity:true,logs:[...view.logs,{text:'new output',level:'Success'}]});
 assert.equal(after.transcript!.start,before.transcript!.start);
 const action=after.controls?.find(control=>control.id==='latest');
 assert.ok(action); assert.equal(action.y,after.input.y-1);
 assert.match(stripTerminalSequences(after.rows[action.y]!),/● ↓ Latest/);
 assert.ok(!render({...view,followTail:true}).controls?.some(control=>control.id==='latest'));
});
test('transcript selections copy plain Unicode text in either drag direction',()=>{
 const lines=['\x1b[38;2;1;2;3mA界B\x1b[0m','second line'];
 assert.equal(selectedTranscript(lines,[[0,1],[1,6]]),'界B\nsecond');assert.equal(selectedTranscript(lines,[[1,6],[0,1]]),'界B\nsecond');
 const marked=highlightTranscript(lines[0]!,0,[[0,1],[0,3]],true);assert.equal(stripTerminalSequences(marked),'A界B');assert.match(marked,/48;2;90;65;42m/);assert.ok(!selectedTranscript(lines,[[0,1],[1,6]]).includes('\x1b'));
});
test('scrolling stays anchored when new output arrives and exposes selectable viewport bounds',()=>{
 const logs=Array.from({length:80},(_,i)=>({text:'Entry '+i+'\n',level:'Success'})),v={...fixture(),logs,followTail:false,scroll:12},initial=render(v);
 assert.ok(initial.transcript);const top=initial.transcript.start;
 const next=render({...v,logs:[...logs,{text:'New output',level:'Success'}],scrollTop:top});assert.equal(next.transcript!.start,top);
 assert.ok(next.transcript!.lineWidths.every(w=>w<=next.transcript!.width));assert.ok(next.transcript!.maxScroll>0);
 const selected=render({...v,transcriptSelection:[[top,0],[top+2,30]]});assert.match(selected.selectionText!,/Entry/);assert.ok(!selected.selectionText!.includes('\x1b'));
});
test('permission surface covers trailing padding even after styled row resets',()=>{
 const v={...fixture(),permission:'Allow?',approval:{tool:'run_shell',args:'swift --version',selected:0}},f=render(v),row=f.rows.find(r=>r.includes('N  Deny'))!;
 let background='',column=0;const colors:string[]=[];
 for(const token of row.split(/(\x1b\[[0-9;]*m)/g)){
  if(token.startsWith('\x1b[')){const n=token.slice(2,-1).split(';').map(Number);for(let i=0;i<n.length;i++){if(n[i]===0||n[i]===49)background='default';if(n[i]===48&&n[i+1]===2){background=n.slice(i+2,i+5).join(';');i+=4;}}}
  else for(const c of token){colors[column++]=background;}
 }
 assert.ok(colors.slice(f.input.x,f.input.x+f.input.width).every(c=>c==='30;30;30'));
});
test('an edit diff stays next to its tool result with the final summary after it',()=>{
 const v=fixture();v.height=60;
 v.logs=[{level:'Success',text:'● (Patch) README.md'},{level:'Info',text:'  └ edited README.md'},{level:'Info',text:'Updated README.md with the requested sentence.'}];
 v.diffs=[{file:'README.md',afterLog:1,lines:[{text:'new sentence',kind:'Added',newLine:9}]}];
 const frame=render(v),text=frame.rows.map(stripTerminalSequences).join('\n');
 assert.ok(text.indexOf('new sentence')<text.indexOf('Updated README.md'));
 assert.ok(text.indexOf('edited README.md')<text.indexOf('new sentence'));
});
test('overflowing output stays scrollable without a visible scrollbar without footer scrolling hints',()=>{
 const v=fixture();v.logs=Array.from({length:50},(_,i)=>({level:'Info',text:`Line ${i}\n`}));
 const bottom=render(v);assert.equal(bottom.transcript!.scrollbar,null);
 const top=render({...v,followTail:false,scrollTop:0});const track=top.transcript!.scrollbar;
 assert.equal(top.transcript!.start,0);assert.equal(track,null);

 assert.doesNotMatch(top.rows.map(stripTerminalSequences).join('\n'),/end live|pgup\/pgdn/);
 assert.ok(top.rows.every(row=>stripTerminalSequences(row).length<=v.width));
});

test('completion status has one clear row above the composer while quick actions stay attached',()=>{
 const f=render({...fixture(),logs:[...Array.from({length:60},(_,i)=>({level:'Success',text:'Line '+i})),{level:'Info',text:'· Finished in 2.1s'}]});
 assert.equal(stripTerminalSequences(f.rows[f.input.y-1]!).trim(),'');
 assert.match(stripTerminalSequences(f.rows[f.input.y-2]!),/Done.*2\.1s elapsed/);
});

test('streamed replies stay continuous until completion, including lines that resemble tool status',()=>{
 const logs=Array.from({length:70},(_,i)=>({level:'Info',text:i===40?'· still part of the answer':`answer line ${i}`}));
 for(const stream of ['', 'last sentence'])for(const expandTools of [false,true]){
  const f=render({...fixture(),userTurns:2,height:44,running:true,streaming:true,streamingStart:0,logs,stream,expandTools});
  assert.ok(!f.controls?.some(c=>c.id==='expand'));
  assert.doesNotMatch(f.rows.map(stripTerminalSequences).join('\n'),/Show more|Show less/);
  assert.match(f.rows.map(stripTerminalSequences).join('\n'),/answer line 69/);
 }
 const finished=render({...fixture(),userTurns:2,height:60,logs:[...logs.map(log=>({...log,text:log.text.replace('· still part of the answer','still part of the answer')})),{level:'Info',text:'last sentence'},{level:'Info',text:'· Finished in 92.1s'}]});
 const text=finished.rows.map(stripTerminalSequences).join('\n');
 assert.match(text,/Show more/); assert.match(text,/1m 32s elapsed/);
 assert.doesNotMatch(text,/last sentence/);
});

test('live file diffs offer no folding controls until the edit completes',()=>{
 const diff={file:'live.py',streaming:true,lines:Array.from({length:60},(_,i)=>({text:`value_${i} = ${i}`,kind:'Added',newLine:i+1}))};
 const live=diffLines(diff,80,false,false).map(stripTerminalSequences).join('\n');
 assert.match(live,/value_59/); assert.doesNotMatch(live,/Show more|Show less/);
 assert.match(diffLines({...diff,streaming:false},80,false,false).map(stripTerminalSequences).join('\n'),/Show more/);
});

test('copy success is blue, brief, above the input and never includes clipboard contents',()=>{
 for(const theme of ['dark','light']){
  const base={...fixture(),theme,userTurns:2,preferences:{status_fields:[],reduce_motion:true,ambient_art:false},copyNotice:{characters:2582,age:200}};
  const f=render(base),row=f.rows[f.input.y-1]!;
  assert.match(stripTerminalSequences(row),/Copied 2,582 characters to clipboard/);
  assert.ok(row.includes(theme==='dark'?'95;175;255':'20;105;210')); assert.equal(f.animate,true);
  const expired=render({...base,copyNotice:{characters:2582,age:3000}});
  assert.doesNotMatch(expired.rows.map(stripTerminalSequences).join('\n'),/Copied/);
  assert.equal(expired.animate,false);
 }
});


test('dark chat prose and neutral code text use pure white',()=>{
 const f=render({...fixture(),userTurns:2,logs:[{text:'WHITE_PROSE\n\n```python\nvalue = symbol\n```',level:'Info'}]});
 const prose=f.rows.find(row=>row.includes('WHITE_PROSE'))!,code=f.rows.find(row=>row.includes('symbol'))!;
 assert.ok(prose.includes('38;2;255;255;255m'));assert.ok(code.includes('38;2;255;255;255m'));
});
