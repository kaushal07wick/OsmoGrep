import {test} from 'node:test';import assert from 'node:assert/strict';
import {stripTerminalSequences,visibleWidth} from '@earendil-works/pi-tui';
import {paperRow} from './paper.ts';import {render} from './renderer.ts';import {fixture} from './test-fixture.ts';
test('paper texture is stable, theme-specific and preserves selectable Unicode text',()=>{
 for(const light of [false,true]){
  const text='Hello 世界 · 👩‍💻';const row=paperRow(text,40,1,3,light);
  assert.equal(stripTerminalSequences(row).trimEnd(),text);assert.equal(visibleWidth(row),40);assert.equal(row,paperRow(text,40,1,3,light));
  assert.ok(new Set(row.match(/\x1b\[48;2;[0-9;]+m/g)).size>1);assert.notEqual(row,paperRow(text,40,1,3,!light));
  assert.ok(!/[░▒▓⠂]/.test(stripTerminalSequences(row)),'grain uses background cells, not copied decoration');
 }
});
test('both material themes preserve composer geometry, selection and tiny terminal bounds',()=>{
 for(const theme of ['light','dark'])for(const[width,height]of [[1,1],[20,6],[60,18],[160,44]]){
  const v={...fixture(),theme,width:width!,height:height!,input:'a界c\nsecond line',cursor:3,selection:[1,2] as [number,number]};const f=render(v);
  assert.equal(f.rows.length,height);assert.ok(f.rows.every(r=>visibleWidth(r)===width));assert.ok(f.cursor.x<width!&&f.cursor.y<height!);if(width!>=60)assert.ok(f.rows.join('').includes('\x1b[7m'));
 }
});
