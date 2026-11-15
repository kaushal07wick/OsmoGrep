import {visibleWidth,wrapTextWithAnsi,stripTerminalSequences} from '@earendil-works/pi-tui';
import {plain,fit} from './chrome.ts';
const reset='\x1b[0m';
export interface CodeCopy {row:number;x:number;width:number;text:string}

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
 const lines=text.split('\n'),bg=light?'\x1b[48;2;240;238;231m':'\x1b[48;2;19;22;24m';
 const dim=light?'\x1b[38;2;100;100;95m':'\x1b[38;2;140;145;145m';
 const surface=(line:string)=>bg+line.replaceAll(reset,reset+bg)+' '.repeat(Math.max(0,width-visibleWidth(line)))+reset;
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
  const button=copied===code?'[Copied]':'[Copy]',x=width-button.length-1,languageLabel=fit(language||'code',Math.max(1,x-2));
  copies.push({row:rows.length,x,width:button.length,text:code});
  rows.push(surface(dim+' '+languageLabel+' '.repeat(Math.max(1,x-visibleWidth(languageLabel)-1))+button));
  for(const line of source){
   for(const wrapped of wrapTextWithAnsi(highlight(line.replaceAll('\t','    '),language,light),width-4))rows.push(surface('  '+wrapped));
  }
  rows.push(surface(''));prose.push('');
 }
 flush();return {rows:rows.map(line=>fit(line,width)),copies};
}
