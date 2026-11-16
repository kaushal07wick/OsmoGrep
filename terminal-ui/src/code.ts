import {visibleWidth,wrapTextWithAnsi,stripTerminalSequences} from '@earendil-works/pi-tui';
import {plain,fit} from './chrome.ts';
const reset='\x1b[0m';
export interface CodeCopy {row:number;x:number;width:number;text:string}

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
) {
 const key = JSON.stringify([text, width, light, copied, style]);
 const cached = historyBlocks.get(key);
 if (cached) {
  historyBlocks.delete(key);
  historyBlocks.set(key, cached);
  return cached.block;
 }
 const block = codeBlocks(text, width, light, markdown, copied);
 const size = 2 * (key.length + block.rows.reduce((total, row) => total + row.length, 0));
 if (size <= historyCacheLimit) {
  while (historyBlocks.size >= 8 || historyCacheSize + size > historyCacheLimit) {
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
 return ink+(shell?highlighted.replace(/^([\w./-]+)/,'\x1b[38;2;95;170;190m$1'+ink):highlighted)+reset;
}

export function codeBlocks(text:string,width:number,light:boolean,markdown:(text:string)=>string[],copied=''){
 text=stripTerminalSequences(text).replace(/[\x00-\x08\x0b-\x1f\x7f-\x9f]/g,'');
 const rows:string[]=[],copies:CodeCopy[]=[];let prose:string[]=[];
 const flush=()=>{if(prose.length){rows.push(...markdown(prose.join('\n')));prose=[];}};
 const lines=text.split('\n'),bg=light?'\x1b[48;2;242;240;234m':'\x1b[48;2;16;17;17m';
 const dim=light?'\x1b[38;2;100;100;95m':'\x1b[38;2;140;145;145m';
 const inset=width>=40?1:0,card=Math.max(1,width-inset*2),pad=width>=28?2:1;
 const surface=(line:string)=>' '.repeat(inset)+bg+line.replaceAll(reset,reset+bg)+' '.repeat(Math.max(0,card-visibleWidth(line)))+reset+' '.repeat(inset);
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
  const button=copied===code?'✓ Copied':'⧉ Copy',x=width-inset-pad-visibleWidth(button),languageLabel=fit((/^(sh|bash|shell|console)$/.test(language)?'shell':language||'code').toUpperCase(),Math.max(1,x-inset-pad-2));
  const action=copied===code?(light?'\x1b[38;2;40;120;65m':'\x1b[38;2;135;185;145m'):(light?'\x1b[38;2;70;75;70m':'\x1b[38;2;185;185;175m');
  rows.push(surface(''));
  copies.push({row:rows.length,x,width:visibleWidth(button),text:code});
  rows.push(surface(' '.repeat(pad)+dim+languageLabel+' '.repeat(Math.max(1,x-inset-pad-visibleWidth(languageLabel)))+action+button),surface(''));
  for(const line of source){
   for(const wrapped of wrapTextWithAnsi(highlight(line.replaceAll('\t','    '),language,light),Math.max(1,card-pad*2)))rows.push(surface(' '.repeat(pad)+wrapped));
  }
  rows.push(surface(''));prose.push('');
 }
 flush();return {rows:rows.map(line=>fit(line,width)),copies};
}
