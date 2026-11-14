import {fit} from './chrome.ts';
export interface Day {day:string;model:string;provider:string;input:number;output:number;cached:number;calls:number;duration_ms:number}
const dateKey=(date:number)=>new Date(date).toISOString().slice(0,10),reset='\x1b[0m';
export function sampleHistory(now=Date.now()):Day[]{
  const end=Date.parse(dateKey(now));
  return Array.from({length:365},(_,i)=>{
    const day=end-(364-i)*86400000,noise=Math.sin(i*127.1+311.7)*43758.5453,n=noise-Math.floor(noise);
    const intensity=Math.max(.2,Math.sin(i*.077)+Math.cos(i*.027)*.6+.7),tokens=n<.18?0:Math.round(intensity*(.2+n)*12500);
    return {day:dateKey(day),model:'Sample model',provider:'demo',input:Math.round(tokens*.82),output:tokens-Math.round(tokens*.82),cached:Math.round(tokens*.28),calls:tokens?Math.ceil(tokens/1400):0,duration_ms:0};
  });
}
export function weeklyUsage(history:Day[],count:number,now=Date.now()){
  const today=Date.parse(dateKey(now)),monday=today-((new Date(today).getUTCDay()+6)%7)*86400000;
  const weeks=Array.from({length:count},(_,i)=>({start:monday-(count-1-i)*7*86400000,input:0,output:0,calls:0}));
  for(const day of history){const index=Math.floor((Date.parse(day.day)-weeks[0]!.start)/(7*86400000));if(index>=0&&index<weeks.length){weeks[index]!.input+=day.input;weeks[index]!.output+=day.output;weeks[index]!.calls+=day.calls;}}
  return weeks;
}
const resetColor='\x1b[0m';
export function statsLines(history:Day[],width:number,height:number,light:boolean,demo:boolean,now=Date.now(),progress=1):string[]{
  const data=demo?sampleHistory(now):history,days=new Set(data.filter(d=>d.input+d.output>0).map(d=>d.day));
  const total=data.reduce((n,d)=>n+d.input+d.output,0),calls=data.reduce((n,d)=>n+d.calls,0);
  const ink=light?'\x1b[38;2;65;60;54m':'\x1b[38;2;201;192;178m',warm=light?'\x1b[38;2;163;77;26m':'\x1b[38;2;222;147;90m',muted=light?'\x1b[38;2;115;110;104m':'\x1b[38;2;119;113;103m',inputColor=light?'\x1b[38;2;132;123;106m':'\x1b[38;2;122;119;109m';
  const number=(n:number)=>n.toLocaleString('en-US'),compact=(n:number)=>n>=1000000?`${(n/1000000).toFixed(2)}m`:n>=10000?`${(n/1000).toFixed(1)}k`:number(n);
  const count=Math.max(2,Math.min(12,height-10-((demo&&history.length)||(!total&&!demo)?1:0))),weeks=weeklyUsage(data,count,now),totals=weeks.map(w=>w.input+w.output),peak=Math.max(1,...totals),latest=totals.at(-1)!,previous=totals.at(-2)!,average=Math.round(totals.reduce((a,b)=>a+b,0)/count);
  const column=Math.max(1,Math.floor(width/3)),cell=(s:string)=>fit(s,column)+' '.repeat(Math.max(0,column-s.length));
  const lines=[warm+`${demo?'SAMPLE DATA':'RECORDED ACTIVITY'}  /  WEEKLY USAGE`+reset,'',muted+`${compact(total)} tokens · ${days.size} active ${days.size===1?'day':'days'} · ${number(calls)} ${calls===1?'call':'calls'}${demo?' · sample year':' · all recorded'}`+reset,'',
    muted+cell('THIS WEEK')+cell('LAST WEEK')+cell(`AVG / ${count} WKS`)+reset,
    warm+cell(number(latest))+ink+cell(number(previous))+cell(number(average))+reset,''];
  const date=(t:number)=>{const d=new Date(t);return `${String(d.getUTCDate()).padStart(2,'0')} ${'Jan Feb Mar Apr May Jun Jul Aug Sep Oct Nov Dec'.split(' ')[d.getUTCMonth()]}`;};
  const labelWidth=15,valueWidth=width>=60?12:8,barWidth=Math.max(1,width-labelWidth-valueWidth-3);
  for(let i=0;i<weeks.length;i++){
    const t=Math.max(0,Math.min(1,(progress-i*.012)/(1-i*.012))),fill=1-Math.pow(1-t,3);
    const week=weeks[i]!,amount=totals[i]!,full=Math.round(amount/peak*barWidth*2),length=Math.round(full*fill),input=Math.round(week.input/Math.max(1,amount)*full);
    let bar='';for(let x=0;x<barWidth;x++){
      const covered=Math.max(0,Math.min(2,length-x*2)),color=x*2<input?inputColor:warm;
      const pattern=covered===2?[0xf7,0xdf,0x7f,0xfe][(x+i)%4]!:covered===1?0x47:0;
      bar+=pattern?color+String.fromCharCode(0x2800+pattern):' ';
    }
    const label=date(week.start)+'–'+date(week.start+6*86400000),value=width>=60?number(amount):compact(amount);
    lines.push((i===weeks.length-1?warm+'› ':muted+'  ')+fit(label,labelWidth-2)+' '+bar+ink+' '.repeat(Math.max(1,valueWidth-value.length))+value+resetColor);
  }
  lines.push('',muted+'UTC weeks · orange = output · graphite = input'+reset,muted+'Bar length = weekly tokens · one shared scale'+reset);
  if(demo&&history.length)lines.push(ink+`Recorded here: ${number(history.reduce((n,d)=>n+d.input+d.output,0))} tokens · D switches to your data`+reset);
  if(!total&&!demo)lines.push(ink+'No reported usage yet · D opens sample data'+reset);
  return lines.slice(0,Math.max(0,height)).map(line=>fit(line,width));
}
