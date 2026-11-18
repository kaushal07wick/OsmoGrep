import { test } from 'node:test';
import assert from 'node:assert/strict';
import { setCapabilities, stripTerminalSequences, visibleWidth } from '@earendil-works/pi-tui';
import { clean, render, type View } from './renderer.ts';
import { introDuration, websiteMark } from './brand.ts';
import { siteLogo } from './site-art.ts';
import { introFrame, introPng } from './intro.ts';
import {lifeStates,scenePoints,scenes} from './scenes.ts';
import {fixture} from './test-fixture.ts';
import { crc32, inflateSync } from 'node:zlib';

setCapabilities({images:null,trueColor:true,hyperlinks:false});

test('optional companion download shows progress without blocking drafts or changing the working orb',()=>{
 const v={...fixture(),input:'keep typing',cursor:11,workspaceStatus:'↓ Downloading terminal-mac · 42% · 0.3/0.8 MB'};
 const frame=render(v);
 assert.equal(frame.artwork,null);
 assert.match(stripTerminalSequences(frame.rows[frame.input.y-2]!),/Downloading terminal-mac.*42%/);
 assert.equal(stripTerminalSequences(frame.rows[frame.input.y-1]!).trim(),'');
 assert.notEqual(frame.cursorVisible,false);
 assert.ok(frame.rows.map(stripTerminalSequences).join('').includes('keep typing'));
 const active=render({...v,running:true,phase:'thinking'});
 assert.match(active.rows.slice(active.input.y-4,active.input.y-1).map(stripTerminalSequences).join(''),/Thinking/);
});

test('the approved graphical orb has no coarse fallback drawn underneath it', () => {
  try {
    setCapabilities({images:'kitty',trueColor:true,hyperlinks:false});
    const frame = render({...fixture(), running:true, phase:'thinking', runElapsed:4000});
    const y = frame.input.y - 4;
    assert.ok(frame.images.some(image => image.y === y));
    assert.ok(frame.rows.slice(y, y + 3).every(row => !/[\u2800-\u28ff]/u.test(stripTerminalSequences(row))));
    assert.match(frame.rows.slice(y, y + 3).map(stripTerminalSequences).join(''), /Thinking/);
  } finally {
    setCapabilities({images:null,trueColor:true,hyperlinks:false});
  }
});


test('matches the reference geometry at idle and during conversation', () => {
  const v = fixture(), idle = render(v);
  assert.equal(idle.input.x, 1); assert.equal(idle.input.width, 158);
  assert.equal(idle.input.y + idle.input.height, 42);
  assert.match(stripTerminalSequences(idle.rows[1]!), /osmogrep/);
  v.logs = [{ text: 'USER|Explain the code', level: 'Info' }, { text: 'A paragraph.\n\nAnother paragraph.', level: 'Info' }];
  const active = render(v);
  assert.equal(active.execution.y,idle.execution.y);
  assert.ok(active.rows.map(stripTerminalSequences).join('').includes('osmogrep'));
  assert.deepEqual(idle.input, active.input);
  assert.match(active.rows.map(row => stripTerminalSequences(row).trimEnd()).join('\n'), /A paragraph\.\n\s*\n.*Another paragraph\./);
});

test('the welcome header stays through the first exchange and hides from the second, including after clearing output',()=>{
 const v=fixture();assert.ok(render(v).execution.y>0);
 for(const change of [{running:true},{stream:'Hello'},{logs:[{text:'USER|Hello',level:'Info'}]}]){
  const first=render({...v,...change,userTurns:1});assert.ok(first.execution.y>0);assert.equal(first.artwork,null);
 }
 for(const change of [{running:true},{conversationStarted:true},{stream:'Hello'},{logs:[{text:'USER|Hello',level:'Info'}]}]){
  const frame=render({...v,...change,userTurns:2});assert.equal(frame.execution.y,0);assert.equal(frame.artwork,null);
 }
});

test('update gate hides conversation and input, reports real bytes and keeps retry and restart controls reachable',()=>{
 for(const [width,height] of [[1,1],[40,18],[80,30],[160,44]])for(const light of [true,false])for(const stage of ['', 'downloading','verifying','installing','failed','ready']){
  const frame=render({...fixture(),width:width!,height:height!,theme:light?'light':'dark',logs:[{text:'PRIVATE TRANSCRIPT',level:'Info'}],release:{current:'0.3.4',latest:'0.3.5',notes:['Fresh sessions; restore previous work explicitly.'],selected:1,installing:['downloading','verifying','installing'].includes(stage),progress:{stage,received:500000,total:1000000,detail:stage==='failed'?'Connection lost':null}}});
  const text=frame.rows.map(stripTerminalSequences).join('\n');
  assert.equal(frame.input.height,0);assert.equal(frame.execution.height,0);assert.equal(frame.cursorVisible,false);assert.ok(!text.includes('PRIVATE TRANSCRIPT'));assert.ok(frame.rows.every(row=>visibleWidth(row)<=width!));
  if(width!>=80){if(stage==='downloading')assert.match(text,/50%/);if(stage==='ready')assert.ok(frame.controls?.some(c=>c.id==='update'&&c.index===0));if(stage==='failed')assert.match(text,/Retry update/);}
 }

});

