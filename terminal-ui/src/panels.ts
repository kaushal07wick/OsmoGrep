import {visibleWidth,wrapTextWithAnsi} from '@earendil-works/pi-tui';
import {plain,fit} from './chrome.ts';
import {statsLines,type Day} from './stats.ts';
import {drawCenter} from './center.ts';
import {usageLines} from './usage.ts';
import {websiteMark} from './brand.ts';
export interface PanelView {
  health_running?:boolean;account_running?:boolean;
  settings?:boolean;previewing?:boolean; preview_offset?:number;
  title?:string;edit_label?:string;elapsed?:number;selecting?:boolean;center_filter?:number;counts?:number[];
  sections:string[];section:number;query:string;selected:number;editing:boolean;
  rows:{label:string;description:string;value:string;id?:string|null;group?:string|null;tag?:string|null}[];notice:string;footer:string;preview?:string[];history?:Day[];demo?:boolean;
}
const reset='\x1b[0m',bold='\x1b[1m';
export function drawPanel(panel:PanelView,width:number,height:number,light:boolean,accent:string,reduceMotion=false){
  if(panel.section===10)return drawCenter(panel,width,height,light,accent,reduceMotion);
  const root=light?'\x1b[48;2;250;250;250m':'\x1b[48;2;0;0;0m',surface=light?'\x1b[48;2;237;235;230m':'\x1b[48;2;18;18;18m';
  const fg=light?'\x1b[38;2;32;32;32m':'\x1b[38;2;225;220;210m',dim=light?'\x1b[38;2;105;105;105m':'\x1b[38;2;130;130;128m';
  const gutter=width>=60?3:width>=24?1:0,card=Math.max(1,width-2*gutter),pad=card>=40?3:1,inner=Math.max(1,card-2*pad),top=height>=14?2:0,available=Math.max(1,height-top-(height>=14?2:0));
  const wrap=(s:string,n=2)=>wrapTextWithAnsi(s,inner).slice(0,n),rows=Array<string>(height).fill(root+' '.repeat(width)+reset);
  const controls:{id:'choice';index:number;label:string;x:number;y:number;width:number;height:number}[]=[];
  if (([11,14].includes(panel.section)||panel.section>=15) && panel.previewing) {
    const put = (y:number, text:string) => {
      if (y >= 0 && y < height) {
        const content = ' '.repeat(gutter + pad) + fit(text, inner);
        rows[y] = root + content + root + ' '.repeat(Math.max(0, width-visibleWidth(content))) + reset;
      }
    };
    put(top, accent + (panel.section>=15?(panel.title??'Lab report'):panel.section===14?'Diagnostic report':'Team message') + reset);
    const bodyTop = top + 2;
    const bodyHeight = Math.max(0, height - bodyTop - 4);
    const source = (panel.preview ?? []).flatMap(line => wrapTextWithAnsi(plain(line), inner));
    const start = Math.min(panel.preview_offset ?? 0, Math.max(0, source.length - bodyHeight));
    source.slice(start, start + bodyHeight).forEach((line, i) => put(bodyTop + i, fg + line));
    const y = Math.max(0, height - 2);
    put(y, accent + 'Show less' + reset);
    controls.push({id:'choice',index:0,label:'Show less',x:gutter+pad,y,width:Math.min(9, inner),height:1});
    return {rows,controls,cursor:{x:0,y:0}};
  }
  const at=Math.max(0,Math.min(panel.selected,panel.rows.length-1)),current=panel.rows[at];
  const tabs=panel.sections.slice(Math.max(0,Math.min(panel.section-1,panel.sections.length-3)),Math.max(0,Math.min(panel.section-1,panel.sections.length-3))+3).map(s=>s===panel.sections[panel.section]?accent+bold+s+reset:dim+s+reset).join(dim+'  /  ');
  const header=panel.section===12&&available>=14?['',accent+'◌ '+bold+'Settings'+reset,'',dim+'Choose a category · enter or click',dim+'Search  '+fg+plain(panel.query),''] :available>=14?['',accent+'◌ '+bold+plain(panel.title??panel.sections[panel.section]??'Workspace')+reset+dim+'   /  WORKSPACE','',panel.settings?dim+(panel.section===12?'Choose a category · enter or click to open':'Settings / '+plain(panel.sections[panel.section]??'')+' · esc categories'):inner>=48?tabs:dim+'tab / shift+tab · sections','',...([7,8].includes(panel.section)?[]:[(panel.editing?accent+'Edit  ':dim+'Search  ')+fg+plain(panel.query)+(panel.query?'':'_'),''])]:[accent+plain(panel.title??panel.sections[panel.section]??'Workspace')];
  const detail=[7,8].includes(panel.section)?'':panel.section===4?(panel.preview??[]).map(plain).join(' · '):plain(current?(inner<60?current.label+' · ':'')+current.description:'');
  if((panel.section===14||panel.section===22)&&available>=14){
    const busy=panel.section===14?panel.health_running:panel.account_running;
    const pulse=busy?(reduceMotion?'◌ ':['◐ ','◓ ','◑ ','◒ '][Math.floor((panel.elapsed??0)/150)%4]):panel.section===14?'✓ ':'';
    header.splice(4,2,...wrap((busy?accent:fg)+pulse+plain(panel.notice)), '');
  }
  const notice=panel.section===7?'Provider-reported · this workspace · cached input is included in input':panel.section===8?(panel.demo?'Sample data · preview only · D switches to recorded data':'Recorded provider usage · D previews sample data'):plain(panel.notice);
  const tail=[7,12].includes(panel.section)&&available>=14?['',...wrap(accent+plain(panel.footer)),'','']:available>=14?['',...(detail?wrap(dim+detail):[]),...([14,22].includes(panel.section)?[]:wrap(dim+notice)),'',...wrap(accent+plain(panel.footer)),'','']:[accent+'↑↓ · enter · esc'];
  const count=Math.max(0,available-header.length-tail.length),body:string[]=[];
  if(panel.section===7){body.push(...(panel.notice.startsWith('Usage history unavailable:')?wrap(dim+plain(panel.notice),3):usageLines(panel.history??[],panel.rows,inner,count,light)));}
  else if(panel.section===8){body.push(...statsLines(panel.history??[],inner,count,light,!!panel.demo,Date.now(),reduceMotion?1:Math.max(0,Math.min(1,(panel.elapsed??900)/900))));}
  else{
    const n=Math.min(count,Math.max(1,panel.rows.length)),start=Math.max(0,Math.min(at-Math.floor(n/2),panel.rows.length-n));
    const labelWidth=panel.section===14&&inner<60?Math.max(1,inner-14):inner<60?Math.max(1,inner-4):Math.min(32,Math.floor(inner*.36));
    for(let i=0;i<n;i++){
      const r=panel.rows[start+i];if(!r)break;
      if(panel.selecting)controls.push({id:'choice',index:start+i,label:r.id??r.label,x:gutter+pad,y:top+header.length+i,width:inner,height:1});const active=start+i===at,label=fit(plain(r.label),labelWidth);
      const statusColor=panel.section===14?({ok:light?'\x1b[38;2;30;130;75m':'\x1b[38;2;100;190;130m',warn:light?'\x1b[38;2;155;100;15m':'\x1b[38;2;225;185;75m',fail:'\x1b[38;2;230;100;100m',info:dim,checking:accent}[r.tag??'']??(active?accent:fg)):(active?accent:fg);
      const statusValue=r.tag==='checking'?`${reduceMotion?'◌':['◐','◓','◑','◒'][Math.floor((panel.elapsed??0)/150)%4]} Checking`:r.value;
      const value=fit(plain(statusValue),Math.min(24,Math.floor(inner*.22))),descWidth=Math.max(0,inner-labelWidth-visibleWidth(value)-7);
      const suffix=inner>=60?' '.repeat(Math.max(1,labelWidth-visibleWidth(label)+2))+dim+fit(plain(r.description),descWidth)+' '.repeat(Math.max(1,descWidth-visibleWidth(fit(plain(r.description),descWidth))+2))+statusColor+value:panel.section===14?'  '+statusColor+value:'';
      body.push((active?accent+'▸ '+bold:dim+'  ')+label+reset+suffix);
    }
    if(!panel.rows.length)body.push(dim+(panel.query?'No matching controls.':'Nothing available here yet.'));
  }
  const content=[...header,...body,...tail];
  for(let i=0;i<Math.min(available,content.length);i++){
    const line=fit(content[i]!,inner);
    rows[top+i]=root+' '.repeat(gutter)+surface+fg+' '.repeat(Math.min(pad,card))+line.replaceAll(reset,reset+surface+fg)+' '.repeat(Math.max(0,card-pad-visibleWidth(line)))+reset+root+' '.repeat(gutter)+reset;
  }
  return {controls,rows:rows.map(s=>fit(s,width)),cursor:{x:Math.max(0,Math.min(width-1,gutter+pad+(panel.editing?6:8)+visibleWidth(plain(panel.query)))),y:Math.min(height-1,top+(header.length>=6?(panel.section===12?4:5):0))}};
}

