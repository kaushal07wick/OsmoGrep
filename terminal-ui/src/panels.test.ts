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


test('settings lists every category at normal size without hidden tab navigation',()=>{
 const categories=['Appearance','Model & provider','Permissions','Status line','Keyboard','Skills','Usage','Activity'];
 const settings={...panel,section:12,settings:true,selecting:true,selected:0,title:'Settings',rows:categories.map(label=>({label,description:'Configure this category',value:'→'})),footer:'↑↓ choose category · enter open · esc close'};
 for(const [width,height] of [[80,24],[120,40]]){
  const f=drawPanel(settings,width!,height!,false,''),text=f.rows.map(stripTerminalSequences).join('\n');
  for(const label of categories)assert.ok(text.includes(label),label);
  assert.equal(f.controls.length,8);assert.ok(!text.includes('tab'));
 }
 const category=drawPanel({...settings,section:3,sections:['','','','Permissions'],title:'Permissions',footer:'esc categories'},80,30,false,'');
 assert.match(category.rows.map(stripTerminalSequences).join('\n'),/Settings \/ Permissions/);
});

test('doctor shows animated checking state and colored completed results',()=>{
 for(const light of [false,true]){
  const health={...panel,section:14,selected:0,elapsed:0,health_running:true,rows:[
   {label:'Provider connection',description:'Model catalog',value:'Checking',tag:'checking'},
   {label:'Storage',description:'Private files',value:'OK',tag:'ok'},
   {label:'Credentials',description:'Needs attention',value:'WARN',tag:'warn'},
   {label:'Snapshot',description:'Invalid',value:'FAIL',tag:'fail'},
  ]};
  const a=drawPanel(health,120,40,light,'\x1b[38;2;235;155;105m').rows.join('\n');
  const b=drawPanel({...health,elapsed:150},120,40,light,'\x1b[38;2;235;155;105m').rows.join('\n');
  assert.notEqual(a,b); assert.match(a,/◐ Checking/);
  assert.ok(a.includes(light?'30;130;75':'100;190;130'));
  assert.ok(a.includes('230;100;100'));
 }
});

test('Lab reports use the shared evidence reader and never look like team messages',()=>{
 const f=drawPanel({...panel,section:16,title:'Replays',previewing:true,preview:['Captured source revision','Reproduction command (not executed)','Independent expected behavior']},80,28,false,'');
 const text=f.rows.map(stripTerminalSequences).join('\n');
 assert.match(text,/Replays/);assert.match(text,/not executed/);assert.match(text,/Show less/);assert.doesNotMatch(text,/Team message/);
 assert.ok(f.rows.every(row=>visibleWidth(row)<=80));
});


test('account approval keeps the code visible, bounded and animated only while waiting',()=>{
 const account={...panel,section:22,title:'Account',sections:Array.from({length:23},(_,i)=>i===22?'Account':'Workspace'),settings:true,selecting:true,selected:0,account_running:true,notice:'Confirm the code in your browser',rows:[{label:'Confirm code',description:'Match the browser before approving',value:'ABCD-EFGH'},{label:'Open browser',description:'Continue with GitHub',value:'Open'},{label:'Cancel',description:'Cancel browser authorization',value:'Cancel'}]};
 const frame=drawPanel(account,100,30,false,'');
 const text=frame.rows.map(stripTerminalSequences).join('\n');
 assert.match(text,/ABCD-EFGH/);assert.match(text,/Confirm the code/);assert.match(text,/Cancel/);
 assert.ok(frame.rows.every(row=>visibleWidth(row)<=100));
});
