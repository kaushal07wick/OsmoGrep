import {visibleWidth} from '@earendil-works/pi-tui';
import {fit,plain} from './chrome.ts';
import {elapsedText} from './activity.ts';
import type {Day} from './stats.ts';
import type {PanelView,PanelFrame} from './panels.ts';
export interface AccountIdentity {id:string;name:string;username?:string|null;workspace?:string|null;avatar?:string|null}
const reset='\x1b[0m',bold='\x1b[1m';
const compact=new Intl.NumberFormat('en',{notation:'compact',maximumFractionDigits:2});
const number=(value:number|null|undefined)=>value==null?'—':compact.format(value);
const date=(at:number)=>new Intl.DateTimeFormat('en',{month:'short',day:'numeric',timeZone:'UTC'}).format(at);
export function initials(name:string){return plain(name).trim().split(/\s+/).slice(0,2).map(p=>Array.from(p)[0]??'').join('').toUpperCase()||'OG';}
export function accountUsage(history:Day[],now:number){
 const start=new Date(now);start.setUTCDate(1);start.setUTCHours(0,0,0,0);
 const first=start.toISOString().slice(0,10),today=new Date(now).toISOString().slice(0,10);
 const records=history.filter(row=>row.day>=first&&row.day<=today);
 const sum=(key:'input'|'output'|'cached'|'calls'|'duration_ms')=>records.reduce((n,row)=>n+row[key],0);
 const groups=new Map<string,{model:string;provider:string;tokens:number}>();
 for(const row of records){const key=row.provider+'/'+row.model,item=groups.get(key)??{model:row.model,provider:row.provider,tokens:0};item.tokens+=row.input+row.output;groups.set(key,item);}
 const end=Date.parse(today),dayCount=Math.min(7,Math.round((end-start.getTime())/86400000)+1),week=Array.from({length:dayCount},(_,i)=>{const day=new Date(end-(dayCount-1-i)*86400000).toISOString().slice(0,10);return records.filter(row=>row.day===day).reduce((n,row)=>n+row.calls,0);});
 return {start:start.getTime(),chartStart:end-(dayCount-1)*86400000,input:sum('input'),output:sum('output'),cached:sum('cached'),calls:sum('calls'),wait:sum('duration_ms'),models:[...groups.values()].sort((a,b)=>b.tokens-a.tokens),week};
}
export function drawAccount(panel:PanelView,width:number,height:number,light:boolean,accent:string,reduceMotion:boolean,graphics:boolean,now=Date.now()):PanelFrame {
 const root=light?'\x1b[48;2;250;250;250m':'\x1b[48;2;0;0;0m';
 const fg=light?'\x1b[38;2;28;28;28m':'\x1b[38;2;255;255;255m',dim=light?'\x1b[38;2;110;105;100m':'\x1b[38;2;137;135;128m';
 const orange=light?'\x1b[38;2;170;78;20m':'\x1b[38;2;255;163;102m',ember=light?'\x1b[38;2;170;115;65m':'\x1b[38;2;135;69;30m';
 const lineColor=light?'\x1b[38;2;213;207;199m':'\x1b[38;2;48;42;36m',green=light?'\x1b[38;2;35;115;65m':'\x1b[38;2;148;189;158m';
 const left=Math.floor((width-Math.min(width,120))/2)+(width>=60?4:width>=16?1:0),right=Math.max(left+1,width-left),inner=right-left;
 type Piece={x:number;text:string;color:string;action?:string};
 const grid:Piece[][]=Array.from({length:height},()=>[]),controls:PanelFrame['controls']=[];
 const put=(rows:Piece[][],y:number,x:number,text:string,color=fg,action?:string)=>{if(y>=0&&y<rows.length&&x>=0&&x<right)rows[y]!.push({x,text:plain(text),color,action});};
 const center=(y:number,text:string,color=fg)=>put(grid,y,Math.max(left,Math.floor((width-visibleWidth(plain(text)))/2)),text,color);
 const full=height>=36&&width>=60,portrait=height>=28&&width>=30;
 const avatar=portrait&&panel.identity?{x:Math.floor(width/2)-4,y:3,width:8,height:4}:null;
 put(grid,Math.min(1,height-1),left,panel.title==='Welcome to OsmoGrep'?'Welcome to OsmoGrep':'Account',orange+bold);
 if(panel.rows.some(row=>row.label==='Refresh'))put(grid,1,right-9,'↻ Refresh',dim,'Refresh');
 const heroY=portrait?8:3;
 if(panel.identity){
  if(avatar&&!(graphics&&panel.identity.avatar))center(4,initials(panel.identity.name),orange+bold);
  center(heroY,panel.identity.name,fg+bold);
  if(panel.identity.username)center(heroY+1,'@'+panel.identity.username,dim);
  center(heroY+2,'● '+(panel.account?.workspace?.kind==='team'?panel.account.workspace.name:'Personal'),green);
 }else{
  center(heroY,panel.account_code?'One step to connect.':'Your workspace, connected.',fg+bold);
  center(heroY+2,panel.account_code??'Use your GitHub-linked account.',panel.account_code?orange:dim);
 }
 const headerEnd=full?17:portrait?14:8;
 if(panel.account){
  const stats=panel.account.stats,metrics=[[stats?.reviews,'Reviews'],[stats?.verified,'Verified fixes'],[stats?.running,'Running'],[stats?.queued,'Queued']] as const;
  if(full){const col=inner/4;metrics.forEach(([value,label],i)=>{const x=Math.round(left+col*(i+.5)),text=number(value);put(grid,12,x-Math.floor(text.length/2),text,fg+bold);put(grid,13,x-Math.floor(label.length/2),label,dim);});}
  else center(headerEnd-3,metrics.map(([value,label])=>number(value)+' '+label.toLowerCase()).join(' · '),dim);
 }
 put(grid,headerEnd-2,left,'─'.repeat(inner),lineColor);
 const body:Piece[][]=[];
 const line=(text='',color=fg)=>{body.push([]);put(body,body.length-1,left,text,color);return body.length-1;};
 if(panel.account){
  let y=line('Usage',fg+bold);
  put(body,y,Math.max(left+6,right-22),'This Mac',panel.account_hosted?dim:orange,'account.local');
  put(body,y,Math.max(left+17,right-9),'Hosted',panel.account_hosted?orange:dim,'account.hosted');
  line();
  if(panel.account_hosted){
   const usage=panel.account.usage;
   line(usage?.input_tokens!=null&&usage?.output_tokens!=null?number(usage.input_tokens+usage.output_tokens)+' hosted tokens':[usage?.input_tokens,usage?.output_tokens,usage?.model_calls].some(v=>v!=null)?'Hosted usage':'No hosted usage reported',fg+bold);
   if(usage?.period_start)line(date(Date.parse(usage.period_start))+(usage.period_end?' – '+date(Date.parse(usage.period_end)-86400000):''),dim);
   line();
   for(const [label,value] of [['Input',usage?.input_tokens],['Output',usage?.output_tokens],['Calls',usage?.model_calls]] as const)line(number(value)+'  '+label,dim);
  }else if(panel.account_usage_error){line('Local usage could not be read.',orange);line(panel.account_usage_error,dim);}
  else{
   const usage=accountUsage(panel.history??[],now),all=usage.input+usage.output;
   y=line(number(all)+' tokens',fg+bold);put(body,y,right-18,date(usage.start)+' – '+date(now),dim);line();
   const chartWidth=width>=60?Math.floor(inner*.66):inner,ringWidth=inner-chartWidth-4,ringX=left+chartWidth+4,chartHeight=full?8:4;
   y=line('Calls · '+usage.week.length+' '+(usage.week.length===1?'day':'days'),dim);if(width>=60)put(body,y,ringX,'Cached input',dim);
   const chartStart=body.length;for(let row=0;row<chartHeight;row++)line();
   const progress=reduceMotion?1:Math.max(0,Math.min(1,(panel.elapsed??1000)/1000)),reveal=1-Math.pow(1-progress,3),ratio=usage.input?usage.cached/usage.input:0,peak=Math.max(1,...usage.week);
   const chart=(x:number,cells:number,sample:(px:number,py:number)=>string|null)=>{
    const bits=[[1,8],[2,16],[4,32],[64,128]] as const;
    for(let row=0;row<chartHeight;row++)for(let cell=0;cell<cells;cell++){
     let mask=0,color=ember;
     for(let dy=0;dy<4;dy++)for(let dx=0;dx<2;dx++){const ink=sample(cell*2+dx,row*4+dy);if(ink){mask|=bits[dy]![dx]!;if(ink===orange||color!==orange)color=ink;}}
     if(mask)put(body,chartStart+row,x+cell,String.fromCharCode(0x2800+mask),color);
    }
   };
   chart(left,chartWidth,(px,py)=>{
    const at=px/Math.max(1,chartWidth*2-1)*(usage.week.length-1),index=Math.floor(at),fraction=at-index,value=usage.week[index]!*(1-fraction)+usage.week[Math.min(index+1,usage.week.length-1)]!*fraction;
    const top=chartHeight*4-1-value/peak*(chartHeight*4-3)*reveal;
    if(px/(chartWidth*2)>reveal||py<top||value===0)return null;
    if(py-top<1.3)return orange;
    const threshold=((px%4)*3+(py%4)*5)%16;
    return threshold<Math.max(2,11-(py-top)*.3)?(threshold<3?orange:ember):null;
   });
   if(width>=60){
    chart(ringX,ringWidth,(px,py)=>{
     const dx=px-ringWidth+.5,dy=py-chartHeight*2+.5,r=Math.hypot(dx,dy),outer=Math.min(chartHeight*2-1,ringWidth-1);
     if(r>outer||r<outer-3)return null;
     const angle=(Math.atan2(dy,dx)+Math.PI/2+Math.PI*2)%(Math.PI*2)/(Math.PI*2);
     if(angle>ratio*reveal)return (px+py)%3===0?lineColor:null;
     return (px*11+py*7)%7<5?orange:ember;
    });
    const percent=usage.input?Math.round(ratio*100)+'%':'—';put(body,chartStart+Math.floor(chartHeight/2)-1,ringX+Math.floor((ringWidth-percent.length)/2),percent,fg+bold);
   }
   y=line(date(usage.chartStart),dim);put(body,y,left+chartWidth-6,date(now),dim);line();
   const metrics=[[number(usage.input),'Input'],[number(usage.output),'Output'],[number(usage.calls),'Calls'],[elapsedText(usage.wait,true),'Provider wait']];
   const cols=width>=60?4:2,col=Math.floor(inner/cols);
   for(let i=0;i<metrics.length;i+=cols){y=line();line();metrics.slice(i,i+cols).forEach(([value,label],j)=>{put(body,y,left+j*col,value!,fg+bold);put(body,y+1,left+j*col,label!,dim);});}
   line();line('Models',dim);
   const barX=left+Math.floor(inner*.43),barWidth=Math.max(1,inner-Math.floor(inner*.43)-12);
   for(const model of usage.models.slice(0,panel.account_expanded?usage.models.length:3)){
    const fill=all?Math.max(1,Math.round(model.tokens/all*barWidth)):0;
    y=line(model.model+(usage.models.some(other=>other!==model&&other.model===model.model)?' · '+model.provider:''));put(body,y,barX,'⣿'.repeat(fill),model===usage.models[0]?orange:ember);put(body,y,barX+fill,'⠂'.repeat(barWidth-fill),lineColor);put(body,y,right-8,number(model.tokens));
   }
   if(!usage.models.length)line('No recorded calls this month.',dim);
   if(usage.models.length>3){y=line();put(body,y,left,panel.account_expanded?'↑ Fewer models':'↓ All '+usage.models.length+' models',orange,'account.models');}
  }
 }else{
  line(panel.account_code?'Confirm this code in your browser.':panel.notice,dim);line();
  for(const row of panel.rows){line();put(body,body.length-1,left,row.label,orange,row.id??row.label);}
 }
 if(panel.account_running){line();line((reduceMotion?'◌':['◐','◓','◑','◒'][Math.floor((panel.elapsed??0)/150)%4])+' '+(panel.account_code?'Waiting for approval…':'Connecting…'),orange);}
 else if(panel.account&&panel.notice&&!['Account connected','Not signed in'].includes(panel.notice)&&!panel.notice.startsWith('Workspace settings')){line();line(panel.notice,orange);}
 const bodyTop=Math.min(headerEnd,Math.max(0,height-6)),bodyHeight=Math.max(0,height-bodyTop-5),maxScroll=Math.max(0,body.length-bodyHeight),scroll=Math.min(maxScroll,panel.account_scroll??0);
 for(let i=0;i<bodyHeight;i++)grid[bodyTop+i]=body[scroll+i]??[];
 put(grid,height-4,left,'─'.repeat(inner),lineColor);
 if(scroll>0)put(grid,height-4,left,' ↑ Previous ',dim,'account.up');
 if(scroll<maxScroll)put(grid,height-4,right-8,' ↓ more ',dim,'account.down');
 if(panel.account){
  if(panel.title==='Welcome to OsmoGrep'){put(grid,height-2,left,'Continue to chat',orange,'Continue to chat');put(grid,height-2,left+22,'Choose model',dim,'Choose a local model or API key');}
  else{put(grid,height-2,left,'Switch account',orange,'Switch account');put(grid,height-2,left+20,'Sign out',dim,'Sign out');}
 }
 put(grid,height-2,right-8,'esc back',dim);
 const rows=grid.map((pieces,y)=>{
  pieces.sort((a,b)=>a.x-b.x);let row=root,x=0;
  pieces.forEach((p,i)=>{
   const text=fit(p.text,Math.max(0,(pieces[i+1]?.x??right)-p.x)),n=visibleWidth(text);
   row+=' '.repeat(Math.max(0,p.x-x))+p.color;
   const index=p.action?panel.rows.findIndex(r=>(r.id??r.label)===p.action):-1;
   if(index>=0&&n>0){if(index===panel.selected)row+='\x1b[4m';controls.push({id:'choice',index,label:p.action!,x:p.x,y,width:n,height:1});}
   row+=text+reset+root;x=p.x+n;
  });
  return fit(row+' '.repeat(Math.max(0,width-x))+reset,width);
 });
 return {rows,controls,scroll:{requested:panel.account_scroll??0,offset:scroll},cursor:{x:0,y:0},avatar:avatar&&graphics&&panel.identity?.avatar?avatar:null,animate:!reduceMotion&&(!!panel.account_running||!!panel.account&&!panel.account_hosted&&!panel.account_usage_error&&(panel.elapsed??1000)<1000)};
}
