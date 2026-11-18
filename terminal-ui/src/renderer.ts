import {type AccountIdentity} from './account.ts';
import {plain} from './chrome.ts';
import {cachedCodeBlocks,copyTabIcon,type CodeCopy} from './code.ts';
import { Image, Markdown, getCapabilities, renderImage, stripTerminalSequences, truncateToWidth, visibleWidth, wrapTextWithAnsi } from '@earendil-works/pi-tui';
import { introDuration, websiteMark } from './brand.ts';
import { introFrame, introPng } from './intro.ts';
import { drawPanel, permissionLines, updateLines, type ReleaseView, type PanelView } from './panels.ts';
import type {Scene} from './scenes.ts';
import {statusLayout,activity,type StatusEntry} from './chrome.ts';
import {diffLines,highlightTranscript,selectedTranscript,type TranscriptSelection,type FileDiff} from './transcript.ts';
import {webLinks} from './links.ts';
import {paperRow,paperColors,paperGraphic} from './paper.ts';
import {orb,activityInfo,completionLine} from './activity.ts';
import {providerMarks} from './provider-marks.ts';
import {teamMessageLines,teamNoticeActive} from './team.ts';

export interface Rect { x: number; y: number; width: number; height: number }
export interface Attachment { id: number; label: string; thumbnail: string; width: number; height: number }
export interface View {
  wallTime?: number;
  id: number; width: number; height: number; version: string; cwd: string;
  model: string; provider: string; branch: string; indexed: boolean; indexing: boolean;
  account?:AccountIdentity|null;
  theme: string; accent: string; density: string; session: string | null; elapsed: number; profile: string; autoApprove: boolean; planMode: boolean;
  logs: { text: string; level: string; kind?: string }[]; plan: { text: string; done: boolean; active: boolean }[];
  diffs: FileDiff[];
  input: string; cursor: number; selection: [number, number] | null; masked: boolean;
  inputMode: string; attachments: Attachment[]; running: boolean; phase: string; tool: string | null;
  followTail: boolean; scroll: number; stream: string; permission: string | null; update: string | null;
  streaming?:boolean;streamingStart?:number;
  copyNotice?:{characters:number;age:number}|null;
  newActivity?:boolean;
  voiceState?:{enabled:boolean;connected:boolean;partial:boolean};
  team?:{enabled:boolean;unread:number;online:number};
  voice: string | null; commands: { cmd: string; desc: string }[]; selectedCommand: number; tokens: number;
  introElapsed?: number;
  introScene?: Scene;
  introBright?: boolean;
  conversationStarted?:boolean;
  userTurns?:number;
  workspaceStatus?:string|null;
  panel?: PanelView | null;
  approval?: {tool:string;args:string;selected?:number} | null;
  statusLine?: string[];
  statusEntries?: StatusEntry[];
  copiedCode?:string;
  release?:ReleaseView|null;
  runElapsed?:number;
  expandTools?:boolean;
  scrollTop?:number;
  queuedMessages?:number;queuePaused?:boolean;
  transcriptSelection?:TranscriptSelection|null;
  shortcuts?: Record<string,string>;
  preferences?: {status_fields:string[]; reduce_motion:boolean; ambient_art:boolean};
}
export interface Rendered {
  id: number; rows: string[]; input: Rect; execution: Rect; cursor: { x: number; y: number }; cursorVisible?:boolean;
  editorWidth: number;
  animate: boolean;
  animationIntervalMs?:number;
  artwork: Rect | null;
  controls?:{id:'model'|'provider'|'choice'|'voice'|'team'|'copy'|'update'|'permission'|'expand'|'latest'|'link';index?:number;label?:string;x:number;y:number;width:number;height:number}[];
  transcript?:{x:number;y:number;width:number;height:number;start:number;maxScroll:number;lineWidths:number[];scrollbar?:Rect|null};
  selectionText?:string;
  panelScroll?:{requested:number;offset:number};
  images: { id: number; x: number; y: number; sequence: string; placement?:string }[];
}
const reset = '\x1b[0m';
const rgb = (r: number, g: number, b: number) => `\x1b[38;2;${r};${g};${b}m`;
const bold = (s: string) => `\x1b[1m${s}\x1b[22m`;
export function clean(s: string): string {
  return stripTerminalSequences(s).replace(/[\x00-\x08\x0b-\x1f\x7f-\x9f]/g, '').replace(/\t/g, '    ');
}
const clip = (s: string, width: number) => width > 0 ? truncateToWidth(s, width, '') : '';

