import {test} from 'node:test';
import assert from 'node:assert/strict';
import {stripTerminalSequences,visibleWidth} from '@earendil-works/pi-tui';
import {usageLines} from './usage.ts';
import {drawPanel} from './panels.ts';
test('usage summarizes input/output without double-counting cached input or inventing a context limit',()=>{
 const history=[{day:'2026-11-15',model:'test',provider:'local',input:27072,output:931,cached:7296,calls:10,duration_ms:34900}];
 const fields=[{label:'Context estimate',value:'~7529 tokens'}];
 for(const light of [false,true]){
  const rows=usageLines(history,fields,100,20,light),text=rows.map(stripTerminalSequences).join('\n');
  for(const label of ['28,003','27,072','931','7,296','27%','10','34.9s','~7529 tokens'])assert.ok(text.includes(label),label);
  assert.match(text,/capacity not supplied/);assert.ok(rows.some(row=>row.includes('110;155;170')||row.includes('65;105;120')));
 }
 const empty=usageLines([],fields,30,10,false);assert.ok(empty.every(row=>visibleWidth(row)<=30));assert.ok(!empty.join('').includes('NaN'));
 const panel={sections:['','','','','','','','Usage'],section:7,query:'',selected:0,editing:false,rows:fields.map(row=>({...row,description:''})),notice:'',footer:'esc categories',history,settings:true};
 const frame=drawPanel(panel,100,24,false,''),text=frame.rows.map(stripTerminalSequences).join('\n');
 assert.match(text,/28,003/);assert.match(text,/MODEL CALLS/);assert.match(text,/CURRENT CONTEXT/);assert.ok(!text.includes('Search'));
 const failed=drawPanel({...panel,history:[],notice:'Usage history unavailable: unreadable store'},100,24,false,'').rows.map(stripTerminalSequences).join('\n');assert.match(failed,/unreadable store/);assert.ok(!failed.includes('0 total'));
});
