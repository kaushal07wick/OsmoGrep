import {test} from 'node:test';
import assert from 'node:assert/strict';
import {setCapabilities,stripTerminalSequences,visibleWidth} from '@earendil-works/pi-tui';
import {accountUsage,drawAccount,initials} from './account.ts';
import {render} from './renderer.ts';
import {fixture} from './test-fixture.ts';
import type {PanelView} from './panels.ts';
const now=Date.parse('2026-11-18T12:00:00Z');
const history=Array.from({length:5},(_,i)=>({day:'2026-11-'+(14+i),model:'model-'+i,provider:i%2?'local':'hosted',input:1000+i*100,output:100,cached:500,calls:10+i,duration_ms:3000}));
function panel():PanelView{return {sections:['Account'],section:22,query:'',selected:0,editing:false,notice:'Account connected',footer:'',elapsed:1000,identity:{id:'fixture',name:'Alex Lee',username:'alex'},account:{workspace:{name:'Personal',kind:'personal'},stats:{reviews:12,verified:7,running:0,queued:0},usage:{input_tokens:null,output_tokens:null,model_calls:null}},history,rows:[...['This Mac','Hosted','All models','Previous','More'].map((label,i)=>({id:['account.local','account.hosted','account.models','account.up','account.down'][i],label,description:'',value:''})),...['Switch account','Refresh','Sign out'].map(label=>({label,description:'',value:''}))]};}
const text=(frame:{rows:string[]})=>frame.rows.map(stripTerminalSequences).join('\n');