// Code-point offsets match the Rust editor's Unicode boundary-safe cursor.
function inputLines(text: string, cursor: number, width: number) {
  const lines: string[] = [''];
  const ranges: [number, number][] = [[0, 0]];
  let x = 0, y = 0, position = { x: 0, y: 0 };
  let i = 0;
  for (const c of text) {
    if (c === '\n') {
      if (i === cursor) position = { x, y };
      y++; lines.push(''); ranges.push([i + 1, i + 1]); x = 0; i++; continue;
    }
    const cells = visibleWidth(c);
    if (x > 0 && x + cells > width) { y++; lines.push(''); ranges.push([i, i]); x = 0; }
    if (i === cursor) position = { x, y };
    lines[y] += c; ranges[y]![1] = i + 1; x += cells; i++;
    if (x >= width) { y++; lines.push(''); ranges.push([i, i]); x = 0; }
  }
  if (i === cursor) position = { x, y };
  return { lines, ranges, position };
}

export function render(v: View): Rendered {
  const w = Math.max(1, Math.min(1000, v.width)), h = Math.max(1, Math.min(500, v.height));
  const gutter = w >= 40 ? 1 : 0, width = Math.max(1, w - gutter * 2);
  const light = v.theme === 'light';
  const fg = light ? rgb(35, 35, 35) : rgb(255, 255, 255);
  const dim = light ? rgb(95, 95, 95) : rgb(145, 145, 145);
  const border = light ? rgb(190, 190, 190) : rgb(65, 65, 65);
  const accents = light
    ? { blue: rgb(20,105,210), green: rgb(20,135,80), violet: rgb(120,85,210), rose: rgb(190,55,90), orange: rgb(210,105,20) }
    : { blue: rgb(95,175,255), green: rgb(70,190,120), violet: rgb(185,140,255), rose: rgb(255,110,145), orange: rgb(235,155,105) };
  const accent = accents[v.accent as keyof typeof accents] ?? accents.orange;
  const bg = light ? '\x1b[48;2;240;240;240m' : '\x1b[48;2;30;30;30m';
  const paper=paperColors(light);
  const controls:NonNullable<Rendered['controls']>=[];
  const images: Rendered['images'] = [];
  const paperGraphics=getCapabilities().images==='kitty';
  const tint = (color: string) => (s: string) => color + s + reset;
  const markdown = (text: string, n: number) => new Markdown(clean(text), 0, 0, {
    heading: s => fg + bold(s) + reset, link: tint(light?rgb(38,103,175):rgb(112,166,224)), linkUrl: tint(light?rgb(38,103,175):rgb(112,166,224)), code: tint(accent),
    codeBlock: tint(fg), codeBlockBorder: tint(border), quote: tint(dim), quoteBorder: tint(border),
    hr: tint(border), listBullet: tint(accent), bold, italic: s => `\x1b[3m${s}\x1b[23m`,
    strikethrough: s => `\x1b[9m${s}\x1b[29m`, underline: s => `\x1b[4m${s}\x1b[24m`,
  }, { color: tint(fg) }).render(Math.max(1, n));
  const rows = Array<string>(h).fill('');
  const put = (y: number, text: string, x = gutter, n = width) => {
    if (y >= 0 && y < h) rows[y] = ' '.repeat(Math.min(x, w - 1)) + clip(text, Math.min(n, w - x));
  };
  if(v.release){
    const boxWidth=Math.max(1,Math.min(88,w-8)),inset=boxWidth>=30?2:0,inner=Math.max(1,boxWidth-inset*2);
    const lines=updateLines(v.release,inner,accent,light,h<25,v.preferences?.reduce_motion?0:v.elapsed);
    const shown=lines.slice(0,Math.max(1,h-2)),x=Math.max(0,Math.floor((w-boxWidth)/2))+inset,y=Math.max(0,Math.floor((h-shown.length)/2));
    shown.forEach((line,i)=>put(y+i,line,x,inner));
    if(v.release&&!v.release.installing){
      const labels=v.release.progress?.stage==='ready'?['R Restart & open chat']:[v.release.progress?.stage==='failed'?'R Retry update':'U Update now','L Later','S Skip this version'];
      labels.forEach((label,index)=>{const row=shown.findIndex(line=>stripTerminalSequences(line).includes(label));if(row>=0){const before=stripTerminalSequences(shown[row]!).split(label)[0]!;controls.push({id:'update',index,x:x+visibleWidth(before),y:y+row,width:Math.min(label.length,inner-visibleWidth(before)),height:1});}});
    }
    const root=light?'\x1b[48;2;250;250;250m':'\x1b[48;2;0;0;0m';
    const empty={x:0,y:0,width:w,height:0};
    return {id:v.id,rows:rows.map(row=>root+fg+clip(row,w)+' '.repeat(Math.max(0,w-visibleWidth(clip(row,w))))+reset),input:empty,execution:empty,editorWidth:inner,animate:!!v.release?.installing&&!v.preferences?.reduce_motion,artwork:null,cursor:{x:0,y:0},cursorVisible:false,images:[],controls};
  }
  if (v.panel && !v.permission && !v.update) {
    const empty={x:0,y:0,width:w,height:0};
    const panel=drawPanel(v.panel,w,h,light,accent,!!v.preferences?.reduce_motion,paperGraphics);
    const portrait=panel.avatar;
    const avatar=portrait&&v.panel.identity?.avatar&&paperGraphics?renderImage(v.panel.identity.avatar,{widthPx:96,heightPx:96},{imageId:(process.pid*128+116)>>>0,maxWidthCells:portrait.width,maxHeightCells:portrait.height,moveCursor:false}):null;
    const accountImages=avatar&&portrait?[{id:(process.pid*128+116)>>>0,x:portrait.x,y:portrait.y,sequence:avatar.sequence}]:[];
    return {id:v.id,rows:panel.rows,input:empty,execution:empty,editorWidth:Math.max(1,w-8),animate:!v.preferences?.reduce_motion&&(v.panel.health_running||v.panel.account_running||(v.panel.section===22&&panel.animate)||(v.panel.section===8&&(v.panel.elapsed??900)<900)||(v.panel.section===10&&!v.panel.previewing&&(v.panel.elapsed??900)<240)),animationIntervalMs:v.panel.section===22?(v.panel.account_running?150:33):undefined,artwork:null,panelScroll:panel.scroll,cursor:panel.cursor,cursorVisible:![8,22].includes(v.panel.section)&&!v.panel.previewing,images:accountImages,controls:panel.controls};
  }
  const conversationStarted=v.conversationStarted||v.running||!!v.stream||v.logs.some(log=>log.text.startsWith('USER|'));
  const userTurns=v.userTurns??(v.conversationStarted?2:v.logs.filter(log=>log.text.startsWith('USER|')&&!log.text.startsWith('USER|/')).length);
  const hideHeader=userTurns>=2;
  const headerY = hideHeader || (v.approval&&h<32) || h < 15 || v.density === 'compact' ? 0 : v.density === 'spacious' ? 2 : 1;
  const headerHeight = hideHeader || v.approval&&h<32?0:h >= 15 && v.density !== 'compact' ? (v.density === 'spacious' ? 5 : 4) : h >= 8 ? 2 : 0;
  const header = [
    fg + bold('osmogrep') + reset,
    v.account ? fg + clean(v.account.name) + reset : dim + clean(`${v.model} · ${v.provider}`) + reset,
    dim + clean(v.cwd) + reset,
  ];
  const mark = websiteMark(light);
  if (headerHeight >= 4 && width >= 40) {
    header.forEach((line, i) => put(headerY + i, mark[i] + '  ' + line));
  } else if (headerHeight) {
    put(headerY, header[0]!); put(headerY + 1, dim + clean(v.cwd) + reset);
  }

  const pad = width >= 40 ? 2 : width >= 20 ? 1 : 0, textWidth = Math.max(1, width - pad * 2 - 2);
  const value = clean(v.input);
  const input = inputLines(value, Math.min(v.cursor, Array.from(value).length), textWidth);
  const attached = v.attachments.filter(a => value.includes(a.label));
  const previewHeight = attached.length && h >= 20 && !v.approval ? 5 : 0;
  const visible = Math.min(input.lines.length, Math.max(1, Math.min(v.approval?2:8, Math.floor(h / 3) - previewHeight)));
  const verticalPad = h >= 8 ? 1 : 0;
  const composerHeight = Math.min(h, visible + verticalPad * 2 + previewHeight);
  const footerHeight = h > composerHeight + headerHeight + headerY + 1 ? 2 : 0;
  const composerY = Math.max(0, h - footerHeight - composerHeight);
  const inputRect = { x: gutter, y: composerY, width, height: composerHeight };
  const lastTeam = v.logs.findLastIndex(log => log.kind === 'Peer');
  const lastUser = v.logs.findLastIndex(log => log.text.startsWith('USER|'));
  const teamLines = lastTeam > lastUser && v.followTail && !v.panel && !v.approval && !v.update && !v.masked && h >= 20 && width >= 24
    && teamNoticeActive(v.logs[lastTeam]!.text, v.wallTime ?? Date.now())
    ? teamMessageLines(v.logs[lastTeam]!.text, Math.max(1, width - pad * 2), light, accent) : null;
  const teamDock = teamLines?.slice(1, -1).slice(0, 3) ?? [];
  if (teamLines && teamLines.length > teamDock.length + 2) {
    teamDock[teamDock.length - 1] = clip(teamDock.at(-1)!, Math.max(1, width - pad * 2 - 2)) + dim + ' …' + reset;
    teamDock.push(accent + '  Show more' + reset);
  }
  const teamHeight = teamDock.length ? teamDock.length + 2 : 0;
  const activityBottom = composerY - teamHeight;
  const activityHeight=v.running&&!v.update&&h>=24&&width>=36?4:v.workspaceStatus?2:1;
  const execution = { x: gutter, y: headerY + headerHeight, width, height: Math.max(0, activityBottom - headerY - headerHeight - activityHeight) };
  const approvalRows=v.approval?permissionLines(v.approval.tool,v.approval.args,v.approval.selected??0,width,accent,light,h<32):v.release?updateLines(v.release,width,accent,light,h<40):[];
  const approvalHeight=Math.min(approvalRows.length,Math.max(0,Math.max(0,execution.height-3)));
  execution.height=Math.max(0,execution.height-(approvalHeight?approvalHeight+1:0));
  const transcript: string[] = [];
  const insertedDiffs=new Set<FileDiff>();
  const codeCopies:CodeCopy[]=[];
  const expandActions: {row:number;label:string;start:number}[] = [];
  const expandAction = (hidden?: number, start = transcript.length) => {
    const label = v.expandTools ? 'Show less' : hidden == null ? 'Show more' : `Show more · ${hidden} more lines`;
    expandActions.push({row:transcript.length,label,start});
    transcript.push(accent + '  ' + label + reset);
  };
  const appendMarkdown = (text: string, fold = true) => {
    const from=transcript.length;
    const block = cachedCodeBlocks(text,Math.max(1,width-pad*2),light,text=>markdown(text,width-pad*2),v.copiedCode,accent,paperGraphics);
    const large = fold && block.rows.length > 32;
    const shown = large && !v.expandTools ? block.rows.slice(0,32) : block.rows;
    codeCopies.push(...block.copies.filter(copy => copy.row < shown.length).map(copy => ({
      ...copy,row:copy.row+transcript.length,height:Math.min(copy.height,shown.length-copy.row),
    })));
    transcript.push(...shown);
    if (large) expandAction(block.rows.length - 32,from);
  };
  const appendDiff = (diff: FileDiff) => {
    const from=transcript.length,rows=diffLines(diff,Math.max(1,width-pad*2),light,!!v.expandTools);
    transcript.push(...rows);
    if (/^Show (more|less)$/.test(stripTerminalSequences(rows.at(-1)??'').trim())) {
      transcript.pop();
      expandAction(undefined,from);
    }
  };
  let paragraph: string[] = [];
  const flush = () => { if (paragraph.length) { appendMarkdown(paragraph.join('\n')); paragraph = []; } };
  for (let i = 0; i < v.logs.length; i++) {
    const firstLog=i;
    const entry = v.logs[i]!; const text = clean(entry.text);
    if (entry.kind === 'Peer') {
      continue;
    } else if (v.streaming && i >= (v.streamingStart??0)) {
      paragraph.push(stripTerminalSequences(entry.text));
    } else if (text.startsWith('USER|')) {
      flush(); let command = text.slice(5), prefix = '❯ ';
      if (v.logs[i + 1]?.text === 'SHELL : $ ' + command) { i++; prefix = '$ '; }
      transcript.push('', ...wrapTextWithAnsi(accent + prefix + reset + fg + bold(command) + reset, Math.max(1, width - pad * 2)), '');
    } else if (text.startsWith('· Finished in ')) {
      flush();transcript.push('',dim+(completionLine(text,Math.max(1,width-pad*2))??text)+reset);
    } else if (text.trimStart().startsWith('└ ')) {
      flush();const results=[text];
      const from=transcript.length;
      while(v.logs[i+1]?.text.trimStart().startsWith('└ '))results.push(clean(v.logs[++i]!.text));
      const rendered = results.flatMap(line => wrapTextWithAnsi(dim+line+reset,Math.max(1,width-pad*2)));
      transcript.push(...(v.expandTools ? rendered : rendered.slice(0,3)));
      if (rendered.length > 3) expandAction(rendered.length - 3,from);
    } else if (text.startsWith('● ') || text.trimStart().startsWith('└ ') || text.startsWith('· ') || entry.level !== 'Info') {
      flush();
      const color = entry.level === 'Error' ? rgb(230, 100, 100) : entry.level === 'Warn' ? rgb(225, 185, 75) : text.startsWith('● ') ? accent : dim;
      transcript.push(...wrapTextWithAnsi(color + text + reset, Math.max(1, width - pad * 2)));
    } else { paragraph.push(stripTerminalSequences(entry.text).replace(/[\x00-\x08\x0b-\x1f\x7f-\x9f]/g,'')); }
    for(const diff of v.diffs)if(diff.afterLog!=null&&diff.afterLog>=firstLog&&diff.afterLog<=i){
      flush();appendDiff(diff);insertedDiffs.add(diff);
    }
  }
  if(v.streaming||v.stream){
    appendMarkdown([...paragraph,v.stream+'▍'].join('\n'),false);
    paragraph=[];
  } else flush();
  for(const diff of v.diffs)if(!insertedDiffs.has(diff))appendDiff(diff);
  if (v.plan.length) transcript.push('', ...v.plan.map(p => `${p.done ? '✓' : p.active ? '›' : '·'} ${clean(p.text)}`));
  const contentHeight=Math.max(0,execution.height-(v.expandTools&&expandActions.length?1:0));
  const maxScroll = Math.max(0, transcript.length - contentHeight);
  const start = v.followTail ? maxScroll : Math.max(0,Math.min(maxScroll,v.scrollTop??maxScroll-v.scroll));
  transcript.slice(start, start + contentHeight).forEach((line, i) => {
    const marked=webLinks(line,light);
    put(execution.y+i,highlightTranscript(marked.text,start+i,v.transcriptSelection,light),gutter+pad,width-pad*2);
    if(!v.panel&&!v.approval&&!v.update&&!v.masked&&!v.commands.length)for(const link of marked.links){
      const length=Math.min(link.width,width-pad*2-link.x);
      if(length>0)controls.push({id:'link',label:link.url,x:gutter+pad+link.x,y:execution.y+i,width:length,height:1});
    }
  });
  if (!v.panel && !v.update && !v.masked) for (const action of expandActions) {
    let y = execution.y + action.row - start;
    if(v.expandTools&&action.start<start+contentHeight&&action.row>=start&&y>=execution.y+contentHeight){
      y=execution.y+contentHeight;
      put(y,accent+'  Show less'+reset,gutter+pad,Math.max(1,width-pad*2));
    }
    if (y >= execution.y && y < execution.y + execution.height && width - pad * 2 > 2) {
      controls.push({id:'expand',label:action.label,x:gutter+pad+2,y,width:Math.min(visibleWidth(action.label),Math.max(1,width-pad*2-2)),height:1});
    }
  }
  let copyIconIndex=0;
  if(!v.masked&&!v.approval&&!v.update&&!v.commands.length)for(const copy of codeCopies){
    const y=execution.y+copy.row-start,top=Math.max(execution.y,y),bottom=Math.min(execution.y+contentHeight,y+copy.height);
    if(!copy.text||Buffer.byteLength(copy.text)>100000||bottom<=top)continue;
    const x=gutter+pad+copy.x;
    // The native frame protocol accepts one terminal row per hit area.
    for(let row=top;row<bottom;row++)controls.push({id:'copy',label:copy.text,x,y:row,width:copy.width,height:1});
    const iconY=y+(copy.iconRow??0);
    if(copy.iconRow!=null&&iconY>=top&&iconY<bottom){
      // Keep code icons in a separate ID range from attachment and workspace graphics.
      const id=(0x40000000+process.pid*256+copyIconIndex++)>>>0;
      const graphic=renderImage(copyTabIcon(light,v.copiedCode===copy.text),{widthPx:20,heightPx:20},{imageId:id,maxWidthCells:2,maxHeightCells:1,moveCursor:false});
      if(graphic)images.push({id,x:x+1,y:iconY,sequence:graphic.sequence.replace('a=T','a=t'),placement:`\x1b_Ga=p,i=${id},p=1,c=2,r=1,C=1,q=2;\x1b\\`});
    }
  }
  const scrollbar=null;

  let artwork: Rect | null = null;
  const introElapsed = v.preferences?.reduce_motion ? introDuration : v.introElapsed ?? v.elapsed;
  if (!conversationStarted && v.preferences?.ambient_art!==false && !v.panel && !value && !v.masked && !transcript.length && !v.running && !v.permission && !v.update && !v.workspaceStatus && !v.voice && !v.commands.length
    && width >= 36 && execution.height >= 16) {
    const clearance=Math.max(3,Math.min(6,Math.floor(execution.height*.15)));
    const height = Math.min(26, execution.height-clearance-1), columns = Math.min(56, height*2, width-6);
    artwork = {x:gutter+Math.floor((width-columns)/2),y:execution.y+Math.floor((execution.height-clearance-height)/2),width:columns,height};
    if (getCapabilities().images === 'kitty') {
      const graphicId = (process.pid*128+127)>>>0;
      const png=introPng(introElapsed,columns,height,light,v.introScene,v.introBright).toString('base64');
      const image=renderImage(png,{widthPx:columns*12,heightPx:height*24},{imageId:graphicId,maxWidthCells:columns,maxHeightCells:height,moveCursor:false});
      if(image) images.push({id:graphicId,x:artwork.x,y:artwork.y,sequence:image.sequence});
    } else {
      introFrame(introElapsed,columns,height,light,v.introScene,v.introBright).forEach((line,i)=>put(artwork!.y+i,line,artwork!.x,columns));
    }
  }

  const status = v.release?'':v.update ?? (v.permission&&!v.approval?v.permission:null) ?? (v.running ? activity(v.runElapsed??v.elapsed,v.approval?'Waiting for permission':v.phase,v.approval||v.phase==='thinking'?null:v.tool,!!v.preferences?.reduce_motion) : v.workspaceStatus??'');
  if(status){
    if(v.running&&activityHeight===4&&!v.update){
      const y=activityBottom-4,frame=orb(v.runElapsed??v.elapsed,light,!!v.preferences?.reduce_motion),info=activityInfo(v.runElapsed??v.elapsed,v.approval?'Waiting for approval':v.phase,v.approval?null:v.tool,!!v.preferences?.reduce_motion);
      const id=(process.pid*128+125)>>>0;
      const graphic=paperGraphics?renderImage(frame.png.toString('base64'),{widthPx:36,heightPx:36},{imageId:id,maxWidthCells:6,maxHeightCells:3,moveCursor:false}):null;
      frame.rows.forEach((line,i)=>{
        const label=i===1?accent+clip(info.label,Math.max(1,width-pad*2-8)):i===2?dim+clip(`${info.time} elapsed · esc interrupt`,Math.max(1,width-pad*2-8)):'';
        put(y+i,(graphic?' '.repeat(6):dim+line+reset)+'  '+label+reset,gutter+pad,Math.max(1,width-pad*2));
      });
      if(graphic)images.push({id,x:gutter+pad,y,sequence:graphic.sequence});
    }else{const action=v.running?'esc interrupt':'';const left=clip(status,Math.max(1,width-action.length-4));put(activityBottom-(v.workspaceStatus&&!v.running?2:1),accent+left+reset+(width>action.length+visibleWidth(left)+3?' '.repeat(width-visibleWidth(left)-action.length)+dim+action+reset:''));}
  }
  if(approvalHeight){
    const y=activityBottom-approvalHeight-activityHeight-1;
    approvalRows.slice(0,approvalHeight).forEach((line,i)=>{
      const content=clip(line,width-2);
      put(y+i,bg+content.replaceAll(reset,reset+bg+fg)+' '.repeat(Math.max(0,width-visibleWidth(content)))+reset);
      const text=stripTerminalSequences(content);
      ['Y  Allow once','N  Deny','A  Auto-approve this session'].forEach((label,index)=>{
        const start=text.indexOf(label);
        if(start>=0)controls.push({id:'permission',index,x:gutter+visibleWidth(text.slice(0,start)),y:y+i,width:visibleWidth(label),height:1});
      });
    });
  }
  for (let y = composerY; y < composerY + composerHeight; y++) put(y, paperRow('',width,y-composerY,composerHeight,light,paperGraphics));
  if(verticalPad&&!v.masked&&!v.permission&&!v.update){
    let right=gutter+width-pad;const capsules:{x:number;content:string}[]=[];
    const capsule=(id:'voice'|'team',label:string)=>{const content=clip(label,Math.max(1,right-gutter-pad));if(!content)return;const x=right-visibleWidth(content);capsules.push({x,content});controls.push({id,x,y:composerY,width:visibleWidth(content),height:1});right=x-3;};
    if(v.voiceState?.enabled||v.voiceState?.connected||(!v.voiceState&&v.voice)) capsule('voice',v.voiceState?.connected?`◉ ${v.voiceState.partial?'Hearing you':'Listening'}`:v.voiceState?.enabled?'◌ Connecting voice':plain(v.voice??'○ Voice'));
    let line='';for(const c of capsules.sort((a,b)=>a.x-b.x))line+=' '.repeat(Math.max(0,c.x-gutter-visibleWidth(line)))+c.content;if(capsules.length)put(composerY,paperRow(line,width,0,composerHeight,light,paperGraphics));
  }
  teamDock.forEach((line, index) => put(composerY - teamHeight + 1 + index, line, gutter + pad, Math.max(1, width - pad * 2)));
  const copyNotice = v.copyNotice && v.copyNotice.age < 3000 && Number.isSafeInteger(v.copyNotice.characters) && v.copyNotice.characters >= 0;
  if(copyNotice&&!v.panel&&!v.approval&&!v.update&&composerY>0){
    put(composerY-1,accents.blue+`Copied ${v.copyNotice!.characters.toLocaleString('en-US')} characters to clipboard`+reset,gutter+pad,width-pad*2);
  }
  if(!copyNotice&&!v.followTail&&maxScroll>0&&composerY>0&&!v.panel&&!v.approval&&!v.update&&!v.masked&&!v.commands.length&&width>=24){
    const label=(v.newActivity?'● ':'')+'↓ Latest',x=gutter+width-pad-visibleWidth(label),y=composerY-1;
    put(y,(v.newActivity?accent:dim)+label+reset,x,visibleWidth(label));
    controls.push({id:'latest',label:'Latest output',x,y,width:visibleWidth(label),height:1});
  }
  if (teamDock.length) {
    const messageId = JSON.parse(v.logs[lastTeam]!.text.slice(5)).message.id;
    if (Number.isSafeInteger(messageId) && messageId > 0) {
      controls.push({id:'team',index:messageId,x:gutter+pad,y:composerY-teamHeight+1,width:Math.max(1,width-pad*2),height:1});
      if (teamDock.length > 3) controls.push({id:'team',index:messageId,x:gutter+pad,y:composerY-2,width:Math.min(12,width-pad*2),height:1});
    }
  }
  if (previewHeight) {
    let x = gutter + pad + 2;
    for (const a of attached) {
      if (x + 16 > gutter + width) break;
      const graphicId = (process.pid * 128 + a.id) >>> 0;
      const image = new Image(a.thumbnail, 'image/png', { fallbackColor: tint(dim) }, { imageId: graphicId, maxWidthCells: 14, maxHeightCells: 4 });
      const picture = image.render(16);
      if (picture.some(s => s.includes('\x1b_G') || s.includes('\x1b]1337;'))) {
        images.push({ id: graphicId, x, y: composerY + verticalPad, sequence: picture.join('') });
      } else { put(composerY + 2, bg + dim + a.label + reset, x, 16); }
      x += 18;
    }
  }
  const topLine = Math.max(0, input.position.y - visible + 1);
  const textY = composerY + verticalPad + previewHeight;
  for (let i = 0; i < visible && textY + i < composerY + composerHeight - verticalPad; i++) {
    const index = topLine + i; const line = input.lines[index] ?? '';
    const range = input.ranges[index] ?? [0, 0];
    const selected = v.selection;
    let styled = '', pos = range[0];
    for (const c of line) {
      const isSelected = selected && pos >= selected[0] && pos < selected[1];
      styled += (isSelected ? '\x1b[7m' : '') + c + (isSelected ? '\x1b[27m' : ''); pos++;
    }
    if (!value && !v.masked) styled = paper.muted + 'Try "explain this codebase"' + paper.foreground;
    const prefix = index === 0 ? '❯ ' : '  ';
    const content = ' '.repeat(pad) + accent + prefix + paper.foreground + styled;
    put(textY + i, paperRow(content,width,textY+i-composerY,composerHeight,light,paperGraphics));
  }
  if (footerHeight) {
    const showHints=!v.running&&!v.panel&&!v.masked&&!v.permission&&!v.update&&v.elapsed>=120000&&v.logs.some(log=>log.text.startsWith('USER|'));
    const statusY=showHints?h-2:h-1;
    const permission = v.planMode ? 'plan mode' : v.autoApprove ? 'auto-approve on' : 'ask approvals';
    const sessionsKey=(v.shortcuts?.agents??'super+left').replace('super+left','⌘←').replace('shift+left','shift+←');
    const sessionsHint=sessionsKey==='⌘←'?(width>=90?`⌘← / ${(v.shortcuts?.['sessions-fallback']??'ctrl+s')}`:(v.shortcuts?.['sessions-fallback']??'ctrl+s')):sessionsKey;
    const right = dim+((v.queuedMessages??0)>0?`${v.queuedMessages} queued${v.queuePaused?' · enter resume':''}`:v.running?`${v.shortcuts?.queue??'tab'} queue`:`${sessionsHint} sessions`)+reset;
    const reserve=width>=60?visibleWidth(right)+3:0;
    const entries=v.statusEntries??(v.statusLine?v.statusLine.map((value,i)=>({id:value.startsWith('~')||value.startsWith('/')?'directory':i===0?'model':'optional',value})):[{id:'permissions',value:`${v.profile} · ${permission}`}]);
    const providerMark=paperGraphics?providerMarks[v.provider.toLowerCase()]?.[light?'light':'dark']:undefined;
    const fields=entries.flatMap(e=>e.id==='model'?[{id:'provider',value:providerMark?'  ▾':clean(v.provider)+' ▾'},{id:'model',value:clean(v.model)+' ▾'}]:[e]);
    const layout=statusLayout(fields,Math.max(1,width-pad*2-reserve)),left=dim+layout.text+reset;
    if(!v.panel&&!v.permission&&!v.update&&!v.masked)for(const item of layout.items)if(item.id==='model'||item.id==='provider')controls.push({id:item.id,x:gutter+pad+item.x,y:statusY,width:item.width,height:1});
    put(statusY,' '.repeat(pad)+left,width>=40?gutter:gutter,Math.max(1,width-pad));
    const providerItem=layout.items.find(item=>item.id==='provider');
    if(providerMark&&providerItem&&providerItem.width>=2){
      const id=(process.pid*128+124)>>>0,graphic=renderImage(providerMark,{widthPx:16,heightPx:16},{imageId:id,maxWidthCells:2,maxHeightCells:1,moveCursor:false});
      if(graphic)images.push({id,x:gutter+pad+providerItem.x,y:statusY,sequence:graphic.sequence.replace('a=T','a=t'),placement:`\x1b_Ga=p,i=${id},p=1,c=2,r=1,C=1,q=2;\x1b\\`});
    }
    if(reserve)rows[statusY]+=' '.repeat(Math.max(1,width-pad*2-visibleWidth(left)-visibleWidth(right)))+right;

    if(showHints){const keys=(v.shortcuts?.keymap??'f8').toUpperCase();put(h-1,' '.repeat(pad)+dim+`${sessionsKey} / ${(v.shortcuts?.['sessions-fallback']??'ctrl+s')} sessions · ${keys} shortcuts`+reset);}
  }
  if (v.commands.length) {
    const bottom = composerY;
    const available = Math.max(0, bottom - execution.y), chrome = available >= 10 ? 7 : 0;
    const count = Math.min(5, v.commands.length, Math.max(0, available - chrome));
    const selected = Math.max(0, Math.min(v.commands.length - 1, v.selectedCommand));
    const first = Math.max(0, Math.min(selected - Math.floor(count / 2), v.commands.length - count));
    const menuWidth = Math.max(1, width), menuY = bottom - count - chrome;
    const surface = light ? '\x1b[48;2;240;240;240m' : '\x1b[48;2;22;22;22m';
    const paint = (y: number, text: string) => {
      const line = clip(text, Math.max(1,menuWidth-3));
      put(y, surface + line + ' '.repeat(Math.max(0, menuWidth - visibleWidth(line))) + reset, gutter, menuWidth);
    };
    if (count && chrome) {
      const title = '   Commands / quick actions', position = `${selected + 1} · ${v.commands.length}`;
      paint(menuY, '');
      paint(menuY+1, dim + (menuWidth >= 28 ? title + ' '.repeat(Math.max(1, menuWidth - 3 - title.length - position.length)) + position : title));
      paint(menuY + 2, '');
      paint(bottom - 4, '');
      paint(bottom - 3, dim + '  ↑↓ move   [enter] select   [esc] dismiss');
      paint(bottom - 2, '');paint(bottom - 1, status?accent+'  '+status+dim+(v.running?' · esc interrupt':''):'');
    }
    const commandWidth = Math.min(26, Math.floor(menuWidth * .4), Math.max(...v.commands.map(item => visibleWidth(clean(item.cmd)))));
    for (let i = 0; i < count; i++) {
      const index = first + i, item = v.commands[index]!, active = index === selected;
      const command = clip(clean(item.cmd), commandWidth);
      const label = menuWidth >= 44 ? command + ' '.repeat(commandWidth - visibleWidth(command) + 3) : clean(item.cmd);
      const description = menuWidth >= 44 ? (active ? fg : dim) + clean(item.desc) : '';
      paint(menuY + (chrome ? 3 : 0) + i, (active ? accent + '  › ' + bold(label) : dim + '    ' + label) + description);
    }
  }
  const rootBackground = light ? '\x1b[48;2;250;250;250m' : '\x1b[48;2;0;0;0m';
  if(paperGraphics)images.unshift({...paperGraphic(width,composerHeight,light,(process.pid*128+126)>>>0),x:gutter,y:composerY});


  return {
    id: v.id, rows: rows.map(row => {
      const clipped = clip(row, w);
      return rootBackground + fg + clipped.replaceAll(reset, reset + rootBackground + fg)
        + ' '.repeat(Math.max(0, w - visibleWidth(clipped))) + reset;
    }), input: inputRect, execution, editorWidth: textWidth, animate: !!copyNotice || v.running || !!teamDock.length || !!artwork && !v.preferences?.reduce_motion, animationIntervalMs:teamDock.length&&!v.running?250:artwork&&introElapsed>=introDuration?125:undefined,artwork,
    cursor: { x: Math.min(w - 1, gutter + pad + 2 + input.position.x), y: Math.min(h - 1, textY + input.position.y - topLine) },
    images, controls,
    transcript:{x:gutter+pad,y:execution.y,width:Math.max(1,width-pad*2),height:execution.height,start,maxScroll,scrollbar,lineWidths:transcript.slice(start,start+contentHeight).map(line=>Math.min(Math.max(1,width-pad*2),visibleWidth(line)))},
    selectionText:selectedTranscript(transcript,v.transcriptSelection),
  };
}
