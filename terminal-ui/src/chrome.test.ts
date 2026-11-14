import {test} from 'node:test';import assert from 'node:assert/strict';
import {visibleWidth,stripTerminalSequences} from '@earendil-works/pi-tui';
import {statusText,activity} from './chrome.ts';import {render} from './renderer.ts';import {fixture} from './test-fixture.ts';
import {diffLines} from './transcript.ts';import {sampleHistory,statsLines} from './stats.ts';
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
 const history:any[]=[],now=Date.UTC(2026,10,14);const a=sampleHistory(now),b=sampleHistory(now);assert.deepEqual(a,b);assert.equal(a.length,365);
 const text=statsLines(history,100,24,false,true,now).map(stripTerminalSequences).join('\n');assert.match(text,/SAMPLE DATA/);assert.match(text,/[░▒▓]/);assert.equal(history.length,0);
 assert.ok(!statsLines(history,100,24,false,false,now).join('').includes('SAMPLE DATA'));
});
