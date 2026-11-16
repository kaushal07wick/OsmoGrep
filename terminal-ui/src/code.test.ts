import {test} from 'node:test';
import assert from 'node:assert/strict';
import {stripTerminalSequences,visibleWidth,setCapabilities} from '@earendil-works/pi-tui';
import {codeBlocks,cachedCodeBlocks} from './code.ts';
import {render} from './renderer.ts';
import {fixture} from './test-fixture.ts';
test('lean code cards keep source indentation, omit language headings and use an orange copy control', () => {
 const code = 'import random\n\ndef run():\n    return 1';
 const block = codeBlocks('```python\n'+code+'\n```',80,false,text=>[text]);
 const rows = block.rows.map(stripTerminalSequences);
 assert.ok(rows.some(row=>row.startsWith(' import random')));
 assert.ok(rows.some(row=>row.startsWith('     return 1')));
 assert.ok(!rows.some(row=>/SHELL|PYTHON/.test(row)));
 assert.equal(block.copies[0]!.text,code);
 assert.match(block.rows[block.copies[0]!.row]!,/235;155;105/);
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
 for(let i=0;i<129;i++)cachedCodeBlocks('eviction '+i,80,false,markdown);
 assert.notStrictEqual(cachedCodeBlocks('cache fixture',80,false,markdown),first);
});
test('fenced commands have a highlighted surface and copy the exact source without fences',()=>{
 const code='python hey_bro_triangle.py\nprintf "hey 界"\n\techo --help';
 for(const light of [false,true])for(const width of [1,14,40,100]){
  const block=codeBlocks('Run this:\n\n```bash\n'+code+'\n```\n\nDone.',width,light,s=>[s]);
  assert.ok(block.rows.every(row=>visibleWidth(row)<=width||!row.includes('\x1b')));
  if(width>=14){assert.equal(block.copies.length,1);assert.equal(block.copies[0]!.text,code);assert.match(block.rows[block.copies[0]!.row]!,/⧉ Copy/);assert.ok(block.rows.some(r=>r.includes('95;170;190')));}
  if(width>=14){const target=block.copies[0]!;assert.equal(stripTerminalSequences(block.rows[target.row-1]!).trim(),'');assert.equal(stripTerminalSequences(block.rows.filter(row=>row.includes('\x1b[48;2;')).at(-1)!).trim(),'');assert.ok(target.x+target.width<width);}
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
 const f=render({...v,followTail:false,scrollTop:0}),target=f.controls!.find(c=>c.id==='copy')!;
 assert.equal(target.label,code);assert.ok(stripTerminalSequences(f.rows[target.y]!).includes('⧉ Copy'));assert.ok(target.y<f.input.y-1);
 const copied=render({...v,followTail:false,scrollTop:0,copiedCode:code});assert.match(copied.rows.map(stripTerminalSequences).join('\n'),/✓ Copied/);
 assert.ok(!render({...v,masked:true,followTail:false,scrollTop:0}).controls?.some(c=>c.id==='copy'));
});
