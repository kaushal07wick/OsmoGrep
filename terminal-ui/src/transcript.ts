import {wrapTextWithAnsi,visibleWidth} from '@earendil-works/pi-tui';
import {plain,fit} from './chrome.ts';
export interface FileDiff {file:string;added?:number;removed?:number;preview?:boolean;lines:{text:string;kind:string;oldLine?:number|null;newLine?:number|null}[]}
const reset='\x1b[0m';
export function diffLines(diff:FileDiff,width:number,light:boolean,expanded:boolean):string[]{
  const dim='\x1b[38;2;126;125;116m',ink=light?'\x1b[38;2;45;45;45m':'\x1b[38;2;205;204;193m';
  const added=diff.added??diff.lines.filter(l=>l.kind==='Added').length,removed=diff.removed??diff.lines.filter(l=>l.kind==='Removed').length;
  const lines=['',ink+plain(diff.file)+reset+dim+`  +${added} −${removed}  /  ${diff.preview?'pending approval':'changed'}`+reset];
  const selected=expanded?diff.lines:diff.lines.slice(0,14),numberWidth=Math.min(6,String(Math.max(1,...diff.lines.map(l=>l.newLine??l.oldLine??0))).length);
  for(const line of selected){
    if(line.kind==='Header'){lines.push(dim+'  '+plain(line.text)+reset);continue;}
    const add=line.kind==='Added',remove=line.kind==='Removed';
    const bg=add?(light?'\x1b[48;2;225;243;226m':'\x1b[48;2;9;30;20m'):remove?(light?'\x1b[48;2;250;226;225m':'\x1b[48;2;39;15;18m'):(light?'\x1b[48;2;243;241;236m':'\x1b[48;2;13;13;13m');
    const color=add?'\x1b[38;2;98;189;122m':remove?'\x1b[38;2;223;122;119m':dim;
    const prefix=String(line.newLine??line.oldLine??'').padStart(numberWidth)+' '+(add?'+':remove?'-':' ')+' ';
    const wrapped=wrapTextWithAnsi(plain(line.text),Math.max(1,width-visibleWidth(prefix)-2));
    wrapped.forEach((text,i)=>{const content=' '+(i?' '.repeat(visibleWidth(prefix)):prefix)+text;lines.push(bg+color+fit(content,width)+' '.repeat(Math.max(0,width-visibleWidth(content)))+reset);});
  }
  if(selected.length<diff.lines.length)lines.push(dim+`  … ${diff.lines.length-selected.length} more lines · ctrl+t expand`+reset);
  return lines.map(line=>fit(line,width));
}
