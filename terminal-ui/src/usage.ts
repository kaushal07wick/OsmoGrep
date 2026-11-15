import {visibleWidth} from '@earendil-works/pi-tui';
import {fit,plain} from './chrome.ts';
import type {Day} from './stats.ts';
export function usageLines(history:Day[],fields:{label:string;value:string}[],width:number,height:number,light:boolean){
 const reset='\x1b[0m',dim=light?'\x1b[38;2;100;105;105m':'\x1b[38;2;135;140;140m',ink=light?'\x1b[38;2;30;35;35m':'\x1b[38;2;225;225;215m';
 const inputColor=light?'\x1b[38;2;65;105;120m':'\x1b[38;2;110;155;170m',outputColor=light?'\x1b[38;2;175;80;25m':'\x1b[38;2;225;150;90m';
 const sum=(key:'input'|'output'|'cached'|'calls'|'duration_ms')=>history.reduce((n,day)=>n+day[key],0),input=sum('input'),output=sum('output'),total=input+output,cached=sum('cached');
 const number=(n:number)=>n.toLocaleString('en-US'),field=(label:string)=>plain(fields.find(row=>row.label===label)?.value??'unavailable');
 const barWidth=Math.max(1,width-2),inputWidth=total?Math.round(input/total*barWidth):0;
 const bar=total?inputColor+'━'.repeat(inputWidth)+outputColor+'━'.repeat(barWidth-inputWidth):dim+'─'.repeat(barWidth);
 const cols=(values:string[])=>{const col=Math.max(1,Math.floor(width/values.length));return values.map((s,i)=>fit(s,col-1)+(i<values.length-1?' '.repeat(Math.max(1,col-visibleWidth(s))):'')).join('');};
 const rows=[dim+'REPORTED TOKENS  /  LAST 365 DAYS',ink+'\x1b[1m'+number(total)+reset+dim+' total', '',
  cols([inputColor+'INPUT',outputColor+'OUTPUT',dim+'CACHED INPUT']),cols([ink+number(input),ink+number(output),ink+number(cached)]),'',bar+reset,
  dim+`${total?Math.round(input/total*100):0}% input · ${total?Math.round(output/total*100):0}% output`, '',
  dim+'CACHE REUSE  '+ink+(input?Math.round(cached/input*100):0)+'%'+dim+' of input',
  dim+'MODEL CALLS  '+ink+number(sum('calls'))+dim+'    WAIT  '+ink+(sum('duration_ms')/1000).toFixed(1)+'s','',
  dim+'CURRENT CONTEXT  '+ink+field('Context estimate'),dim+'Estimate · context capacity not supplied','',dim+'Pricing / quotas unavailable'];
 if(height<rows.length){rows.splice(13,1);rows.splice(11,1);rows.splice(8,1);rows.splice(5,1);rows.splice(2,1);}
 return rows.slice(0,Math.max(0,height)).map(line=>fit(line,width)+reset);
}
