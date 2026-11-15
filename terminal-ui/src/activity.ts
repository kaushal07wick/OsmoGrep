import {rgbPng} from './png.ts';
const cache=new Map<string,{png:Buffer;rows:string[]}>();
const bits=[[1,8],[2,16],[4,32],[64,128]];

/** A rotating dot surface; one cached 36px sphere occupies six by three cells. */
export function orb(elapsed:number,light=false,reduced=false){
 const frame=reduced?0:Math.floor(Math.max(0,elapsed)/125)%32,key=`${frame}:${light}`;
 const cached=cache.get(key);if(cached)return cached;
 const angle=frame*Math.PI/16,bg=light?250:0,raw=Buffer.alloc((36*3+1)*36),dots=new Uint8Array(12*12);
 for(let y=0;y<36;y++){raw[y*109]=0;for(let x=0;x<36;x++){
  const offset=y*109+1+x*3;raw.fill(bg,offset,offset+3);
  const u=(x-17.5)/13.5,v=(y-17.5)/13.5,r=u*u+v*v;if(r>=1||x%2||y%2)continue;
  const z=Math.sqrt(1-r),longitude=Math.atan2(u,z)+angle,latitude=Math.asin(v);
  const ribbons=.55+.45*Math.sin(longitude*5+latitude*3+Math.sin(latitude*4+angle)*.4)**2;
  const lightness=Math.max(.08,(.3+z*.5-u*.24-v*.22)*ribbons);
  const dither=((x*13+y*7)%17)/17;
  if(lightness<.2+dither*.32)continue;
  const shade=light?Math.round(170-lightness*105):Math.round(55+lightness*145);
  raw[offset]=shade;raw[offset+1]=light?shade:Math.max(0,shade-8);raw[offset+2]=light?shade:Math.max(0,shade-19);
  dots[Math.min(11,Math.floor(y/3))*12+Math.min(11,Math.floor(x/3))]=1;
 }}
 const rows=Array.from({length:3},(_,row)=>Array.from({length:6},(_,col)=>{let mask=0;for(let y=0;y<4;y++)for(let x=0;x<2;x++)if(dots[(row*4+y)*12+col*2+x])mask|=bits[y]![x]!;return String.fromCodePoint(0x2800+mask);}).join(''));
 const value={png:rgbPng(36,36,raw),rows};cache.set(key,value);return value;
}
export function activityInfo(elapsed:number,phase:string,tool:string|null,reduced=false){
 const seconds=Math.max(0,Math.floor(elapsed/1000)),time=seconds>=3600?`${Math.floor(seconds/3600)}h ${Math.floor(seconds%3600/60)}m`:seconds>=60?`${Math.floor(seconds/60)}m ${seconds%60}s`:`${seconds}s`;
 const labels:Record<string,string>={list_dir:'Exploring files',read_file:'Reading source',edit_file:'Editing',write_file:'Writing',patch:'Applying a change',run_shell:'Running a command',run_tests:'Running tests',diagnostics:'Checking diagnostics',language_server:'Consulting language server'};
 const thoughts=['Thinking','Working through your request','Considering the next step','Connecting the pieces'];
 const label=phase==='thinking'?thoughts[reduced?0:Math.floor(elapsed/6000)%thoughts.length]!:tool?(labels[tool]??tool.replaceAll('_',' ')):phase==='starting'?'Preparing context':phase.replaceAll('_',' ');
 return {label,time};
}
export function completionLine(text:string,width:number){
 const match=/^· Finished in ([\d.]+)s$/.exec(text);if(!match)return null;
 const label='Done',time=`${match[1]}s elapsed`;return width>=label.length+time.length+3?label+' '.repeat(width-label.length-time.length)+time:`Done · ${match[1]}s`;
}
