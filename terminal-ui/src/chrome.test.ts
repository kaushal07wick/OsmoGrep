import {test} from 'node:test';import assert from 'node:assert/strict';
import {visibleWidth,stripTerminalSequences} from '@earendil-works/pi-tui';
import {statusText,activity} from './chrome.ts';import {render} from './renderer.ts';import {fixture} from './test-fixture.ts';
import {diffLines} from './transcript.ts';import {sampleHistory,statsLines,weeklyUsage} from './stats.ts';
test('status fields shrink paths before optional fields and remain within narrow panes',()=>{
 const fields=[{id:'model',value:'gpt-5.2 · openai'},{id:'directory',value:'~/Desktop/workspace/personal/Antimatter'},{id:'branch',value:'main'},{id:'elapsed',value:'1m'}];
 for(const w of [12,28,48,80,160]){const text=statusText(fields,w);assert.ok(visibleWidth(text)<=w);assert.ok(!text.endsWith(' · '));if(w===48){assert.match(text,/Antimatter/);assert.ok(!text.includes('Desktop'));}}
});
test('permission dock preserves recent output, input and the selected decision',()=>{
 const v=fixture();v.logs=[{level:'Info',text:'I am examining README.md before changing it.'}];v.running=true;v.runElapsed=12300;v.permission='Allow edit_file?';v.approval={tool:'edit_file',args:'README.md',selected:1};
 const f=render(v),text=f.rows.map(stripTerminalSequences).join('\n');assert.match(text,/examining README/);assert.match(text,/N  Deny/);assert.match(text,/12s/);assert.ok(!text.includes('Search'));assert.ok(f.rows.every(r=>visibleWidth(r)===v.width));assert.ok(f.execution.height>3);
 assert.notEqual(activity(100,'thinking',null),activity(500,'thinking',null));assert.match(activity(75000,'thinking',null),/1m 15s/);
});
test('long tool output folds without dropping access to details and diffs retain line numbers',()=>{
 const v=fixture();v.logs=[{level:'Success',text:'● (Read) README.md'},...Array.from({length:20},(_,i)=>({level:'Info',text:`  └ line ${i}`}))];
 let text=render(v).rows.map(stripTerminalSequences).join('\n');assert.match(text,/17 more lines/);assert.ok(!text.includes('line 19'));
 v.expandTools=true;text=render(v).rows.map(stripTerminalSequences).join('\n');assert.match(text,/line 19/);
 const rows=diffLines({file:'readme.md',preview:true,lines:[{kind:'Removed',text:'old',oldLine:7},{kind:'Added',text:'new',newLine:7}]},30,false,false);
 assert.match(rows.map(stripTerminalSequences).join('\n'),/7 - old/);assert.match(rows.map(stripTerminalSequences).join('\n'),/7 \+ new/);assert.ok(rows.some(r=>r.includes('48;2;9;30;20')));assert.ok(rows.every(r=>visibleWidth(r)<=30));
});
test('sample charts are labeled, deterministic and never mutate real usage',()=>{
 const history:any[]=[],now=Date.UTC(2026,10,14);const a=sampleHistory(now),b=sampleHistory(now);assert.deepEqual(a,b);assert.equal(a.length,365);assert.ok(weeklyUsage(a,12,now).every(w=>w.input+w.output>0));
 const text=statsLines(history,100,24,false,true,now).map(stripTerminalSequences).join('\n');assert.match(text,/SAMPLE DATA/);assert.match(text,/[\u2801-\u28ff]/);assert.equal(history.length,0);
 assert.ok(!statsLines(history,100,24,false,false,now).join('').includes('SAMPLE DATA'));
});

test('weekly aggregates start on Monday in UTC, cross years and combine model calls without double-counting cache',()=>{
 const make=(day:string,input:number,output:number,cached=0)=>({day,model:'test',provider:'local',input,output,cached,calls:1,duration_ms:0});
 const history=[make('2026-12-27',100,10),make('2026-12-28',200,20,100),make('2027-01-03',300,30),make('2027-01-04',400,40),make('2027-01-04',500,50)];
 const weeks=weeklyUsage(history,3,Date.UTC(2027,0,4,1));assert.deepEqual(weeks.map(w=>[new Date(w.start).toISOString().slice(0,10),w.input+w.output]),[['2026-12-21',110],['2026-12-28',550],['2027-01-04',990]]);
 const text=statsLines(history,100,24,false,false,Date.UTC(2027,0,4,1)).map(stripTerminalSequences).join('\n');assert.match(text,/THIS WEEK.*LAST WEEK.*AVG/);assert.match(text,/990/);assert.match(text,/28 Dec–03 Jan/);assert.match(text,/04 Jan–10 Jan/);
});

test('weekly bars fill on entry without changing totals, then settle and respect reduced motion',()=>{
 const now=Date.UTC(2026,10,14),history=sampleHistory(now),frames=[0,.5,1].map(p=>statsLines(history,100,24,false,true,now,p).map(stripTerminalSequences));
 const dots=(rows:string[])=>rows.join('').match(/[\u2801-\u28ff]/g)?.length??0;
 assert.equal(dots(frames[0]!),0);assert.ok(dots(frames[1]!)>0);assert.ok(dots(frames[2]!)>dots(frames[1]!));assert.deepEqual(frames[0]!.slice(0,7),frames[2]!.slice(0,7));
 const panel={sections:['','','','','','','','','Stats'],section:8,query:'',selected:0,editing:false,rows:[],notice:'',footer:'D sample / recorded',demo:true,elapsed:100};
 const v={...fixture(),panel},animating=render(v);assert.equal(animating.animate,true);
 assert.equal(render({...v,panel:{...panel,elapsed:900}}).animate,false);
 assert.equal(render({...v,preferences:{status_fields:[],reduce_motion:true,ambient_art:true}}).animate,false);
});
test('model and provider have separate bounded footer targets and no targets cover credentials or permissions',()=>{
 for(const width of [24,60,80,160]){
  const v={...fixture(),width,model:'model-界',provider:'local',statusEntries:[{id:'model',value:'ignored'},{id:'directory',value:'~/long/workspace'}]},f=render(v);
  assert.ok(f.controls?.some(c=>c.id==='model'));assert.ok(f.controls?.some(c=>c.id==='provider'));
  for(const c of f.controls!){assert.ok(c.x>=0&&c.x+c.width<=width);assert.equal(c.y,v.height-1);}
  assert.equal(render({...v,masked:true}).controls?.length,0);assert.equal(render({...v,permission:'Approve?'}).controls?.length,0);
 }
});