test('startup shows an editable composer immediately when no update is available', () => {
 const frame = render({...fixture(), input:'ready to type', cursor:13});
 assert.ok(frame.input.height > 0);
 assert.ok(frame.cursorVisible !== false);
 const text = frame.rows.map(stripTerminalSequences).join('\n');
 assert.match(text, /ready to type/);
 assert.doesNotMatch(text, /Checking for updates/);
});

test('graphical provider selector uses a bundled small logo and preserves the exact model ID',()=>{
 try{
  setCapabilities({images:'kitty',trueColor:true,hyperlinks:false});
  const view={...fixture(),provider:'openai',model:'gpt-5.2-2025-12-11',conversationStarted:true,statusEntries:[{id:'model',value:'ignored'}]};
  const frame=render(view),control=frame.controls?.find(c=>c.id==='provider');
  assert.ok(control);assert.ok(frame.images.some(image=>image.x===control.x&&image.y===control.y));
  assert.match(frame.rows.map(stripTerminalSequences).join(''),/gpt-5\.2-2025-12-11/);
  assert.ok(!frame.rows.map(stripTerminalSequences).join('').includes('openai'));
 }finally{setCapabilities({images:null,trueColor:true,hyperlinks:false});}
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
test('header preserves the static deployed logo', () => {
  assert.deepEqual(siteLogo,['...iiiii...','.iiiiiiiii.','.ii.....ii.','ii.......ii','ii.......aa','ii.......aa','ii.......ii','.ii.....ii.','.iiiiiiiii.','...iiiii...']);
  assert.ok(websiteMark().every(row=>visibleWidth(row)===7));
  assert.ok(websiteMark().join('').includes('234;88;12'));
});
test('central intro settles into a quiet orbit and yields to drafts and conversation', () => {
  const v=fixture();
  const first=render(v);
  assert.equal(first.animate,true);
  v.elapsed=2500;
  const next=render(v);
  assert.deepEqual(first.rows.slice(1,4),next.rows.slice(1,4));
  assert.notDeepEqual(first.rows.slice(4),next.rows.slice(4));
  assert.ok(next.artwork);
  assert.ok(next.artwork.y >= next.execution.y && next.artwork.y+next.artwork.height < next.input.y);
  v.elapsed=introDuration;
  assert.equal(render(v).animate,true);
  assert.equal(render(v).animationIntervalMs,125);
  assert.ok(render(v).artwork);
  v.elapsed=0;v.input='typing';
  assert.equal(render(v).animate,false);
  assert.equal(render(v).artwork,null);
  v.input='';v.logs=[{text:'hello',level:'Info'}];
  assert.equal(render(v).artwork,null);
  const text=render(v).rows.map(stripTerminalSequences).join('\n');
  assert.ok(!text.includes('Catch the bug.'));
  assert.ok(render(v).rows.every(row=>row.startsWith('\x1b[48;2;0;0;0m')));
});

test('approved scenes morph from the same ring and return to the same quiet pose', () => {
  const signatures=new Set<string>();
  for(const scene of ['horizon','interference','life','lorenz'] as const){
    const start=introFrame(0,60,28,false,scene),concept=introFrame(5000,60,28,false,scene),end=introFrame(introDuration,60,28,false,scene);
    assert.deepEqual(start,end);assert.notDeepEqual(end,introFrame(introDuration+3000,60,28,false,scene));
    assert.notDeepEqual(start,concept);assert.notDeepEqual(concept,introFrame(5500,60,28,false,scene));
    signatures.add(concept.join(''));
    for(const frame of [start,concept,end]){assert.equal(frame.length,28);assert.ok(frame.every(row=>visibleWidth(row)===60));assert.ok(frame.some(row=>/[\u2801-\u28ff]/.test(row)));}
    assert.notDeepEqual(concept,introFrame(5000,60,28,false,scene,true));
  }
  assert.equal(signatures.size,4);
});

test('approved geometry is finite, Life follows real generations, and bright playback returns to a readable smaller mark',()=>{
  assert.equal(lifeStates()[0]!.length,36);assert.ok(lifeStates()[120]!.length>36);
  for(const scene of scenes){assert.ok(Array.from(scenePoints(scene,5,6144)).every(Number.isFinite));}
  const pixels=(png:Buffer)=>{const data:Buffer[]=[];for(let at=8;at<png.length;){const n=png.readUInt32BE(at);if(png.toString('ascii',at+4,at+8)==='IDAT')data.push(png.subarray(at+8,at+8+n));at+=n+12;}return inflateSync(Buffer.concat(data));};
  const quiet=pixels(introPng(5000,60,28)),bright=pixels(introPng(5000,60,28,false,'horizon',true));
  assert.ok(quiet.reduce((a,b)=>Math.max(a,b),0)>140&&quiet.reduce((a,b)=>Math.max(a,b),0)<240);assert.ok(bright.reduce((a,b)=>Math.max(a,b),0)>100);
  assert.deepEqual(introPng(introDuration,60,28,false,'life',true),introPng(0,60,28));
  const v=fixture();v.preferences={status_fields:[],ambient_art:true,reduce_motion:true};assert.equal(render(v).animate,false);
  v.preferences.ambient_art=false;assert.equal(render(v).artwork,null);
});

test('pointer replay uses its own clock and dialogs, images and small terminals hide the artwork', () => {
  const v=fixture();v.elapsed=20_000;
  const settled=render(v);assert.equal(settled.animate,true);
  v.introElapsed=2500;
  assert.equal(render(v).animate,true);
  assert.notDeepEqual(settled.rows,render(v).rows);
  assert.deepEqual(render(v),render(Object.assign({},v,{pointer:{x:90,y:15}})), 'legacy pointer data cannot influence a frame');
  for(const state of [{permission:'approve'},{update:'update'},{voice:'voice'},{masked:true},{input:'[Image #1]'},{width:30},{height:12}]) {
    assert.equal(render({...v,...state}).artwork,null);
  }
});

test('fine intro PNG uses six times the linear dot resolution and valid PNG checksums', () => {
  const png=introPng(5000,60,28), data:Buffer[]=[];
  assert.equal(png.subarray(0,8).toString('hex'),'89504e470d0a1a0a');
  let width=0,height=0;
  for(let at=8;at<png.length;) {
    const length=png.readUInt32BE(at), type=png.toString('ascii',at+4,at+8);
    const payload=png.subarray(at+8,at+8+length);
    assert.equal(crc32(png.subarray(at+4,at+8+length)),png.readUInt32BE(at+8+length));
    if(type==='IHDR'){width=payload.readUInt32BE();height=payload.readUInt32BE(4);}
    if(type==='IDAT') data.push(payload);
    at+=12+length;
  }
  assert.equal(width,60*2*6);assert.equal(height,28*4*6);
  const raw=inflateSync(Buffer.concat(data));
  assert.equal(raw.length,(width*3+1)*height);
  assert.ok(raw.some(value=>value>0));
  for(let y=0;y<height;y++)for(let x=0;x<width;x++)if(x%2||y%2){
    const at=y*(width*3+1)+1+x*3;
    assert.equal(raw[at]!+raw[at+1]!+raw[at+2]!,0,'fine dots retain a black gap rather than merging into a solid surface');
  }
  assert.deepEqual(introPng(0,60,28),introPng(introDuration,60,28));
});
test('orange highlight completes a clockwise orbit without moving ring pixels and reuses cached frames',()=>{
  const centers:number[][]=[],masks:Buffer[]=[];
  for(let quarter=0;quarter<4;quarter++){
    const png=introPng(introDuration+quarter*3000,52,26),chunks:Buffer[]=[];
    assert.equal(introPng(introDuration+quarter*3000+12000,52,26),png,'one bounded cache holds a complete orbit');
    for(let at=8;at<png.length;){const n=png.readUInt32BE(at);if(png.toString('ascii',at+4,at+8)==='IDAT')chunks.push(png.subarray(at+8,at+8+n));at+=n+12;}
    const width=png.readUInt32BE(16),height=png.readUInt32BE(20),raw=inflateSync(Buffer.concat(chunks)),mask=Buffer.alloc(width*height);
    let xSum=0,ySum=0,count=0;
    for(let y=0;y<height;y++)for(let x=0;x<width;x++){
      const at=y*(width*3+1)+1+x*3;mask[y*width+x]=raw[at]!>0?1:0;
      if(raw[at]!>raw[at+1]!*1.5){xSum+=x;ySum+=y;count++;}
    }
    assert.ok(count>0);centers.push([xSum/count-width/2,ySum/count-height/2]);masks.push(mask);
  }
  assert.ok(centers[0]![0]!>50&&centers[1]![1]!>50&&centers[2]![0]!< -50&&centers[3]![1]!< -50);
  for(const mask of masks)assert.deepEqual(mask,masks[0]);
});
test('artwork keeps breathing room above the composer at different terminal sizes',()=>{
 for(const [width,height]of [[80,30],[160,44],[240,80]]){
  const f=render({...fixture(),width:width!,height:height!});assert.ok(f.artwork);
  assert.ok(f.input.y-f.artwork.y-f.artwork.height>=3);
  assert.ok(f.artwork.height<=26&&f.artwork.width<=56);
 }
});

test('Kitty-compatible terminals use the fine intro without changing its reserved bounds', () => {
  setCapabilities({images:'kitty',trueColor:true,hyperlinks:false});
  try {
    const v=fixture(), frame=render(v);
    const artwork=frame.images.filter(image=>!image.placement);
    assert.equal(artwork.length,1);
    assert.ok(artwork[0]!.sequence.includes('\x1b_G'));
    assert.equal(artwork[0]!.x,frame.artwork!.x);
    assert.equal(artwork[0]!.y,frame.artwork!.y);
    v.input='typing';
    assert.equal(render(v).images.filter(image=>!image.placement).length,0);
  } finally {setCapabilities({images:null,trueColor:true,hyperlinks:false});}
});
test('composer has a continuous padded surface with room around text and footer', () => {
  const v = fixture(), f = render(v);
  assert.equal(f.input.height, 3);
  assert.equal(f.cursor.y, f.input.y + 1);
  assert.equal(f.cursor.x, f.input.x + 4);
  for (let y=f.input.y; y<f.input.y+f.input.height; y++) {
    assert.ok(/\x1b\[48;2;3[2-9];3[2-9];(?:2[8-9]|3[0-9])m/.test(f.rows[y]!));
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
  assert.ok(/\x1b\[7m(?:\x1b\[[0-9;]*m)*b\x1b\[27m/.test(render(v).rows.join('')));
  v.input = ''; v.masked = true;
  assert.ok(!render(v).rows.join('').includes('Try'));
});
test('real thumbnail is emitted inside the composer on supported terminals', () => {
  setCapabilities({ images: 'kitty', trueColor: true, hyperlinks: false });
  const v = fixture(); v.input = 'Look [Image #1]'; v.cursor = 15;
  v.attachments = [{id:1,label:'[Image #1]',thumbnail:'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aJ9sAAAAASUVORK5CYII=',width:1,height:1}];
  const f = render(v);
  const thumbnails=f.images.filter(image=>!image.placement);
  assert.equal(thumbnails.length, 1); assert.ok(thumbnails[0]!.sequence.includes('\x1b_G'));
  assert.ok(thumbnails[0]!.y > f.input.y && thumbnails[0]!.y < f.cursor.y);
  v.input = 'Image removed';
  assert.equal(render(v).images.filter(image=>!image.placement).length, 0);
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
  assert.match(stripTerminalSequences(compact.rows[0]!),/osmogrep/);
  v.density = 'spacious';
  const spacious = render(v);
  assert.match(stripTerminalSequences(spacious.rows[2]!),/osmogrep/);
  assert.ok(spacious.execution.y > compact.execution.y);
});

test('command picker uses a bounded dark card and keeps selection visible while scrolling', () => {
  const v = fixture(); v.input = '/'; v.cursor = 1;
  v.commands = Array.from({length:20}, (_,i) => ({cmd:`/command-${i}`,desc:`Description ${i}`}));
  v.selectedCommand = 14; v.permission = 'Pending approval';
  const f = render(v), text = f.rows.map(stripTerminalSequences).join('\n');
  assert.match(text, /Commands.*15 · 20/);
  assert.match(text, /› \/command-14\s+Description 14/);
  assert.match(text, /↑↓ move.*enter.*select/);
  assert.match(text, /Pending approval/);
  assert.equal(text.match(/Description \d+/g)?.length, 5);
  assert.ok(!text.includes('Description 0'));
  assert.ok(!f.rows.join('').includes('\x1b[7m'));
  const selected = f.rows.find(row => row.includes('Description 14'))!;
  assert.ok(selected.includes('\x1b[38;2;235;155;105m'));
  assert.ok(f.rows.indexOf(selected) < f.input.y - 1);
  v.commands = [{cmd:'/key',desc:'Configure credentials'}];v.selectedCommand = 0;
  assert.match(render(v).rows.map(stripTerminalSequences).join('\n'), /1 · 1/);
});

test('command picker fits small terminals, preserves the editor and supports light theme', () => {
  for (const theme of ['dark','light']) for (const [width,height] of [[1,1],[20,5],[40,12],[60,18],[160,44]]) {
    const v = {...fixture(),width:width!,height:height!,theme,input:'/',cursor:1,selectedCommand:2};
    v.commands = [{cmd:'/help',desc:'Help'}, {cmd:'/日本語-long-command',desc:'Unicode description'}, {cmd:'/key',desc:'Set API key'}];
    const f = render(v);
    assert.ok(f.rows.every(row => visibleWidth(row) === width));
    assert.ok(f.cursor.y >= f.input.y && f.cursor.y < f.input.y+f.input.height);
    if (height! >= 12) assert.match(f.rows.map(stripTerminalSequences).join('\n'), /› \/key/);
    if (theme === 'light' && height === 44) assert.ok(f.rows.join('').includes('\x1b[48;2;240;240;240m'));
  }
});

test('quick actions retain padding while attaching edge to edge to the composer',()=>{
 const v={...fixture(),input:'/',commands:[{cmd:'/skills',desc:'A long description '.repeat(20)}]},frame=render(v),surface='\x1b[48;2;22;22;22m',indices=frame.rows.map((r,i)=>r.includes(surface)?i:-1).filter(i=>i>=0),card=indices.map(i=>frame.rows[i]!);
 assert.ok(card.length>5);assert.equal(stripTerminalSequences(card[0]!).trim(),'');assert.ok(card.slice(-2).every(r=>stripTerminalSequences(r).trim()===''));
 for(const row of card){const content=stripTerminalSequences(row).slice(frame.input.x,frame.input.x+frame.input.width);assert.ok(stripTerminalSequences(content).startsWith('  '));assert.ok(stripTerminalSequences(content).endsWith('   '));}
 assert.ok(indices[0]!>frame.execution.y);assert.ok(indices.at(-1)===frame.input.y-1);
});

test('workspace drawer keeps focus and actions inside narrow and wide terminals',()=>{
 for(const [width,height] of [[1,1],[25,12],[80,30],[160,44]])for(const editing of [false,true]){
   const frame=render({...fixture(),width:width!,height:height!,experience:{page:'queue',tabs:['Find','Context','Queue','Changes','Retry','Pins','Layout','Timing'],query:'edit 🦀\nsecond line',cursor:8,selected:1,rows:Array.from({length:120},(_,i)=>({id:`row:${i}`,label:`Prompt ${i}`,detail:'Full content remains available',kind:'queued'})),preview:Array.from({length:200},(_,i)=>`full output line ${i}`).join('\n'),preview_offset:9999,notice:'Queue paused while editing',buttons:[{id:'resume',label:'Resume queue'}],busy:true,editing}});
   assert.equal(frame.rows.length,height);assert.ok(frame.rows.every(r=>visibleWidth(r)<=width!));assert.ok(frame.cursor.x<width!&&frame.cursor.y<height!);
   assert.ok(frame.controls?.every(c=>c.x>=0&&c.y>=0&&c.x+c.width<=width!&&c.y<height!));assert.ok(frame.workspaceScroll!.offset<=frame.workspaceScroll!.requested);assert.equal(frame.transcript,undefined);
 }
});
test('queue controls steer or queue drafts explicitly and hide during credential entry',()=>{
 const v={...fixture(),input:'follow this up',running:true};const frame=render(v);
 assert.ok(frame.controls?.some(c=>c.label==='send-next'));assert.ok(frame.controls?.some(c=>c.label==='steer-draft'));
 assert.ok(!render({...v,masked:true}).controls?.some(c=>c.id==='workspace'));assert.ok(render({...fixture(),queuedMessages:2}).controls?.some(c=>c.label==='open:queue'));
});
test('source errors offer guarded actions and search jumps acknowledge their log target',()=>{
 const jumped=render({...fixture(),followTail:false,logs:Array.from({length:80},(_,i)=>({text:`USER|request ${i}`,level:'Info'})),jumpLog:10});
 assert.equal(jumped.jumpedLog,10);assert.ok(jumped.transcript!.start<jumped.transcript!.maxScroll);
 const error=render({...fixture(),logs:[{text:'src/main.rs:24:3: error: invalid type',level:'Error'}]});assert.ok(error.controls?.some(c=>c.id==='source'&&JSON.parse(c.label!).path==='src/main.rs'));
});
