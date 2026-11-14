import {inflateSync,crc32} from 'node:zlib';
import {test} from 'node:test';import assert from 'node:assert/strict';
import {setCapabilities,stripTerminalSequences,visibleWidth} from '@earendil-works/pi-tui';
import {paperRow,paperPng,paperGraphic} from './paper.ts';import {render} from './renderer.ts';import {fixture} from './test-fixture.ts';
test('paper texture is stable, theme-specific and preserves selectable Unicode text',()=>{
 for(const light of [false,true]){
  const text='Hello 世界 · 👩‍💻';const row=paperRow(text,40,1,3,light);
  assert.equal(stripTerminalSequences(row).trimEnd(),text);assert.equal(visibleWidth(row),40);assert.equal(row,paperRow(text,40,1,3,light));
  assert.ok(new Set(row.match(/\x1b\[48;2;[0-9;]+m/g)).size===1);assert.notEqual(row,paperRow(text,40,1,3,!light));
  assert.ok(!/[░▒▓⠂]/.test(stripTerminalSequences(row)),'fallback has no copied decoration');
 }
});
test('both material themes preserve composer geometry, selection and tiny terminal bounds',()=>{
 for(const theme of ['light','dark'])for(const[width,height]of [[1,1],[20,6],[60,18],[160,44]]){
  const v={...fixture(),theme,width:width!,height:height!,input:'a界c\nsecond line',cursor:3,selection:[1,2] as [number,number]};const f=render(v);
  assert.equal(f.rows.length,height);assert.ok(f.rows.every(r=>visibleWidth(r)===width));assert.ok(f.cursor.x<width!&&f.cursor.y<height!);if(width!>=60)assert.ok(f.rows.join('').includes('\x1b[7m'));
 }
});

test('native paper contains fine pixel grain inside a single text cell and valid PNG chunks',()=>{
 for(const light of [false,true]){
  const png=Buffer.from(paperPng(30,3,light),'base64'),data:Buffer[]=[];
  assert.equal(png.subarray(0,8).toString('hex'),'89504e470d0a1a0a');
  for(let at=8;at<png.length;){const length=png.readUInt32BE(at),type=png.toString('ascii',at+4,at+8),payload=png.subarray(at+8,at+8+length);assert.equal(crc32(png.subarray(at+4,at+8+length)),png.readUInt32BE(at+8+length));if(type==='IHDR'){assert.equal(payload.readUInt32BE(),480);assert.equal(payload.readUInt32BE(4),96);}if(type==='IDAT')data.push(payload);at+=12+length;}
  const raw=inflateSync(Buffer.concat(data)),grain=new Set<number>();
  for(let y=32;y<64;y++)for(let x=16;x<32;x++)grain.add(raw[y*(480*3+1)+1+x*3]!);
  assert.ok(grain.size>=10,'grain must vary within one cell, rather than between cells');
  assert.equal(paperPng(30,3,light),png.toString('base64'));
 }
});
test('Kitty paper is placed below selection backgrounds and is independent of editable text',()=>{
 setCapabilities({images:'kitty',trueColor:true,hyperlinks:false});
 try{
  const v={...fixture(),input:'select this',cursor:5,selection:[0,6] as [number,number]},frame=render(v),paper=frame.images.find(i=>i.placement)!;
  assert.ok(paper);assert.match(paper.sequence,/a=t,/);assert.ok(!paper.sequence.includes('a=T,'));
  assert.match(paper.placement!,/a=p,.*z=-1073741825,C=1/);assert.equal(paper.x,frame.input.x);assert.equal(paper.y,frame.input.y);
  const light=render({...v,theme:'light'});assert.match(light.rows[light.cursor.y]!,/\x1b\[7m\x1b\[48;2;220;214;199m/);
  assert.ok(frame.rows[frame.cursor.y]!.includes('\x1b[49m'));assert.ok(frame.rows[frame.cursor.y]!.includes('\x1b[7m'));
  assert.match(frame.rows.map(stripTerminalSequences).join(''),/select this/);
  assert.equal(paper.sequence,render({...v,input:'different text',cursor:2}).images.find(i=>i.placement)!.sequence);
  assert.notEqual(paper.sequence,paperGraphic(frame.input.width,frame.input.height,true,paper.id).sequence);
 }finally{setCapabilities({images:null,trueColor:true,hyperlinks:false});}
 assert.equal(render(fixture()).images.length,0);
});
