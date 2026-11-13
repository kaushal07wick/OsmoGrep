import { test } from 'node:test';
import assert from 'node:assert/strict';
import { setCapabilities, stripTerminalSequences, visibleWidth } from '@earendil-works/pi-tui';
import { clean, render, type View } from './renderer.ts';
import { introDuration, websiteMark } from './brand.ts';
import { siteLogo } from './site-art.ts';

export function fixture(): View {
  return {
    id: 1, width: 160, height: 44, version: 'test', cwd: '~/project', model: 'test-model', provider: 'local',
    branch: 'main', indexed: true, indexing: false, theme: 'dark', accent: 'orange', density: 'standard', session: null, elapsed: 0,
    profile: 'workspace-auto', autoApprove: false, planMode: false, logs: [], plan: [], diffs: [],
    input: '', cursor: 0, selection: null, masked: false, inputMode: 'AgentText', attachments: [],
    running: false, phase: 'idle', tool: null, followTail: true, scroll: 0, stream: '', permission: null,
    update: null, voice: null, commands: [], selectedCommand: 0, tokens: 0,
  };
}
test('matches the reference geometry at idle and during conversation', () => {
  const v = fixture(), idle = render(v);
  assert.equal(idle.input.x, 1); assert.equal(idle.input.width, 158);
  assert.equal(idle.input.y + idle.input.height, 42);
  assert.match(stripTerminalSequences(idle.rows[1]!), /osmogrep.*test/);
  v.logs = [{ text: 'USER|Explain the code', level: 'Info' }, { text: 'A paragraph.\n\nAnother paragraph.', level: 'Info' }];
  const active = render(v);
  assert.deepEqual(idle.input, active.input);
  assert.match(active.rows.map(row => stripTerminalSequences(row).trimEnd()).join('\n'), /A paragraph\.\n\s*\n.*Another paragraph\./);
});
test('bounded layout, editor and cursor at tiny and wide sizes', () => {
  for (const [width, height] of [[1,1], [20,5], [40,12], [80,24], [160,44], [240,80]]) {
    const v = { ...fixture(), width: width!, height: height!, input: '界 hello\nsecond line', cursor: 8 };
    const f = render(v);
    assert.equal(f.rows.length, height);
    assert.ok(f.rows.every(row => visibleWidth(row) <= width!));
    assert.ok(f.cursor.x < width! && f.cursor.y < height!);
    assert.ok(f.input.y + f.input.height <= height!);
  }
});
test('long text scrolls to the cursor without losing the draft', () => {
  const v = fixture(); v.input = 'hello 界\n'.repeat(25_000); v.cursor = Array.from(v.input).length;
  const frame = render(v);
  assert.ok(frame.input.height <= 10);
  assert.ok(frame.cursor.y >= frame.input.y && frame.cursor.y < frame.input.y + frame.input.height);
  v.cursor = 3;
  const edited = render(v);
  assert.equal(edited.cursor.x, 8);
  assert.equal(v.input.length, 'hello 界\n'.length * 25_000);
});
test('untrusted terminal controls cannot change the terminal', () => {
  assert.equal(clean('hello\x1b]52;c;ZXhmaWw=\x07world\x1b[2J'), 'helloworld');
  const v = fixture(); v.logs = [{ level: 'Info', text: '\x1b]52;c;ZXhmaWw=\x07hello' }];
  assert.ok(!render(v).rows.join('').includes(']52;'));
});
test('shell commands appear once', () => {
  const v = fixture(); v.logs = [{text:'USER|ls -al',level:'Info'},{text:'SHELL : $ ls -al',level:'Info'},{text:'src',level:'Info'}];
  const text = render(v).rows.map(stripTerminalSequences).join('\n');
  assert.equal(text.match(/ls -al/g)?.length, 1); assert.ok(!text.includes('SHELL :'));
});
test('logo animation moves its accent without changing the deployed silhouette', () => {
  assert.deepEqual(siteLogo,['...iiiii...','.iiiiiiiii.','.ii.....ii.','ii.......ii','ii.......aa','ii.......aa','ii.......ii','.ii.....ii.','.iiiiiiiii.','...iiiii...']);
  assert.ok(websiteMark().every(row=>visibleWidth(row)===7));
  assert.ok(websiteMark().join('').includes('234;88;12'));
  const still = websiteMark();
  assert.notDeepEqual(websiteMark(false, 0), websiteMark(false, 500));
  for (let ms = 0; ms <= introDuration; ms += 125) {
    const frame = websiteMark(false, ms);
    // Mixed foreground/background halves still fill the same terminal cell.
    const silhouette = (rows: string[]) => rows.map(row => stripTerminalSequences(row).replace(/[▀▄█]/g, '█'));
    assert.deepEqual(silhouette(frame), silhouette(still));
    assert.ok(frame.every(row => visibleWidth(row) === 7));
  }
  assert.deepEqual(websiteMark(false, introDuration), still);
  assert.deepEqual(websiteMark(false, introDuration + 500), still);
});
test('only the header logo animates for ten seconds, including while editing', () => {
  const v=fixture();
  const first=render(v);
  assert.equal(first.animate,true);
  v.elapsed=500;
  const next=render(v);
  assert.notDeepEqual(first.rows.slice(1,4),next.rows.slice(1,4));
  assert.deepEqual(first.rows.slice(4),next.rows.slice(4));
  assert.ok(next.rows.slice(next.execution.y,next.execution.y+next.execution.height).every(row => stripTerminalSequences(row).trim() === ''));
  v.elapsed=introDuration;
  assert.equal(render(v).animate,false);
  v.elapsed=0;v.input='typing';
  assert.equal(render(v).animate,true);
  v.density='compact';
  assert.equal(render(v).animate,false);
  const text=render(v).rows.map(stripTerminalSequences).join('\n');
  assert.ok(!text.includes('Catch the bug.'));
  assert.ok(render(v).rows.every(row=>row.startsWith('\x1b[48;2;0;0;0m')));
});
test('composer has a continuous padded surface with room around text and footer', () => {
  const v = fixture(), f = render(v);
  assert.equal(f.input.height, 3);
  assert.equal(f.cursor.y, f.input.y + 1);
  assert.equal(f.cursor.x, f.input.x + 4);
  for (let y=f.input.y; y<f.input.y+f.input.height; y++) {
    assert.ok(f.rows[y]!.includes('\x1b[48;2;30;30;30m'));
    assert.ok(!stripTerminalSequences(f.rows[y]!).includes('─'));
  }
  assert.equal(stripTerminalSequences(f.rows[f.input.y]!).trim(), '');
  assert.equal(stripTerminalSequences(f.rows[f.input.y+2]!).trim(), '');
  assert.equal(stripTerminalSequences(f.rows[v.height-2]!).trim(), '');
  v.input='first\nsecond\nthird';v.cursor=Array.from(v.input).length;
  const expanded=render(v);
  assert.equal(expanded.input.height, 5);
  assert.equal(expanded.cursor.y, expanded.input.y+3);
  assert.equal(expanded.editorWidth, expanded.input.width-6);
});
test('selected input is highlighted and secret entry has no placeholder', () => {
  const v = fixture(); v.input = 'abc'; v.cursor = 2; v.selection = [1,2];
  assert.ok(render(v).rows.join('').includes('\x1b[7mb\x1b[27m'));
  v.input = ''; v.masked = true;
  assert.ok(!render(v).rows.join('').includes('Try'));
});
test('real thumbnail is emitted inside the composer on supported terminals', () => {
  setCapabilities({ images: 'kitty', trueColor: true, hyperlinks: false });
  const v = fixture(); v.input = 'Look [Image #1]'; v.cursor = 15;
  v.attachments = [{id:1,label:'[Image #1]',thumbnail:'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aJ9sAAAAASUVORK5CYII=',width:1,height:1}];
  const f = render(v);
  assert.equal(f.images.length, 1); assert.ok(f.images[0]!.sequence.includes('\x1b_G'));
  assert.ok(f.images[0]!.y > f.input.y && f.images[0]!.y < f.cursor.y);
  v.input = 'Image removed';
  assert.equal(render(v).images.length, 0);
});
test('approval, update, voice and plan are visible', () => {
  const v = fixture(); v.permission = 'Allow shell? [y]es [n]o'; v.plan = [{text:'Inspect code',done:false,active:true}];
  let text = render(v).rows.map(stripTerminalSequences).join('\n');
  assert.match(text,/Allow shell/); assert.match(text,/Inspect code/);
  v.permission = null; v.update = 'Install update?';
  assert.match(render(v).rows.map(stripTerminalSequences).join('\n'),/Install update/);
  v.update = null; v.voice = 'voice connected';
  assert.match(render(v).rows.map(stripTerminalSequences).join('\n'),/voice connected/);
});

