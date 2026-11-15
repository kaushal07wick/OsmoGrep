import {visibleWidth} from '@earendil-works/pi-tui';
import type {PanelView} from './panels.ts';
import {plain,fit} from './chrome.ts';

export function drawCenter(panel:PanelView,width:number,height:number,light:boolean,accent:string){
 const reset='\x1b[0m',bold='\x1b[1m',root=light?'\x1b[48;2;250;250;250m':'\x1b[48;2;0;0;0m';
 const ink=light?'\x1b[38;2;35;35;32m':'\x1b[38;2;226;222;211m',dim=light?'\x1b[38;2;113;109;101m':'\x1b[38;2;126;122;113m';
 const selected=light?'\x1b[48;2;232;226;216m':'\x1b[48;2;29;27;23m';
 const margin=width>=70?4:width>=30?2:0,inner=Math.max(1,width-margin*2),top=height>=18?2:0;
 const rows=Array<string>(height).fill(root+' '.repeat(width)+reset),controls:{id:'choice';index:number;label:string;x:number;y:number;width:number;height:number}[]=[];
 const put=(y:number,text:string,bg=root)=>{if(y<0||y>=height)return;const clipped=fit(text,inner);rows[y]=root+' '.repeat(margin)+bg+ink+clipped.replaceAll(reset,reset+bg+ink)+' '.repeat(Math.max(0,inner-visibleWidth(clipped)))+reset+root+' '.repeat(margin)+reset;};
 const at=Math.max(0,Math.min(panel.selected,panel.rows.length-1)),current=panel.rows[at];
 const filters=['All','Needs you','Working','Ready','Inactive'],filter=panel.center_filter??0;
 put(top,accent+'⠿ '+reset+bold+'Sessions'+reset+dim+'  /  OsmoGrep');
 const tabs=filters.map((name,i)=>(i===filter?accent+bold:dim)+name+' '+(panel.counts?.[i]??0)+reset).join(dim+'   ');
 if(height>=10)put(top+2,tabs);
 const searchY=Math.min(height-1,top+(height>=10?4:1));
 put(searchY,(panel.editing?accent+'Message  ':dim+'/ ')+ink+plain(panel.query)+(panel.query?'':dim+(panel.editing?'Write to this collaborator':'Search tasks, models, and projects'))+reset);
 const footerRows=height>=18?5:height>=10?3:1,bodyTop=Math.min(height,searchY+2),bodyHeight=Math.max(0,height-footerRows-bodyTop-(height>=18?1:0));
 if(panel.previewing){
  const lines=(panel.preview??[]).flatMap(line=>{const text=plain(line);return [fit(text,inner)];});
  const offset=Math.max(0,lines.length-bodyHeight-(panel.preview_offset??0));
  lines.slice(offset,offset+bodyHeight).forEach((line,i)=>put(bodyTop+i,ink+line));
  if(!lines.length)put(bodyTop,dim+'No saved output yet. This snapshot refreshes every second.');
  put(height-3,dim+fit(panel.notice,inner));
  put(height-2,dim+'↑/↓ scroll · home/end · F3 fork · F4 message · esc sessions');
  return {rows:rows.map(r=>fit(r,width)),controls,cursor:{x:0,y:0}};
 }
 const items:{index?:number;group?:string}[]=[];let previous='';
 panel.rows.forEach((row,index)=>{const group=row.group??'This workspace';if(group!==previous&&bodyHeight>=4){items.push({group});previous=group;}items.push({index});});
 const focus=items.findIndex(item=>item.index===at),start=Math.max(0,Math.min(focus-Math.floor(bodyHeight/2),items.length-bodyHeight));
 const shown=items.slice(start,start+bodyHeight);
 if(shown[0]?.index!=null&&bodyHeight>=4){shown.unshift({group:panel.rows[shown[0].index]!.group??'This workspace'});shown.length=Math.min(shown.length,bodyHeight);}
 const statusWidth=inner>=80?20:inner>=48?16:0;
 for(let i=0;i<shown.length;i++){
  const item=shown[i]!,y=bodyTop+i;
  if(item.group!=null){const parts=plain(item.group).split('/').filter(Boolean),name=parts.at(-1)??item.group;put(y,dim+fit(`${name}  /  ${parts.slice(-3,-1).join('/')}`,inner)+reset);continue;}
  const index=item.index!,r=panel.rows[index]!,active=index===at;
  const tone=r.tag==='needs_you'||r.tag==='error'?accent:r.tag==='working'?'\x1b[38;2;123;173;137m':dim;
  const marker=r.tag==='working'?'●':r.tag==='needs_you'?'◌':'○';
  const title=fit(plain(r.label),Math.max(1,inner-statusWidth-5));
  const state=plain(r.value).replaceAll('_',' ').replace(/^needs you/,'Needs you').replace(/^working/,'Working').replace(/^ready/,'Ready').replace(/^inactive/,'Inactive');
  const suffix=statusWidth?' '.repeat(Math.max(1,inner-visibleWidth(title)-4-visibleWidth(state)))+(active?ink:dim)+state:'';
  put(y,(active?accent+'› ':dim+'  ')+tone+marker+' '+(active?bold:ink)+title+reset+suffix,active?selected:root);
  controls.push({id:'choice',index,label:r.id??r.label,x:margin,y,width:inner,height:1});
 }
 if(!panel.rows.length&&bodyHeight>0)put(bodyTop,dim+(panel.query?'No matching sessions.':'No sessions yet. F2 starts a conversation here.'));
 const footer=Math.max(bodyTop,height-footerRows);
 if(height>=18){put(footer,dim+fit(current?.description??panel.notice,inner));put(footer+1,dim+fit(panel.notice,inner));}
 const legend=panel.editing?'enter send · esc cancel':inner>=95?'←/→ filter · enter open · F1 preview · F2 new · F3 fork · F4 message · F5 pin · esc back':'←/→ filter · enter open · F2 new · esc back';
 put(height-(height>=10?2:1),dim+legend);
 return {rows:rows.map(r=>fit(r,width)),controls,cursor:{x:Math.min(width-1,margin+(panel.editing?9:2)+visibleWidth(plain(panel.query))),y:searchY}};
}