test('account usage combines models, keeps cache a subset and uses the UTC calendar month',()=>{
 const usage=accountUsage([...history,{...history[0]!,day:'2026-10-31',input:99999},{...history[0]!,day:'2026-11-19',input:99999}],now);
 assert.equal(usage.input,6000);assert.equal(usage.output,500);assert.equal(usage.cached,2500);assert.equal(usage.calls,60);
 assert.deepEqual(usage.week,[0,0,10,11,12,13,14]);assert.equal(usage.models.length,5);assert.equal(usage.models[0]!.model,'model-4');
 assert.deepEqual(accountUsage(history,Date.parse('2026-11-01T12:00:00Z')).week,[0]);
});
test('approved account has a dithered chart, a cache ring and compact model bars with real actions',()=>{
 const frame=drawAccount(panel(),100,46,false,'',false,false,now),all=text(frame);
 for(const label of ['Alex Lee','@alex','Reviews','Verified fixes','6.5K tokens','42%','model-4','All 5 models'])assert.ok(all.includes(label),label);
 assert.match(all,/[\u2801-\u28ff]/);assert.ok(!all.includes('model-0'));
 for(const label of ['account.local','account.hosted','account.models','Switch account','Refresh','Sign out'])assert.ok(frame.controls.some(c=>c.label===label),label);
 assert.ok(!frame.controls.some(c=>c.label==='account.down'),'full approved layout fits without scrolling');
 assert.equal(initials('Alex Lee'),'AL');assert.ok(all.includes('AL'),'initials replace a portrait on text-only terminals');
});
test('account layouts, click targets and portrait bounds fit tiny, split and wide terminals',()=>{
 for(const [width,height] of [[1,1],[12,8],[38,24],[48,32],[60,36],[80,40],[100,46],[160,50]])for(const light of [true,false])for(const hosted of [true,false])for(const scroll of [0,512]){
  const frame=drawAccount({...panel(),account_hosted:hosted,account_scroll:scroll,account_expanded:true,identity:{...panel().identity!,avatar:'fixture-image'}},width!,height!,light,'',true,true,now);
  assert.equal(frame.rows.length,height);assert.ok(frame.rows.every(row=>visibleWidth(row)<=width!));
  for(const c of frame.controls){assert.ok(c.x>=0&&c.y>=0);assert.ok(c.x+c.width<=width!&&c.y+c.height<=height!);}
  if(frame.avatar){const a=frame.avatar;assert.ok(a.x>=0&&a.y>=0&&a.x+a.width<=width!&&a.y+a.height<=height!);}
 }
});
test('charts animate once, settle, respect reduced motion, and expansion and scrolling keep every model reachable',()=>{
 const p=panel(),entry=drawAccount({...p,elapsed:0},100,46,false,'',false,false,now),mid=drawAccount({...p,elapsed:400},100,46,false,'',false,false,now),settled=drawAccount(p,100,46,false,'',false,false,now);
 assert.notDeepEqual(entry.rows,mid.rows);assert.notDeepEqual(mid.rows,settled.rows);assert.ok(entry.animate);assert.ok(!settled.animate);
 assert.deepEqual(settled.rows,drawAccount({...p,elapsed:5000},100,46,false,'',false,false,now).rows);
 assert.deepEqual(settled.rows,drawAccount({...p,elapsed:0},100,46,false,'',true,false,now).rows);
 const expanded=drawAccount({...p,account_expanded:true,account_scroll:512},80,40,false,'',true,false,now);
 assert.ok(expanded.scroll!.offset<expanded.scroll!.requested);
 const up=drawAccount({...p,account_expanded:true,account_scroll:expanded.scroll!.offset-1},80,40,false,'',true,false,now);
 assert.equal(up.scroll!.offset,expanded.scroll!.offset-1);
 assert.match(text(expanded),/model-0/);assert.match(text(expanded),/Fewer models/);assert.ok(expanded.controls.some(c=>c.label==='account.up'));
});
test('missing hosted metrics and unreadable local history remain distinct from actual zero usage',()=>{
 const hosted=text(drawAccount({...panel(),account_hosted:true},100,46,false,'',true,false,now));
 assert.match(hosted,/No hosted usage reported/);assert.doesNotMatch(hosted,/6\.5K|model-4|0 tokens/);
 const failed=text(drawAccount({...panel(),history:[],account_usage_error:'Usage history unavailable: unreadable store'},100,46,false,'',true,false,now));
 assert.match(failed,/could not be read/);assert.doesNotMatch(failed,/0 tokens|No recorded calls/);
 assert.match(text(drawAccount({...panel(),history:[]},100,46,false,'',true,false,now)),/No recorded calls this month/);
});
test('welcome identity stays next to the logo without an avatar or indexing badge, and disappears after turn two',()=>{
 setCapabilities({images:null,trueColor:true,hyperlinks:false});
 const v={...fixture(),account:panel().identity!,input:'draft',cursor:5},first=render(v),top=first.rows.slice(0,5).map(stripTerminalSequences).join('\n');
 assert.match(top,/osmogrep/);assert.match(top,/Alex Lee/);assert.match(top,/~\/project/);assert.doesNotMatch(top,/indexed|indexing|vtest|test-model/);
 assert.ok(!first.images.some(i=>i.id===((process.pid*128+117)>>>0)));
 const second=render({...v,userTurns:2});assert.equal(second.execution.y,0);assert.ok(!second.rows.map(stripTerminalSequences).join('').includes('Alex Lee'));
});


test('completed onboarding keeps Continue to chat and model setup clickable',()=>{
 const p={...panel(),title:'Welcome to OsmoGrep',rows:[...panel().rows,{label:'Continue to chat',description:'',value:''},{label:'Choose a local model or API key',description:'',value:''}]};
 const frame=drawAccount(p,100,46,false,'',true,false,now);
 assert.ok(frame.controls.some(c=>c.label==='Continue to chat'));assert.ok(frame.controls.some(c=>c.label==='Choose a local model or API key'));
});
test('native portrait belongs only to the Account panel and text fallback is restored when graphics are unavailable',()=>{
 const avatar='iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aJ9sAAAAASUVORK5CYII=';
 const p={...panel(),identity:{...panel().identity!,avatar}};
 try{
  setCapabilities({images:'kitty',trueColor:true,hyperlinks:false});
  const frame=render({...fixture(),width:100,height:46,panel:p});
  assert.equal(frame.images.length,1);assert.equal(frame.images[0]!.x,46);assert.equal(frame.images[0]!.y,3);
  assert.equal(frame.input.height,0);assert.equal(frame.cursorVisible,false);
  assert.ok(!render(fixture()).images.some(image=>image.id===frame.images[0]!.id),'closing Account removes its portrait placement');
  assert.ok(!text(frame).includes('AL'));
 }finally{setCapabilities({images:null,trueColor:true,hyperlinks:false});}
 assert.match(text(render({...fixture(),width:100,height:46,panel:p})),/AL/);
});