test('file diffs preserve added and removed markers', () => {
  const v = fixture();
  v.diffs = [{file:'src/main.rs',lines:[{text:'old code',kind:'Removed'},{text:'new code',kind:'Added'}]}];
  const text = render(v).rows.map(stripTerminalSequences).join('\n');
  assert.match(text,/- old code/); assert.match(text,/\+ new code/);
});

test('light theme paints a light surface with readable dark text', () => {
  const v = fixture(); v.theme = 'light';
  const frame = render(v);
  assert.ok(frame.rows.every(row => row.startsWith('\x1b[48;2;250;250;250m')));
  assert.ok(frame.rows[1]!.includes('\x1b[38;2;35;35;35m'));
  assert.ok(frame.rows.every(row => visibleWidth(row) === v.width));
});

test('existing density and named-session settings are reflected', () => {
  const v = fixture(); v.density = 'compact'; v.session = 'parser work';
  const compact = render(v);
  assert.match(stripTerminalSequences(compact.rows[0]!),/osmogrep.*parser work/);
  v.density = 'spacious';
  const spacious = render(v);
  assert.match(stripTerminalSequences(spacious.rows[2]!),/osmogrep.*parser work/);
  assert.ok(spacious.execution.y > compact.execution.y);
});

test('command picker uses a bounded dark card and keeps selection visible while scrolling', () => {
  const v = fixture(); v.input = '/'; v.cursor = 1;
  v.commands = Array.from({length:20}, (_,i) => ({cmd:`/command-${i}`,desc:`Description ${i}`}));
  v.selectedCommand = 14; v.permission = 'Pending approval';
  const f = render(v), text = f.rows.map(stripTerminalSequences).join('\n');
  assert.match(text, /Commands.*15 \/ 20/);
  assert.match(text, /› \/command-14\s+Description 14/);
  assert.match(text, /↑↓ navigate.*enter run/);
  assert.match(text, /Pending approval/);
  assert.equal(text.match(/Description \d+/g)?.length, 6);
  assert.ok(!text.includes('Description 0'));
  assert.ok(!f.rows.join('').includes('\x1b[7m'));
  const selected = f.rows.find(row => row.includes('Description 14'))!;
  assert.ok(selected.includes('\x1b[48;2;42;42;42m'));
  assert.ok(f.rows.indexOf(selected) < f.input.y - 1);
  v.commands = [{cmd:'/key',desc:'Configure credentials'}];v.selectedCommand = 0;
  assert.match(render(v).rows.map(stripTerminalSequences).join('\n'), /1 \/ 1/);
});

test('command picker fits small terminals, preserves the editor and supports light theme', () => {
  for (const theme of ['dark','light']) for (const [width,height] of [[1,1],[20,5],[40,12],[60,18],[160,44]]) {
    const v = {...fixture(),width:width!,height:height!,theme,input:'/',cursor:1,selectedCommand:2};
    v.commands = [{cmd:'/help',desc:'Help'}, {cmd:'/日本語-long-command',desc:'Unicode description'}, {cmd:'/key',desc:'Set API key'}];
    const f = render(v);
    assert.ok(f.rows.every(row => visibleWidth(row) === width));
    assert.ok(f.cursor.y >= f.input.y && f.cursor.y < f.input.y+f.input.height);
    if (height! >= 12) assert.match(f.rows.map(stripTerminalSequences).join('\n'), /› \/key/);
    if (theme === 'light' && height === 44) assert.ok(f.rows.join('').includes('\x1b[48;2;222;222;222m'));
  }
});
