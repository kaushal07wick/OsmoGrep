import {visibleWidth,wrapTextWithAnsi} from '@earendil-works/pi-tui';
import {plain,fit} from './chrome.ts';
import {statsLines,type Day} from './stats.ts';
export interface PanelView {
  sections:string[];section:number;query:string;selected:number;editing:boolean;
  rows:{label:string;description:string;value:string}[];notice:string;footer:string;preview?:string[];history?:Day[];demo?:boolean;
}
const reset='\x1b[0m',bold='\x1b[1m';
export function drawPanel(panel:PanelView,width:number,height:number,light:boolean,accent:string){
  const root=light?'\x1b[48;2;250;250;250m':'\x1b[48;2;0;0;0m',surface=light?'\x1b[48;2;237;235;230m':'\x1b[48;2;18;18;18m';
  const fg=light?'\x1b[38;2;32;32;32m':'\x1b[38;2;225;220;210m',dim=light?'\x1b[38;2;105;105;105m':'\x1b[38;2;130;130;128m';
  const gutter=width>=60?2:0,card=Math.max(1,width-2*gutter),pad=card>=40?3:1,inner=Math.max(1,card-2*pad),top=height>=14?1:0,available=Math.max(1,height-top-(height>=14?1:0));
  const wrap=(s:string,n=2)=>wrapTextWithAnsi(s,inner).slice(0,n),rows=Array<string>(height).fill(root+' '.repeat(width)+reset);
  const at=Math.max(0,Math.min(panel.selected,panel.rows.length-1)),current=panel.rows[at];
  const tabs=panel.sections.slice(Math.max(0,Math.min(panel.section-1,panel.sections.length-3)),Math.max(0,Math.min(panel.section-1,panel.sections.length-3))+3).map(s=>s===panel.sections[panel.section]?accent+bold+s+reset:dim+s+reset).join(dim+'  /  ');
  const header=available>=14?['',accent+'◆ '+bold+plain(panel.sections[panel.section]??'Workspace')+reset+dim+'   /  WORKSPACE','',inner>=48?tabs:dim+'tab / shift+tab · sections','',...(panel.section===8?[]:[(panel.editing?accent+'Edit  ':dim+'Search  ')+fg+plain(panel.query)+(panel.query?'':'_'),''])]:[accent+plain(panel.sections[panel.section]??'Workspace')];
  const detail=panel.section===4?(panel.preview??[]).map(plain).join(' · '):plain(current?(inner<60?current.label+' · ':'')+current.description:'');
  const notice=panel.section===8?(panel.demo?'Sample data · preview only · D switches to recorded data':'Recorded provider usage · D previews sample data'):plain(panel.notice);
  const tail=available>=14?['',...wrap(dim+detail),...wrap(dim+notice),'',...wrap(accent+plain(panel.footer)),'','']:[accent+'↑↓ · enter · esc'];
  const count=Math.max(0,available-header.length-tail.length),body:string[]=[];
  if(panel.section===8){body.push(...statsLines(panel.history??[],inner,count,light,!!panel.demo));}
  else{
    const n=Math.min(count,Math.max(1,panel.rows.length)),start=Math.max(0,Math.min(at-Math.floor(n/2),panel.rows.length-n));
    const labelWidth=inner<60?Math.max(1,inner-4):Math.min(32,Math.floor(inner*.36));
    for(let i=0;i<n;i++){
      const r=panel.rows[start+i];if(!r)break;const active=start+i===at,label=fit(plain(r.label),labelWidth);
      const value=fit(plain(r.value),Math.min(24,Math.floor(inner*.22))),descWidth=Math.max(0,inner-labelWidth-visibleWidth(value)-7);
      const suffix=inner>=60?' '.repeat(Math.max(1,labelWidth-visibleWidth(label)+2))+dim+fit(plain(r.description),descWidth)+' '.repeat(Math.max(1,descWidth-visibleWidth(fit(plain(r.description),descWidth))+2))+(active?accent:fg)+value:'';
      body.push((active?accent+'▸ '+bold:dim+'  ')+label+reset+suffix);
    }
    if(!panel.rows.length)body.push(dim+(panel.query?'No matching controls.':'Nothing available here yet.'));
  }
  const content=[...header,...body,...tail];
  for(let i=0;i<Math.min(available,content.length);i++){
    const line=fit(content[i]!,inner);
    rows[top+i]=root+' '.repeat(gutter)+surface+fg+' '.repeat(Math.min(pad,card))+line.replaceAll(reset,reset+surface+fg)+' '.repeat(Math.max(0,card-pad-visibleWidth(line)))+reset+root+' '.repeat(gutter)+reset;
  }
  return {rows:rows.map(s=>fit(s,width)),cursor:{x:Math.max(0,Math.min(width-1,gutter+pad+(panel.editing?6:8)+visibleWidth(plain(panel.query)))),y:Math.min(height-1,top+(header.length>=6?5:0))}};
}

export function permissionLines(tool:string,args:string,selected:number,width:number,accent:string,light:boolean):string[]{
  const fg=light?'\x1b[38;2;32;32;32m':'\x1b[38;2;225;220;210m',dim='\x1b[38;2;140;134;124m';
  const wrap=(s:string)=>wrapTextWithAnsi(s,Math.max(1,width-4));
  return ['',accent+'  PERMISSION  /  '+fg+plain(tool),...wrap(dim+'  '+plain(args)).slice(0,2),'',
    ...['Y  Allow once','N  Deny','A  Auto-approve this session'].map((s,i)=>(selected===i?accent+'  ▸ '+bold:dim+'    ')+s+reset),'',dim+'  ↑↓ select · enter confirm · esc cancel',''];
}
