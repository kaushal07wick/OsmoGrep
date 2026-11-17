import {test} from 'node:test';
import assert from 'node:assert/strict';
import {stripTerminalSequences, visibleWidth} from '@earendil-works/pi-tui';
import {teamMessageLines,teamNoticeActive} from './team.ts';
import {render} from './renderer.ts';
import {fixture} from './test-fixture.ts';
import {drawPanel} from './panels.ts';

const receivedAt = Date.UTC(2026, 10, 17, 13, 20);

const entry = (outgoing = false) => 'TEAM|' + JSON.stringify({outgoing, message: {
  id: 1, sender: 'a073782e-d637-4e58-962c-8184925f952f', sender_title: 'API agent',
  recipient: 'ba000000-0000-0000-0000-000000000000', at: Date.UTC(2026, 10, 17, 13, 20),
  body: 'I am fixing retries.\nKeep the existing timeout.\n**Reference only.**',
}});

test('peer messages stay literal, wrap in both themes, and identify direction', () => {
  for (const light of [false, true]) for (const width of [24, 60, 110]) {
    const lines = teamMessageLines(entry(), width, light, '\x1b[38;2;235;135;65m')!;
    assert.ok(lines.every(line => visibleWidth(line) <= width));
    const text = lines.map(stripTerminalSequences).join('\n');
    assert.match(text, /↙ API agent/);
    assert.ok(text.includes('**Reference only.**'));
    assert.match(teamMessageLines(entry(true), width, light, '')!.join(''), /↗ to ba000000/);
  }
  assert.equal(teamMessageLines('TEAM|{bad}', 80, false, ''), null);
});

test('the latest message sits above the draft once, without a Team capsule', () => {
  const frame = render({...fixture(), width: 100, height: 32, userTurns: 2, wallTime: receivedAt + 1000,
    team: {enabled: true, unread: 1, online: 1},
    input: 'keep this draft', cursor: 15, logs: [{text: '· Finished in 2.0s', level: 'Info'}, {text: entry(), level: 'Info',kind:'Peer'}]});
  const text = frame.rows.map(stripTerminalSequences).join('\n');
  assert.match(text, /↙ API agent/);
  assert.match(text, /I am fixing retries/);
  assert.match(text, /keep this draft/);
  assert.ok(!text.includes('TEAM|'));
  assert.ok(!text.includes('Team 1'));
  assert.equal(text.match(/I am fixing retries/g)?.length, 1);
  const rows = frame.rows.map(stripTerminalSequences);
  const messageY = rows.findIndex(row => row.includes('↙ API agent'));
  assert.ok(messageY >= frame.input.y - 5 && messageY < frame.input.y - 1);
  assert.equal(rows[frame.input.y - 1]!.trim(), '');
  assert.ok(rows.findIndex(row => row.includes('Done')) < messageY - 1);
});

test('notices expire without being reprinted into chat or altering the draft', () => {
  assert.equal(teamNoticeActive(entry(), receivedAt + 6999), true);
  assert.equal(teamNoticeActive(entry(), receivedAt + 7000), false);
  const frame = render({...fixture(), width: 100, height: 40, userTurns: 2, wallTime: receivedAt + 8000,
    input: 'keep this draft', cursor: 15, logs: [{text: entry(), level: 'Info',kind:'Peer'}]});
  const text = frame.rows.map(stripTerminalSequences).join('\n');
  assert.doesNotMatch(text, /I am fixing retries|TEAM\|/);
  assert.match(text, /keep this draft/);
  assert.equal(frame.animate, false);
});

test('ordinary model output cannot impersonate a peer notification', () => {
  const frame = render({...fixture(), width:100, height:40, userTurns:2, wallTime:receivedAt+1000,
    logs:[{text:entry(),level:'Info',kind:'Output'}]});
  assert.ok(!frame.controls?.some(control => control.id === 'team'));
});

test('Team history exposes a full long message and a visible Show less target', () => {
  const panel = {sections:['Team'],section:11,query:'',selected:0,editing:false,rows:[],notice:'',footer:'',
    previewing:true,preview:Array.from({length:200},(_,i)=>`full message line ${i}`),preview_offset:190};
  const frame = drawPanel(panel, 100, 32, false, '');
  const text = frame.rows.map(stripTerminalSequences).join('\n');
  assert.match(text, /full message line 199/);
  assert.ok(frame.controls.some(control => control.label === 'Show less' && control.height === 1));
});

test('large answers fold with clickable Show more and Show less without losing source', () => {
  const source = Array.from({length:100},(_,i)=>`answer line ${i}`).join('\n');
  const view = {...fixture(),width:100,height:44,userTurns:2,input:'my draft',logs:[{text:source,level:'Info'}]};
  const folded = render(view);
  assert.match(folded.rows.map(stripTerminalSequences).join('\n'), /Show more/);
  assert.ok(folded.controls?.some(control => control.id === 'expand' && control.height === 1));
  const expanded = render({...view,expandTools:true});
  const text = expanded.rows.map(stripTerminalSequences).join('\n');
  assert.match(text, /answer line 99/);
  assert.match(text, /Show less/);
  assert.equal(view.logs[0]!.text, source);
});
