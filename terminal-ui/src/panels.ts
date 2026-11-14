import { stripTerminalSequences, truncateToWidth, visibleWidth } from '@earendil-works/pi-tui';

export interface PanelView {
  sections: string[]; section: number; query: string; selected: number; editing: boolean;
  rows: {label:string; description:string; value:string}[]; notice:string; footer:string; preview?:string[];
  history?: {day:string; model:string; provider:string; input:number;output:number;cached:number;calls:number;duration_ms:number}[];
}
const reset='\x1b[0m', bold='\x1b[1m';
const safe=(s:string)=>stripTerminalSequences(s).replace(/[\x00-\x1f\x7f-\x9f]/g,' ');
const clip=(s:string,n:number)=>truncateToWidth(s,Math.max(0,n),'');

export function drawPanel(panel:PanelView, width:number, height:number, light:boolean, accent:string): {rows:string[]; cursor:{x:number;y:number}} {
  const fg=light?'\x1b[38;2;32;32;32m':'\x1b[38;2;225;220;210m';
  const dim=light?'\x1b[38;2;105;105;105m':'\x1b[38;2;130;130;128m';
  const surface=light?'\x1b[48;2;237;235;230m':'\x1b[48;2;18;18;18m';
  const selected=light?'\x1b[48;2;220;217;209m':'\x1b[48;2;35;33;30m';
  const pad=width>=40?3:1, inner=Math.max(1,width-pad*2);
  const rows=Array<string>(height).fill(surface+' '.repeat(width)+reset);
  const paint=(y:number,text:string,active=false)=>{
    if(y<0||y>=height)return;
    const line=clip(text,inner),bg=active?selected:surface;
    rows[y]=bg+' '.repeat(Math.min(pad,width))+fg+line.replaceAll(reset,reset+bg+fg)+' '.repeat(Math.max(0,width-pad-visibleWidth(line)))+reset;
  };
  const title=panel.sections[panel.section]??'Workspace';
  paint(1,accent+'◆ '+bold+safe(title)+reset+dim+'   /  WORKSPACE');
  const first=Math.max(0,Math.min(panel.section-1,panel.sections.length-3));
  const tabs=panel.sections.slice(first,first+3).map((s,i)=>i+first===panel.section?accent+bold+s+reset:dim+s+reset).join(dim+'  /  ');
  paint(3,tabs);
  paint(5,(panel.editing?accent+'Edit  ':dim+'Search  ')+fg+safe(panel.query)+(panel.query?'':'_'));
  let listY=7;
  if(panel.section===8 && height>=29 && inner>=32){
    const days=new Map<string,number>();
    for(const d of panel.history??[])days.set(d.day,(days.get(d.day)??0)+d.input+d.output);
    const weeks=Math.min(53,Math.floor((inner-5)/2)),end=new Date();
    end.setUTCHours(0,0,0,0);end.setUTCDate(end.getUTCDate()+6-end.getUTCDay());
    const first=+end-(weeks*7-1)*86400000;
    const max=Math.max(1,...days.values());
    paint(7,dim+'REPORTED TOKENS  /  UTC DAYS');
    for(let y=0;y<7;y++){
      let line=dim+['S','M','T','W','T','F','S'][y]+'  ';
      for(let x=0;x<weeks;x++){
        const date=new Date(first+(x*7+y)*86400000).toISOString().slice(0,10),value=days.get(date)??0;
        const level=value?Math.max(1,Math.ceil(Math.sqrt(value/max)*3)):0;
        const color=light?['130;128;125','160;105;75','170;80;35','170;60;10'][level]:['46;44;42','90;62;43','148;87;47','218;137;76'][level];
        line+=`\x1b[38;2;${color}m`+['· ','░ ','▒ ','▓ '][level];
      }
      paint(9+y,line);
    }
    paint(17,dim+(days.size?`${days.size} active days · ${[...days.values()].reduce((a,b)=>a+b,0).toLocaleString()} tokens`:'No recorded activity yet · new provider responses build this history'));
    listY=19;
  }
  const footerY=Math.max(listY,height-5), count=Math.max(0,footerY-listY);
  const at=Math.max(0,Math.min(panel.selected,panel.rows.length-1));
  const start=Math.max(0,Math.min(at-Math.floor(count/2),panel.rows.length-count));
  const labelWidth=Math.min(32,Math.max(12,Math.floor(inner*.38)));
  for(let i=0;i<count;i++){
    const r=panel.rows[start+i];if(!r)break;
    const active=start+i===at;
    const label=clip(safe(r.label),labelWidth);
    const suffix=inner>=50?' '.repeat(Math.max(1,labelWidth-visibleWidth(label)+2))+(active?fg:dim)+safe(r.value||r.description):'';
    paint(listY+i,(active?accent+'› '+bold:dim+'  ')+label+reset+suffix,active);
  }
  if(!panel.rows.length)paint(listY,dim+'No matches. Try another search.');
  const current=panel.rows[at];
  paint(height-4,panel.section===4?accent+(panel.preview??[]).map(safe).join(' · '):dim+(current?safe(current.description):''));
  paint(height-3,dim+safe(panel.notice));
  paint(height-1,accent+safe(panel.footer));
  return {rows:rows.map(line=>clip(line,width)),cursor:{x:Math.min(width-1,pad+(panel.editing?6:8)+visibleWidth(safe(panel.query))),y:Math.min(height-1,5)}};
}