export function permissionLines(tool:string,args:string,selected:number,width:number,accent:string,light:boolean,compact=false):string[]{
  const fg=light?'\x1b[38;2;32;32;32m':'\x1b[38;2;225;220;210m',dim='\x1b[38;2;140;134;124m';
  const wrap=(s:string)=>wrapTextWithAnsi(s,Math.max(1,width-4));
  const labels = ['Y  Allow once','N  Deny','A  Auto-approve this session'];
  const choices = labels.map((label,index) => (selected === index ? accent+'› '+bold : dim+'  ') + label + reset);
  const horizontal = width >= 76;
  return [accent+'  Allow '+fg+plain(tool).replaceAll('_',' '),
    ...wrap(dim+'  '+plain(args)).slice(0, compact ? 1 : 2),
    ...(horizontal ? ['  '+choices.join('   ')] : choices.map(choice=>'  '+choice)),
    dim+'  ↑↓ choose · enter confirm · esc cancel'];
}


export interface ReleaseView {current:string;latest:string;notes:string[];selected:number;installing:boolean;progress?:{stage:string;received:number;total:number|null;detail:string|null}}
export function updateLines(release:ReleaseView,width:number,accent:string,light:boolean,compact=false,elapsed=0):string[]{
 const fg=light?'\x1b[38;2;32;32;32m':'\x1b[38;2;225;220;210m',dim=light?'\x1b[38;2;100;100;95m':'\x1b[38;2;145;140;132m';
 const split=width>=66,leftWidth=14,bodyWidth=Math.max(1,width-(split?leftWidth+4:0));
 const stage=release.progress?.stage??(release.installing?'checksum':'');
 const busy=release.installing,ready=stage==='ready',failed=stage==='failed';
 const body=[fg+bold+'osmogrep'+reset+dim+' / update','',fg+(ready?'Ready for your next conversation.':failed?'Let’s try that again.':'A better workspace is ready.')];
 const notes=release.notes.length?release.notes:['Improvements and fixes in this release.'];
 if(!compact)body.push('',...notes.slice(0,3).flatMap(note=>{
  const wrapped=wrapTextWithAnsi(plain(note.split(';')[0]!),Math.max(1,bodyWidth-2));
  return [accent+'+ '+dim+wrapped[0],...(wrapped.length>1?[dim+'  '+fit(wrapped.slice(1).join(' '),Math.max(1,bodyWidth-2))]:[])];
 }));
 body.push('');
 if(busy){
  const p=release.progress,known=stage==='downloading'&&p?.total!=null&&p.total>0;
  const ratio=known?Math.max(0,Math.min(1,p!.received/p!.total!)):0;
  const barWidth=Math.max(1,Math.min(38,bodyWidth-7)),filled=Math.floor(barWidth*ratio),tick=Math.floor(elapsed/100);
  body.push(accent+({'checksum':'Fetching release checksum','downloading':'Downloading update','verifying':'Verifying checksum','installing':'Installing update'}[stage]??'Preparing update'));
  body.push(known?accent+'━'.repeat(filled)+dim+'─'.repeat(barWidth-filled)+accent+` ${Math.floor(ratio*100)}%`:Array.from({length:barWidth},(_,i)=>Math.abs(i-tick%(barWidth+5)+2)<2?accent+'━':dim+'─').join(''));
  body.push(dim+(known?`${(p!.received/1e6).toFixed(1)} / ${(p!.total!/1e6).toFixed(1)} MB`:stage==='downloading'?`${((p?.received??0)/1e6).toFixed(1)} MB received`:stage==='verifying'?'Checking the downloaded release.':stage==='installing'?'Your conversations stay saved.':'Connecting to GitHub…'));
  if(p?.detail)body.push(dim+fit(plain(p.detail),bodyWidth));
  body.push('',dim+'Chat opens after the update.');
 }else if(ready){
  body.push('\x1b[38;2;130;185;135m✓ Update installed',dim+'Restart to use '+plain(release.latest)+'.','',accent+bold+'› R Restart & open chat'+reset);
 }else{
  if(failed){
   body.push('\x1b[38;2;230;130;120mDownload or installation interrupted.',...wrapTextWithAnsi(dim+plain(release.progress?.detail??'Please retry when your connection is ready.'),bodyWidth).slice(0,2),dim+'The update did not complete.','');
  }
  const labels=[failed?'R Retry update':'U Update now','L Later','S Skip this version'];
  body.push(...labels.map((label,i)=>(release.selected===i?accent+bold+'› ':dim+'  ')+label+reset));
  body.push('',dim+'↑↓ choose · enter confirm · esc later');
 }
 const left=split?[...websiteMark(light),'',fg+bold+plain(release.latest)+reset,dim+'from '+plain(release.current)]:[fg+plain(release.latest)+dim+' · from '+plain(release.current)];
 if(!split)return [...left,'',...body].map(row=>fit(row,width));
 return Array.from({length:Math.max(left.length,body.length)},(_,i)=>{
  const label=fit(left[i]??'',leftWidth);return label+' '.repeat(Math.max(0,leftWidth-visibleWidth(label)))+dim+' │  '+(body[i]??'');
 }).map(row=>fit(row,width));
}
