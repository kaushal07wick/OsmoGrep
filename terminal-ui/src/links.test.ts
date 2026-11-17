import {test} from 'node:test';
import assert from 'node:assert/strict';
import {stripTerminalSequences,visibleWidth} from '@earendil-works/pi-tui';
import {webLinks,validWebLink} from './links.ts';
import {render} from './renderer.ts';
import {fixture} from './test-fixture.ts';

test('web links have a blue tint and exact targets without native browser escape sequences',()=>{
  for(const light of [false,true]){
    const source='Dev server: http://127.0.0.1:4173.';
    const marked=webLinks(source,light);
    assert.equal(marked.links.length,1);
    assert.equal(marked.links[0]!.url,'http://127.0.0.1:4173');
    assert.equal(stripTerminalSequences(marked.text),source);
    assert.ok(marked.text.includes(light?'38;2;38;103;175':'38;2;112;166;224'));
    const named=webLinks('\x1b]8;;https://example.com/docs\x1b\\Read docs\x1b]8;;\x1b\\',light);
    assert.equal(named.links[0]!.url,'https://example.com/docs');
    assert.equal(named.links[0]!.width,9);
    assert.ok(!named.text.includes('\x1b]8;'));
  }
  for(const unsafe of ['file:///tmp/a','javascript:alert(1)','https://user:password@example.com','https://example.com/\ncommand'])assert.equal(validWebLink(unsafe),false);
});
test('visible links expose bounded clickable controls and preserve the draft',()=>{
  const frame=render({...fixture(),width:80,height:32,userTurns:2,input:'my draft',logs:[{text:'Running at http://127.0.0.1:4173',level:'Info'}]});
  const link=frame.controls?.find(control=>control.id==='link');
  assert.equal(link?.label,'http://127.0.0.1:4173');
  assert.equal(link?.height,1);
  assert.match(frame.rows.map(stripTerminalSequences).join('\n'),/my draft/);
  assert.ok(frame.rows.every(row=>visibleWidth(row)<=80));
});
