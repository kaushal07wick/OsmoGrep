import {test} from 'node:test';
import assert from 'node:assert/strict';
import {stripTerminalSequences,visibleWidth,setCapabilities} from '@earendil-works/pi-tui';
import {codeBlocks,cachedCodeBlocks,copyTabIcon} from './code.ts';
import {inflateSync} from 'node:zlib';
import {render} from './renderer.ts';
import {fixture} from './test-fixture.ts';
test('orange tabs keep source indentation and separate the action from multiline code', () => {
 const code = 'import random\n\ndef run():\n    return 1';
 const block = codeBlocks('```python\n'+code+'\n```',80,false,text=>[text]);
 const rows = block.rows.map(stripTerminalSequences);
 assert.ok(rows.some(row=>row.startsWith('  import random')));
 assert.ok(rows.some(row=>row.startsWith('      return 1')));
 assert.ok(!rows.some(row=>/SHELL|PYTHON/.test(row)));
 assert.equal(block.copies[0]!.text,code);
 const target=block.copies[0]!;
 assert.match(block.rows[target.row]!,/48;2;232;117;43/);
 assert.equal(rows[target.row+1]!.trim(),'');
 assert.ok(!rows[target.row]!.includes('import'));
 assert.equal(target.x+target.width,80);
});
test('settled transcript cache invalidates on source, width, theme, copy feedback and style', () => {
 let calls=0;
 const markdown=(text:string)=>{calls++;return [text];};
 const first=cachedCodeBlocks('cache fixture',80,false,markdown);
 assert.strictEqual(cachedCodeBlocks('cache fixture',80,false,markdown),first);
 assert.equal(calls,1);
 for(const [text,width,light,copied,style] of [['new source',80,false,'',''],['cache fixture',79,false,'',''],['cache fixture',80,true,'',''],['cache fixture',80,false,'copied',''],['cache fixture',80,false,'','blue']] as const){
  assert.notStrictEqual(cachedCodeBlocks(text,width,light,markdown,copied,style),first);
 }
 assert.equal(calls,6);
 assert.notStrictEqual(cachedCodeBlocks('cache fixture',80,false,markdown,'','',true),first);
 for(let i=0;i<129;i++)cachedCodeBlocks('eviction '+i,80,false,markdown);
 assert.notStrictEqual(cachedCodeBlocks('cache fixture',80,false,markdown),first);
});
test('fenced commands have a highlighted surface and copy the exact source without fences',()=>{
 const code='python hey_bro_triangle.py\nprintf "hey 界"\n\techo --help';
 for(const light of [false,true])for(const width of [1,14,40,100]){
  const block=codeBlocks('Run this:\n\n```bash\n'+code+'\n```\n\nDone.',width,light,s=>[s]);
  assert.ok(block.rows.every(row=>visibleWidth(row)<=width||!row.includes('\x1b')));
  if(width>=14){assert.equal(block.copies.length,1);assert.equal(block.copies[0]!.text,code);assert.match(block.rows[block.copies[0]!.row]!,/ Copy/);assert.ok(block.rows.some(r=>r.includes(light?'45;105;125':'95;170;190')));}
  if(width>=14){const target=block.copies[0]!;assert.equal(stripTerminalSequences(block.rows[target.row+1]!).trim(),'');assert.equal(stripTerminalSequences(block.rows.filter(row=>row.includes('\x1b[48;2;')).at(-1)!).trim(),'');assert.equal(target.x+target.width,width);}
  assert.ok(!block.rows.join('\n').includes('```'));
 }
 const unsafe=codeBlocks('```bash\necho hi\x1b[2J\x07\n```',40,false,s=>[s]);assert.equal(unsafe.copies[0]!.text,'echo hi');assert.ok(!unsafe.rows.join('').includes('\x1b[2J'));
 assert.equal(codeBlocks('An inline `command`.',40,false,s=>[s]).copies.length,0);
 const blank=codeBlocks('```python\nx = 1\n\nprint(x)\n```',40,false,s=>[s]);assert.ok(!blank.rows.map(stripTerminalSequences).join('\n').includes('$1'));
 const tilde=codeBlocks('~~~~py\nx = "```"\n~~~~',40,false,s=>[s]);assert.equal(tilde.copies[0]!.text,'x = "```"');
});
test('copy targets track code headers through transcript scrolling and copy feedback',()=>{
 setCapabilities({images:null,trueColor:true,hyperlinks:false});
 const code='python hey_bro_triangle.py',logs=[{level:'Info',text:'```bash\n'+code+'\n```'},...Array.from({length:60},(_,i)=>({level:'Success',text:'Line '+i}))];
 const v={...fixture(),logs};assert.ok(!render(v).controls?.some(c=>c.id==='copy'));
 const f=render({...v,followTail:false,scrollTop:0}),target=f.controls!.find(c=>c.id==='copy'&&stripTerminalSequences(f.rows[c.y]!).includes(' Copy'))!;
 assert.equal(target.label,code);assert.ok(f.rows.slice(target.y,target.y+target.height).some(row=>stripTerminalSequences(row).includes(' Copy')));assert.ok(target.y<f.input.y-1);
 const copied=render({...v,followTail:false,scrollTop:0,copiedCode:code});assert.match(copied.rows.map(stripTerminalSequences).join('\n'),/ Copied/);
 assert.deepEqual(copied.controls!.find(c=>c.id==='copy'&&c.y===target.y),target);
 assert.ok(!render({...v,masked:true,followTail:false,scrollTop:0}).controls?.some(c=>c.id==='copy'));
});
test('single commands stay inline without colliding with a stable copy tab in either theme',()=>{
 for(const light of [false,true])for(const graphics of [false,true])for(const width of [14,24,40,100]){
  const code='python retry.py',before=codeBlocks('```sh\n'+code+'\n```',width,light,s=>[s],'',graphics),after=codeBlocks('```sh\n'+code+'\n```',width,light,s=>[s],code,graphics);
  assert.deepEqual(before.copies,after.copies);
  assert.ok(before.rows.every(row=>visibleWidth(row)<=width));
  assert.ok(after.rows.every(row=>visibleWidth(row)<=width));
  assert.ok(!before.rows.join('').includes('⧉'));
  const target=before.copies[0]!;
  if(width>=40){assert.equal(target.height,3);assert.ok(stripTerminalSequences(before.rows[target.row+1]!).includes(code));}
  if(graphics)assert.equal(target.iconRow,target.height===3?1:0);
 }
});
test('copy pixels use the tab background and native graphics follow visible controls only',()=>{
 const png=Buffer.from(copyTabIcon(false,false),'base64'),idats:Buffer[]=[];
 assert.equal(png.readUInt32BE(16),20);assert.equal(png.readUInt32BE(20),20);
 for(let i=8;i<png.length;){const size=png.readUInt32BE(i);if(png.toString('ascii',i+4,i+8)==='IDAT')idats.push(png.subarray(i+8,i+8+size));i+=size+12;}
 const pixels=inflateSync(Buffer.concat(idats));
 assert.deepEqual([...pixels.subarray(1,4)],[232,117,43]);
 assert.deepEqual([...pixels.subarray(6*61+1+2*3,6*61+4+2*3)],[28,16,8]);
 assert.notEqual(copyTabIcon(false,true),copyTabIcon(false,false));
 setCapabilities({images:'kitty',trueColor:true,hyperlinks:false});
 try{
  const v={...fixture(),height:30,width:80,logs:Array.from({length:14},(_,i)=>({level:'Info',text:'```sh\necho '+i+'\n```'}))};
  for(const scrollTop of [0,1,2,3,8,20]){
   const frame=render({...v,followTail:false,scrollTop}),copies=frame.controls!.filter(c=>c.id==='copy'),icons=frame.images.filter(i=>i.id>=0x40000000);
   assert.ok(icons.length>0);
   assert.ok(copies.every(c=>c.height===1&&c.width>0&&c.x+c.width<=v.width&&c.y>=frame.execution.y&&c.y<frame.execution.y+frame.execution.height));
   assert.equal(new Set(frame.images.map(i=>i.id)).size,frame.images.length);
   for(const icon of icons){assert.ok(icon.y>=frame.execution.y&&icon.y<frame.execution.y+frame.execution.height);assert.ok(copies.some(c=>icon.x===c.x+1&&icon.y>=c.y&&icon.y<c.y+c.height));assert.match(icon.placement!,/c=2,r=1/);}
  }
  const hidden=render({...v,commands:[{cmd:'/help',desc:'Help'}]});
  assert.ok(!hidden.images.some(i=>i.id>=0x40000000));assert.ok(!hidden.controls!.some(c=>c.id==='copy'));
 }finally{setCapabilities({images:null,trueColor:true,hyperlinks:false});}
});
