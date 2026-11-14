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
  assert.match(text,/120 tokens/);assert.match(text,/1 active day/);assert.match(text,/[\u2801-\u28ff]/);
  assert.match(drawPanel({...view,history:[]},100,40,false,'').rows.map(stripTerminalSequences).join('\n'),/0 tokens.*0 active days/);
});

test('workspace cards have outer margins and at least two blank surface rows below hints',()=>{
 for(const [width,height] of [[80,24],[160,44]]){
  const rows=drawPanel(panel,width!,height!,false,'').rows,surface='\x1b[48;2;18;18;18m',card=rows.filter(r=>r.includes(surface));
  assert.ok(rows.join('').includes('◌')&&!rows.join('').includes('◆'));
  assert.ok(!rows[0]!.includes(surface)&&!rows[1]!.includes(surface));assert.ok(!rows.at(-1)!.includes(surface)&&!rows.at(-2)!.includes(surface));
  assert.ok(card.every(r=>stripTerminalSequences(r).startsWith('      ')&&stripTerminalSequences(r).endsWith('      ')));
  const hint=card.findIndex(r=>r.includes('esc close'));assert.ok(hint>=0);assert.ok(card.slice(hint+1,hint+3).every(r=>stripTerminalSequences(r).trim()===''));assert.ok(card.length>=hint+3);
 }
});

test('selector clicks target the same filtered row that is displayed',()=>{
 const view={...panel,selecting:true,title:'Choose model',selected:17},frame=drawPanel(view,100,40,false,'');
 assert.ok(frame.controls.length>0);for(const c of frame.controls){assert.equal(c.label,panel.rows[c.index]!.label);assert.ok(stripTerminalSequences(frame.rows[c.y]!).includes(c.label));assert.ok(c.x+c.width<=100&&c.y<40);}
 assert.equal(drawPanel(panel,100,40,false,'').controls.length,0);
});
