import {test} from 'node:test';
import assert from 'node:assert/strict';
import {inflateSync} from 'node:zlib';
import {setCapabilities,visibleWidth,stripTerminalSequences} from '@earendil-works/pi-tui';
import {orb,activityInfo,completionLine} from './activity.ts';
import {drawCenter} from './center.ts';
import {render} from './renderer.ts';
import {fixture} from './test-fixture.ts';

test('working orb is small, dim, deterministic and still under reduced motion',()=>{
 const a=orb(0),b=orb(1000);
 assert.equal(a.png.readUInt32BE(16),36);assert.equal(a.png.readUInt32BE(20),36);
 assert.deepEqual(a.rows.map(visibleWidth),[6,6,6]);assert.notDeepEqual(a.png,b.png);
 assert.deepEqual(orb(0,false,true),orb(30000,false,true));assert.deepEqual(a,orb(4000));
 const chunks:Buffer[]=[];for(let i=8;i<a.png.length;){const size=a.png.readUInt32BE(i);if(a.png.toString('ascii',i+4,i+8)==='IDAT')chunks.push(a.png.subarray(i+8,i+8+size));i+=size+12;}
 assert.ok(Math.max(...inflateSync(Buffer.concat(chunks)))<220);
 assert.equal(activityInfo(9000,'thinking',null).label,'Working through your request');
 assert.equal(activityInfo(9000,'thinking',null,true).label,'Thinking');
 assert.equal(activityInfo(9000,'tool','run_shell').label,'Running a command');
});

test('runtime status retains breathing room, completion alignment, and voice stays inside the composer',()=>{
 setCapabilities({images:null,trueColor:true,hyperlinks:false});
 const v={...fixture(),running:true,phase:'thinking',runElapsed:9000,voice:'voice connected',voiceState:{enabled:true,connected:true,partial:false},team:{enabled:true,unread:2,online:1}};
 const f=render(v),text=f.rows.map(stripTerminalSequences).join('\n');
 const sphere=orb(9000);sphere.rows.forEach((row,i)=>assert.ok(f.rows[f.input.y-4+i]!.includes(row)));
 assert.match(text,/Working through your request/);assert.match(text,/Listening/);assert.doesNotMatch(text,/Team 1|2 new/);
 assert.ok(f.execution.y+f.execution.height<=f.input.y-4);
 assert.equal(f.rows.map(stripTerminalSequences)[f.input.y-1]!.trim(),'');
 assert.equal(f.controls?.find(c=>c.id==='voice')?.y,f.input.y);
 assert.ok(!f.controls?.some(c=>c.id==='team'));
 assert.equal(completionLine('· Finished in 3.3s',70)?.length,70);
 assert.match(completionLine('· Finished in 3.3s',70)!,/^Done +3\.3s elapsed$/);
 assert.equal(completionLine('ordinary text',70),null);
 const masked=render({...v,masked:true});assert.ok(!masked.controls?.some(c=>c.id==='voice'||c.id==='team'));
});

test('session center shows compact project names and stable session IDs within every viewport',()=>{
 const rows=Array.from({length:30},(_,i)=>({id:`session-${i}`,label:'Repeated task name',description:`model · workspace · session-${i}`,value:'working · now',tag:i%3?'working':'inactive',group:i<15?'/projects/alpha':'/projects/beta'}));
 const panel={sections:['Sessions'],section:10,query:'',selected:22,editing:false,rows,notice:'Live owners keep their edits',footer:'',counts:[30,0,20,0,10],center_filter:0};
 for(const [width,height] of [[1,1],[20,8],[60,18],[100,32],[180,48]]){
  const f=drawCenter(panel,width!,height!,false,'\x1b[38;2;235;155;105m');
  assert.equal(f.rows.length,height);assert.ok(f.rows.every(row=>visibleWidth(row)<=width!));
  for(const control of f.controls){assert.equal(control.label,rows[control.index]!.id);assert.ok(control.x>=0&&control.x+control.width<=width!&&control.y>=0&&control.y<height!);}
  if(height!>=18)assert.ok(f.controls.some(c=>c.index===22));
 }
 const attack=drawCenter({...panel,rows:[{...rows[0]!,group:'\x1b[2J/project\x07'}]},100,32,false,'');
 assert.ok(!attack.rows.join('').includes('\x1b[2J'));assert.ok(!attack.rows.join('').includes('\x07'));
});

test('selected session dot fills once, settles and respects reduced motion',()=>{
 const panel={sections:['Sessions'],section:10,query:'',selected:0,editing:false,rows:[{id:'s1',label:'Recent task',description:'model',value:'ready · now',tag:'ready',group:'/projects/alpha'}],notice:'',footer:''};
 const frames=[0,55,110,165,240].map(elapsed=>drawCenter({...panel,elapsed},100,30,false,'').rows.map(stripTerminalSequences).join('\n'));
 assert.ok(frames[0]!.includes('○ Recent task'));assert.ok(frames[2]!.includes('◑ Recent task'));assert.ok(frames[4]!.includes('● Recent task'));
 assert.equal(drawCenter({...panel,elapsed:0},100,30,false,'',true).rows.map(stripTerminalSequences).join('\n'),frames[4]);
 assert.ok(!frames[4]!.includes('/projects/alpha'));assert.match(frames[4]!,/alpha/);
 assert.equal(render({...fixture(),panel:{...panel,elapsed:0}}).animate,true);
 assert.equal(render({...fixture(),panel:{...panel,elapsed:240}}).animate,false);
});


