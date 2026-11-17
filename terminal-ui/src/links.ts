import {sliceByColumn,stripTerminalSequences,visibleWidth} from '@earendil-works/pi-tui';

export interface WebLink {url:string;x:number;width:number}
export function validWebLink(value:string):boolean {
  if(value.length>4096||/[\s\x00-\x1f\x7f]/u.test(value))return false;
  try{const url=new URL(value);return ['http:','https:'].includes(url.protocol)&&!!url.hostname&&!url.username&&!url.password;}catch{return false;}
}
export function webLinks(row:string,light:boolean):{text:string;links:WebLink[]} {
  const osc=/\x1b\]8;[^;]*;([^\x07\x1b]*)(?:\x07|\x1b\\)/g;
  const links:WebLink[]=[];
  let active:{url:string;x:number}|null=null;
  for(const match of row.matchAll(osc)){
    const x=visibleWidth(row.slice(0,match.index));
    if(active&&x>active.x)links.push({...active,width:x-active.x});
    active=validWebLink(match[1]!)?{url:match[1]!,x}:null;
  }
  if(active&&visibleWidth(row)>active.x)links.push({...active,width:visibleWidth(row)-active.x});
  const text=row.replace(osc,''),plain=stripTerminalSequences(text);
  for(const match of plain.matchAll(/https?:\/\/[^\s<>"\x00-\x1f]+/g)){
    let url=match[0].replace(/[.,;!?\]}]+$/,'');
    while(url.endsWith(')')&&(url.match(/\)/g)?.length??0)>(url.match(/\(/g)?.length??0))url=url.slice(0,-1);
    const x=visibleWidth(plain.slice(0,match.index)),width=visibleWidth(url);
    if(validWebLink(url)&&!links.some(link=>x<link.x+link.width&&x+width>link.x))links.push({url,x,width});
  }
  links.sort((a,b)=>a.x-b.x);
  const blue=light?'\x1b[38;2;38;103;175m':'\x1b[38;2;112;166;224m';
  let painted='',column=0;
  for(const link of links){
    painted+=sliceByColumn(text,column,link.x-column)+blue+'\x1b[4m'+stripTerminalSequences(sliceByColumn(text,link.x,link.width))+'\x1b[24m\x1b[39m';
    column=link.x+link.width;
  }
  return {text:links.length?painted+sliceByColumn(text,column,Math.max(0,visibleWidth(text)-column)):text,links};
}
