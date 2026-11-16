import {visibleWidth,wrapTextWithAnsi,stripTerminalSequences} from '@earendil-works/pi-tui';
import {plain,fit} from './chrome.ts';
import {rgbPng} from './png.ts';
const reset='\x1b[0m';
export interface CodeCopy {row:number;x:number;width:number;height:number;text:string;iconRow?:number}

function tabColors(light:boolean,copied:boolean) {
 return copied
  ? {background:light?[66,115,74]:[160,197,155],foreground:light?[255,250,242]:[28,16,8]}
  : {background:[232,117,43],foreground:[28,16,8]};
}
const copyIcons=new Map<string,string>();
/** The website's 8px copy mark, drawn independently of the terminal font. */
export function copyTabIcon(light:boolean,copied:boolean):string {
 const key=`${light}:${copied}`,cached=copyIcons.get(key);
 if(cached)return cached;
 const {background,foreground}=tabColors(light,copied),size=20,scanlines=Buffer.alloc((size*3+1)*size);
 for(let y=0;y<size;y++)for(let x=0;x<size;x++){
  const px=Math.floor(x/2)-1,py=Math.floor(y/2)-1;
  const front=copied
   ? [[1,4],[2,5],[3,6],[4,5],[5,4],[6,3],[7,2]].some(([a,b])=>px===a&&py===b)
   : (py===2&&px>=0&&px<=5)||((px===0||px===5)&&py>=3&&py<=7)||(py===7&&px>=1&&px<=4);
  const back=!copied&&((py===0&&px>=2&&px<=7)||(px===7&&py>=1&&py<=5)||(px===2&&py===1));
  const opacity=front?1:back?0.5:0;
  for(let channel=0;channel<3;channel++)scanlines[y*(size*3+1)+1+x*3+channel]=Math.round(background[channel]!*(1-opacity)+foreground[channel]!*opacity);
 }
 const png=rgbPng(size,size,scanlines).toString('base64');copyIcons.set(key,png);return png;
}

const historyBlocks = new Map<string, {block:ReturnType<typeof codeBlocks>; size:number}>();
const historyCacheLimit = 2 * 1024 * 1024;
let historyCacheSize = 0;

/** Animation frames reuse settled text; copy targets still receive current layout coordinates. */
export function cachedCodeBlocks(
 text: string,
 width: number,
 light: boolean,
 markdown: (text: string) => string[],
 copied = '',
 style = '',
 graphics = false,
) {
 const key = JSON.stringify([text, width, light, copied, style, graphics]);
 const cached = historyBlocks.get(key);
 if (cached) {
  historyBlocks.delete(key);
  historyBlocks.set(key, cached);
  return cached.block;
 }
 const block = codeBlocks(text, width, light, markdown, copied, graphics);
 const size = 2 * (key.length + block.rows.reduce((total, row) => total + row.length, 0));
 if (size <= historyCacheLimit) {
  while (historyBlocks.size >= 128 || historyCacheSize + size > historyCacheLimit) {
   const first = historyBlocks.keys().next().value!;
   historyCacheSize -= historyBlocks.get(first)!.size;
   historyBlocks.delete(first);
  }
  historyBlocks.set(key, {block, size});
  historyCacheSize += size;
 }
 return block;
}