test('compact approvals retain every decision while preserving transcript and a long draft',()=>{
 const draft='long input '.repeat(100);
 for(const height of [20,24,30]){
  const v={...fixture(),width:80,height,input:draft,cursor:draft.length,running:true,phase:'tool',logs:[{level:'Info',text:'RECENT_OUTPUT'}],approval:{tool:'patch',args:'notes.txt',selected:2}};
  const f=render(v),text=f.rows.map(stripTerminalSequences).join('\n');
  assert.match(text,/RECENT_OUTPUT/);assert.match(text,/Allow once/);assert.match(text,/Deny/);assert.match(text,/Auto-approve this session/);
  assert.ok(f.execution.height>=3);assert.ok(f.rows.every(r=>visibleWidth(r)<=80));
 }
});

test('live session preview scrolls saved output without exposing action controls',()=>{
 const panel={sections:['Sessions'],section:10,query:'',selected:0,editing:false,rows:[],notice:'Read only',footer:'',previewing:true,preview_offset:0,preview:Array.from({length:80},(_,i)=>'SAVED_'+i)};
 const tail=drawCenter(panel,100,30,false,''),up=drawCenter({...panel,preview_offset:4},100,30,false,'');
 assert.equal(tail.controls.length,0);assert.match(tail.rows.join('\n'),/SAVED_79/);assert.ok(!up.rows.join('\n').includes('SAVED_79'));
});

test('inactive voice is absent even after the status was previously shown',()=>{
 for(const voice of [null,'VOICE (off)','voice disconnected']){
  const f=render({...fixture(),voice,voiceState:{enabled:false,connected:false,partial:false}});
  assert.ok(!f.controls?.some(c=>c.id==='voice'));
  assert.ok(!f.rows.map(stripTerminalSequences).join('\n').includes('VOICE (off)'));
  assert.equal(stripTerminalSequences(f.rows[f.input.y]!).trim(),'');
 }
});


test('release screen gates chat while preserving the draft in application state',()=>{
 for(const height of [20,24,32,36,40])for(const light of [false,true]){
  const f=render({...fixture(),height,theme:light?'light':'dark',input:'keep draft',cursor:10,logs:[{level:'Info',text:'RECENT_OUTPUT'}],update:'Update available',release:{current:'0.3.4',latest:'0.4.0',notes:['Better sessions','Improved input'],selected:1,installing:false}});
  const text=f.rows.map(stripTerminalSequences).join('\n');
  for(const label of ['osmogrep','Update now','Later','Skip this version'])assert.ok(text.includes(label),`${height}: ${label}`);
  assert.ok(!text.includes('RECENT_OUTPUT')&&!text.includes('keep draft'));
  assert.equal(f.execution.height,0);assert.equal(f.input.height,0);assert.ok(f.rows.every(row=>visibleWidth(row)<=f.input.width+2));
 }
});


test('established conversations show session and shortcut hints below the selectors',()=>{
 const v={...fixture(),elapsed:180000,logs:[{level:'Info',text:'USER|Explain this function'}],statusEntries:[{id:'model',value:'test-model · local'}]};
 const f=render(v),text=f.rows.map(stripTerminalSequences);
 assert.match(text.at(-1)!,/⌘← \/ ctrl\+s sessions · F8 shortcuts/);
 assert.equal(f.controls?.find(c=>c.id==='model')?.y,v.height-2);
 assert.ok(!render({...v,running:true}).rows.map(stripTerminalSequences).at(-1)!.includes('F8 shortcuts'));
});

test('elapsed time carries seconds into minutes and hours',()=>{
 for(const [seconds,expected] of [[59.9,'59.9s'],[59.999,'1m 0s'],[92.1,'1m 32s'],[3599.999,'1h 0m 0s'],[3723.4,'1h 2m 3s']] as const){
  assert.equal(completionLine(`· Finished in ${seconds}s`,8),`Done · ${expected}`);
 }
 assert.equal(activityInfo(92100,'thinking',null).time,'1m 32s');
});


test('session rename uses a save action and a bounded cursor instead of message wording',()=>{
 const panel={sections:['Sessions'],section:10,query:'New conversation name',selected:0,editing:true,edit_label:'Name',rows:[],notice:'',footer:''};
 const frame=drawCenter(panel,100,30,false,'');
 const text=frame.rows.map(stripTerminalSequences).join('\n');
 assert.match(text,/Name  New conversation name/);
 assert.match(text,/enter save/);assert.doesNotMatch(text,/enter send|Message  /);
 assert.ok(frame.cursor.x>=0&&frame.cursor.x<100);
});
