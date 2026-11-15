import {test} from 'node:test';
import assert from 'node:assert/strict';
import {stripTerminalSequences,visibleWidth,setCapabilities} from '@earendil-works/pi-tui';
import {codeBlocks} from './code.ts';
import {render} from './renderer.ts';
import {fixture} from './test-fixture.ts';
test('fenced commands have a highlighted surface and copy the exact source without fences',()=>{
 const code='python hey_bro_triangle.py\nprintf "hey 界"\n\techo --help';
 for(const light of [false,true])for(const width of [1,14,40,100]){
  const block=codeBlocks('Run this:\n\n```bash\n'+code+'\n```\n\nDone.',width,light,s=>[s]);
  assert.ok(block.rows.every(row=>visibleWidth(row)<=width||!row.includes('\x1b')));
  if(width>=14){assert.equal(block.copies.length,1);assert.equal(block.copies[0]!.text,code);assert.match(block.rows[block.copies[0]!.row]!,/\[Copy\]/);assert.ok(block.rows.some(r=>r.includes('95;170;190')));}
  assert.ok(!block.rows.join('\n').includes('```'));
 }
 const unsafe=codeBlocks('```bash\necho hi\x1b[2J\x07\n```',40,false,s=>[s]);assert.equal(unsafe.copies[0]!.text,'echo hi');assert.ok(!unsafe.rows.join('').includes('\x1b[2J'));
 assert.equal(codeBlocks('An inline `command`.',40,false,s=>[s]).copies.length,0);
 const tilde=codeBlocks('~~~~py\nx = "```"\n~~~~',40,false,s=>[s]);assert.equal(tilde.copies[0]!.text,'x = "```"');
});
test('copy targets track code headers through transcript scrolling and copy feedback',()=>{
 setCapabilities({images:null,trueColor:true,hyperlinks:false});
 const code='python hey_bro_triangle.py',logs=[{level:'Info',text:'```bash\n'+code+'\n```'},...Array.from({length:60},(_,i)=>({level:'Success',text:'Line '+i}))];
 const v={...fixture(),logs};assert.ok(!render(v).controls?.some(c=>c.id==='copy'));
 const f=render({...v,followTail:false,scrollTop:0}),target=f.controls!.find(c=>c.id==='copy')!;
 assert.equal(target.label,code);assert.ok(stripTerminalSequences(f.rows[target.y]!).includes('[Copy]'));assert.ok(target.y<f.input.y-1);
 const copied=render({...v,followTail:false,scrollTop:0,copiedCode:code});assert.match(copied.rows.map(stripTerminalSequences).join('\n'),/\[Copied\]/);
 assert.ok(!render({...v,masked:true,followTail:false,scrollTop:0}).controls?.some(c=>c.id==='copy'));
});
