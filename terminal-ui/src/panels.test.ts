import {test} from 'node:test';
import assert from 'node:assert/strict';
import {visibleWidth,stripTerminalSequences} from '@earendil-works/pi-tui';
import {drawPanel,type PanelView} from './panels.ts';
const panel:PanelView={sections:['Commands','Appearance','Model'],section:0,query:'',selected:17,editing:false,rows:Array.from({length:25},(_,i)=>({label:`command ${i}`,description:'A useful workspace control',value:'↵'})),notice:'Draft preserved',footer:'enter select · esc close'};
test('panel navigation stays visible without overflowing tiny or wide terminals',()=>{
  for(const [w,h]of [[1,1],[20,8],[80,24],[160,44]])for(const light of [false,true]){
    const f=drawPanel(panel,w!,h!,light,'\x1b[38;2;234;88;12m');
    assert.equal(f.rows.length,h);assert.ok(f.rows.every(r=>visibleWidth(r)<=w!));assert.ok(f.cursor.x<w!&&f.cursor.y<h!);
    if(h!>=24)assert.match(f.rows.join('\n'),/command 17/);
  }
});
test('panel text is plain and control injection cannot leave the presentation boundary',()=>{
  const f=drawPanel({...panel,query:'hello\x1b[2J\x07',notice:'x\x1b]52;c;secret\x07'},80,24,false,'');
  assert.ok(!f.rows.join('').includes('\x1b[2J'));assert.ok(!f.rows.join('').includes(']52'));
  assert.match(f.rows.map(stripTerminalSequences).join('\n'),/hello/);
});
test('pixel activity uses recorded provider totals and keeps the empty state honest',()=>{
  const date=new Date().toISOString().slice(0,10);
  const view={...panel,section:8,sections:['','','','','','','','','Stats'],history:[{day:date,model:'test',provider:'local',input:100,output:20,cached:50,calls:1,duration_ms:7}]};
  const text=drawPanel(view,100,40,false,'').rows.map(stripTerminalSequences).join('\n');
  assert.match(text,/120 tokens/);assert.match(text,/1 active days/);assert.match(text,/[░▒▓]/);
  assert.match(drawPanel({...view,history:[]},100,40,false,'').rows.map(stripTerminalSequences).join('\n'),/No recorded activity yet/);
});
