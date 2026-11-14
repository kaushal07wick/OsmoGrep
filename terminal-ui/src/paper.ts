import {visibleWidth,truncateToWidth} from '@earendil-works/pi-tui';
const reset='\x1b[0m';
const graphemes=new Intl.Segmenter(undefined,{granularity:'grapheme'});
export function paperColors(light:boolean){return {foreground:light?'\x1b[38;2;48;46;40m':'\x1b[38;2;223;218;205m',muted:light?'\x1b[38;2;121;115;102m':'\x1b[38;2;142;139;125m'};}

/** Stable cell shading keeps text selectable and never consumes image/clipboard protocols. */
export function paperRow(content:string,width:number,row:number,height:number,light:boolean):string {
  const base=light?[220,214,199]:[37,37,33],foreground=paperColors(light).foreground;
  const edge=row===0?(light?4:2):row===height-1?(light?-7:-3):0;
  const bg=(x:number)=>{let seed=Math.imul(x+1,374761393)^Math.imul(row+1,668265263);seed=Math.imul(seed^(seed>>>13),1274126177);const grain=(seed>>>29)%5-2;return `\x1b[48;2;${base.map(n=>Math.max(0,Math.min(255,n+grain+edge))).join(';')}m`;};
  const clipped=truncateToWidth(content,Math.max(1,width),'');let result=foreground,x=0,last='';
  const write=(text:string)=>{for(const {segment}of graphemes.segment(text)){const cells=visibleWidth(segment);if(x+cells>width)break;const color=bg(x);if(color!==last){result+=color;last=color;}result+=segment;x+=cells;}};
  for(const token of clipped.split(/(\x1b\[[0-9;]*m)/g)){
    if(token.startsWith('\x1b[')){result+=token;if(token===reset){result+=foreground;last='';}}
    else write(token);
  }
  write(' '.repeat(Math.max(0,width-x)));
  return result+reset;
}
