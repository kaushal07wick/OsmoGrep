import {encodeKitty} from '@earendil-works/pi-tui';
import {rgbPng} from './png.ts';
import {visibleWidth,truncateToWidth} from '@earendil-works/pi-tui';
const reset='\x1b[0m';
const graphemes=new Intl.Segmenter(undefined,{granularity:'grapheme'});
export function paperColors(light:boolean){return {foreground:light?'\x1b[38;2;48;46;40m':'\x1b[38;2;223;218;205m',muted:light?'\x1b[38;2;121;115;102m':'\x1b[38;2;142;139;125m'};}

/** Text stays text. Graphics use the default cell background so the negative-z paper remains visible. */
export function paperRow(content:string,width:number,row:number,height:number,light:boolean,graphics=false):string {
  const base=light?[220,214,199]:[37,37,33],foreground=paperColors(light).foreground;
  const edge=row===0?(light?4:2):row===height-1?(light?-7:-3):0;
  let inverse=false;
  const bg=()=>graphics&&!inverse?'\x1b[49m':`\x1b[48;2;${base.map(n=>n+edge).join(';')}m`;
  const clipped=truncateToWidth(content,Math.max(1,width),'');let result=foreground,x=0,last='';
  const write=(text:string)=>{for(const {segment}of graphemes.segment(text)){const cells=visibleWidth(segment);if(x+cells>width)break;const color=bg();if(color!==last){result+=color;last=color;}result+=segment;x+=cells;}};
  for(const token of clipped.split(/(\x1b\[[0-9;]*m)/g)){
    if(token.startsWith('\x1b[')){result+=token;if(token==='\x1b[7m')inverse=true;if(token==='\x1b[27m')inverse=false;if(token===reset){inverse=false;result+=foreground;last='';}}
    else write(token);
  }
  write(' '.repeat(Math.max(0,width-x)));
  return result+reset;
}


const textures=new Map<string,string>();
export function paperPng(columns:number,rows:number,light:boolean):string {
  const width=Math.min(4096,Math.max(1,columns*16)),height=Math.min(512,Math.max(1,rows*32)),key=`${width}:${height}:${light}`;
  const cached=textures.get(key);if(cached)return cached;
  const raw=Buffer.alloc((width*3+1)*height),base=light?[220,214,199]:[37,37,33];
  for(let y=0;y<height;y++)for(let x=0;x<width;x++){
    let seed=Math.imul(x+1,374761393)^Math.imul(y+1,668265263);seed=Math.imul(seed^(seed>>>13),1274126177);
    const grain=((seed>>>24)/255-.5)*(light?13:11);
    const fibre=((Math.imul(Math.floor((x+y*.24)/3)+1,92837111)^Math.imul(Math.floor(y/7)+1,689287499))>>>0)%59===0?2:0;
    const edge=y===0?(light?13:13):y===height-1?(light?-23:-12):y<3?3:y>height-4?-3:0;
    const at=y*(width*3+1)+1+x*3;
    for(let c=0;c<3;c++)raw[at+c]=Math.max(0,Math.min(255,Math.round(base[c]!+grain+fibre+edge)));
  }
  const png=rgbPng(width,height,raw).toString('base64');if(textures.size>=4)textures.delete(textures.keys().next().value!);textures.set(key,png);return png;
}
export function paperGraphic(columns:number,rows:number,light:boolean,id:number){
  const sequence=encodeKitty(paperPng(columns,rows,light),{columns,rows,imageId:id,moveCursor:false}).replace('a=T,','a=t,');
  // Below non-default backgrounds: selections and modal surfaces remain above the paper.
  const placement=`\x1b_Ga=p,i=${id},p=1,c=${columns},r=${rows},z=-1073741825,C=1,q=2;\x1b\\`;
  return {id,sequence,placement};
}