// shortcut: basic tokens cover common snippets; use a grammar highlighter when language-specific parsing is needed.
function highlight(line:string,language:string,light:boolean){
 const ink=light?'\x1b[38;2;40;45;50m':'\x1b[38;2;220;220;210m';
 const colors=light?['100;110;105','45;110;60','125;65;155','170;85;25']:['120;130;125','145;195;150','190;160;215','225;158;100'];
 const shell=/^(bash|sh|zsh|shell|console)$/.test(language);
 const tokens=/(?:#[^\n]*|\/\/[^\n]*)|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`|\b(?:def|class|return|import|from|for|while|if|else|elif|in|const|let|var|function|async|await|export|fn|pub|use|true|false|null|None|True|False)\b|\b\d+(?:\.\d+)?\b|--?[a-zA-Z][\w-]*/g;
 const highlighted=line.replace(tokens,token=>{
  const kind=token.startsWith('#')||token.startsWith('//')?0:/^["'`]/.test(token)?1:/^\d/.test(token)?2:3;
  return `\x1b[38;2;${colors[kind]}m${token}${ink}`;
 });
 const commandColor=light?'45;105;125':'95;170;190';
 return ink+(shell?highlighted.replace(/^([\w./-]+)/,`\x1b[38;2;${commandColor}m$1`+ink):highlighted)+reset;
}

export function codeBlocks(text:string,width:number,light:boolean,markdown:(text:string)=>string[],copied='',graphics=false){
 text=stripTerminalSequences(text).replace(/[\x00-\x08\x0b-\x1f\x7f-\x9f]/g,'');
 const rows:string[]=[],copies:CodeCopy[]=[];let prose:string[]=[];
 const flush=()=>{if(prose.length){rows.push(...markdown(prose.join('\n')));prose=[];}};
 const lines=text.split('\n'),bg=light?'\x1b[48;2;232;228;217m':'\x1b[48;2;26;27;24m';
 const card=Math.max(1,width),pad=width>=40?2:1;
 const surface=(line:string)=>bg+line.replaceAll(reset,reset+bg)+' '.repeat(Math.max(0,card-visibleWidth(line)))+reset;
 for(let i=0;i<lines.length;i++){
  const open=/^ {0,3}(`{3,}|~{3,})([^`]*)$/.exec(lines[i]!);
  if(!open){prose.push(lines[i]!);continue;}
  flush();const fence=open[1]!,language=plain(open[2]!.trim().split(/\s/)[0]??'').toLowerCase(),source:string[]=[];
  while(++i<lines.length){
   const close=/^ {0,3}(`{3,}|~{3,})\s*$/.exec(lines[i]!);
   if(close&&close[1]![0]===fence[0]&&close[1]!.length>=fence.length)break;
   source.push(lines[i]!);
  }
  const code=source.join('\n');
  if(width<14){rows.push(...source.flatMap(line=>wrapTextWithAnsi(highlight(line.replaceAll('\t','    '),language,light),Math.max(1,width))));continue;}
  const done=copied===code,button=((graphics?'    ':' ')+(done?'Copied':'Copy')).padEnd(graphics?11:8);
  const x=card-button.length,{background,foreground}=tabColors(light,done);
  const action=`\x1b[48;2;${background.join(';')}m\x1b[38;2;${foreground.join(';')}m`;
  const first = source[0] ?? '';
  const inline=source.length===1&&/^(bash|sh|zsh|shell|console)$/.test(language)&&visibleWidth(first.replaceAll('\t','    '))<=x-pad-2;
  copies.push({row:rows.length,x,width:button.length,height:inline?3:1,text:code,...(graphics?{iconRow:inline?1:0}:{})});
  if(inline){
   const edge=surface(' '.repeat(x)+action+' '.repeat(button.length)+reset),lead=highlight(first.replaceAll('\t','    '),language,light);
   rows.push(edge,surface(' '.repeat(pad)+lead+' '.repeat(x-pad-visibleWidth(lead))+action+button+reset),edge);
   prose.push('');continue;
  }
  rows.push(surface(' '.repeat(x)+action+button+reset),surface(''));
  for (const line of source) {
   for (const wrapped of wrapTextWithAnsi(highlight(line.replaceAll('\t','    '),language,light),Math.max(1,card-pad*2))) {
    rows.push(surface(' '.repeat(pad)+wrapped));
   }
  }
  rows.push(surface(''));prose.push('');
 }
 flush();return {rows:rows.map(line=>fit(line,width)),copies};
}
