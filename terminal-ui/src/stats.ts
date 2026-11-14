import {fit} from './chrome.ts';
export interface Day {day:string;model:string;provider:string;input:number;output:number;cached:number;calls:number;duration_ms:number}
const dateKey=(date:number)=>new Date(date).toISOString().slice(0,10),reset='\x1b[0m';
export function sampleHistory(now=Date.now()):Day[]{
  const end=Date.parse(dateKey(now));
  return Array.from({length:365},(_,i)=>{
    const day=end-(364-i)*86400000,noise=Math.sin(i*127.1+311.7)*43758.5453,n=noise-Math.floor(noise);
    const intensity=Math.max(0,Math.sin(i*.077)+Math.cos(i*.027)*.6+.7),tokens=n<.18?0:Math.round(intensity*(.2+n)*12500);
    return {day:dateKey(day),model:'Sample model',provider:'demo',input:Math.round(tokens*.82),output:tokens-Math.round(tokens*.82),cached:Math.round(tokens*.28),calls:tokens?Math.ceil(tokens/1400):0,duration_ms:0};
  });
}
export function statsLines(history:Day[],width:number,height:number,light:boolean,demo:boolean,now=Date.now()):string[]{
  const data=demo?sampleHistory(now):history,days=new Map<string,number>();
  for(const d of data)days.set(d.day,(days.get(d.day)??0)+d.input+d.output);
  const total=data.reduce((n,d)=>n+d.input+d.output,0),calls=data.reduce((n,d)=>n+d.calls,0),active=[...days.values()].filter(Boolean).length;
  const ink=light?'\x1b[38;2;65;60;54m':'\x1b[38;2;185;174;158m',warm='\x1b[38;2;222;147;90m',muted=light?'\x1b[38;2;115;110;104m':'\x1b[38;2;100;94;86m';
  const number=(n:number)=>n>=1000000?`${(n/1000000).toFixed(2)}m`:n.toLocaleString('en-US');
  const lines=[warm+`${demo?'SAMPLE DATA  /  PREVIEW':'RECORDED ACTIVITY  /  THIS WORKSPACE'}`+reset,'',ink+`${number(total)} tokens   ${active} active days   ${number(calls)} calls`+reset,''];
  if(height<14){
    const end=Date.parse(dateKey(now)),count=Math.max(1,Math.min(48,width-2));
    const recent=Array.from({length:count},(_,i)=>days.get(dateKey(end-(count-1-i)*86400000))??0),peak=Math.max(1,...recent);
    lines.push(muted+'RECENT DAYS · UTC'+reset);
    for(let row=2;row>=0;row--)lines.push(warm+recent.map(n=>{const fill=n/peak*3-row;return fill>=1?'▓':fill>.6?'▒':fill>.25?'░':'·';}).join('')+reset);
    lines.push(muted+'QUIET · ░ ▒ ▓ BUSY'+reset);
    return lines.slice(0,height).map(line=>fit(line,width));
  }
  const weeks=Math.min(52,Math.max(4,Math.floor((width-4)/2))),end=Date.parse(dateKey(now));
  const sunday=end-new Date(end).getUTCDay()*86400000,start=sunday-(weeks-1)*7*86400000,max=Math.max(1,...days.values());
  let months='    ',previous=-1;
  for(let x=0;x<weeks;x++){const date=new Date(start+x*7*86400000),m=date.getUTCMonth();if(m!==previous){const label=date.toLocaleString('en-US',{month:'short',timeZone:'UTC'});months+=label;previous=m;x++;}else months+='  ';}
  lines.push(muted+fit(months,width)+reset);
  for(let y=0;y<7;y++){
    let line=muted+(['S','M','T','W','T','F','S'][y]??'')+'   ';
    for(let x=0;x<weeks;x++){
      const n=days.get(dateKey(start+(x*7+y)*86400000))??0,level=n?Math.max(1,Math.ceil(Math.sqrt(n/max)*4)):0;
      const rgb=light?['177;169;156','174;139;106','180;115;66','180;90;35','158;62;16'][level]:['44;40;35','79;59;42','123;79;47','177;106;57','231;158;94'][level];
      line+=`\x1b[38;2;${rgb}m`+['· ','⠂ ','░ ','▒ ','▓ '][level];
    }lines.push(line+reset);
  }
  lines.push('',muted+'QUIET  · ⠂ ░ ▒ ▓  BUSY     UTC · input + output'+reset);
  if(height>=21){
    const recent=Array.from({length:Math.min(48,width-4)},(_,i)=>days.get(dateKey(end-(Math.min(48,width-4)-1-i)*86400000))??0),peak=Math.max(1,...recent);
    lines.push('',muted+'RECENT DAYS'+reset);
    for(let row=2;row>=0;row--)lines.push(warm+'    '+recent.map(n=>{const fill=n/peak*3-row;return fill>=1?'▓':fill>.6?'▒':fill>.25?'░':' ';}).join('')+reset);
  }
  if(!total&&!demo)lines.push(ink+'No reported responses in this workspace yet.'+reset);
  return lines.slice(0,height).map(line=>fit(line,width));
}
