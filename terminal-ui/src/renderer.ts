import { Image, Markdown, getCapabilities, renderImage, stripTerminalSequences, truncateToWidth, visibleWidth, wrapTextWithAnsi } from '@earendil-works/pi-tui';
import { introDuration, websiteMark } from './brand.ts';
import { introFrame, introPng } from './intro.ts';
import { drawPanel, permissionLines, type PanelView } from './panels.ts';
import type {Scene} from './scenes.ts';
import {statusText,activity,type StatusEntry} from './chrome.ts';
import {diffLines,type FileDiff} from './transcript.ts';
import {paperRow,paperColors,paperGraphic} from './paper.ts';

export interface Rect { x: number; y: number; width: number; height: number }
export interface Attachment { id: number; label: string; thumbnail: string; width: number; height: number }
export interface View {
  id: number; width: number; height: number; version: string; cwd: string;
  model: string; provider: string; branch: string; indexed: boolean; indexing: boolean;
  theme: string; accent: string; density: string; session: string | null; elapsed: number; profile: string; autoApprove: boolean; planMode: boolean;
  logs: { text: string; level: string }[]; plan: { text: string; done: boolean; active: boolean }[];
  diffs: FileDiff[];
  input: string; cursor: number; selection: [number, number] | null; masked: boolean;
  inputMode: string; attachments: Attachment[]; running: boolean; phase: string; tool: string | null;
  followTail: boolean; scroll: number; stream: string; permission: string | null; update: string | null;
  voice: string | null; commands: { cmd: string; desc: string }[]; selectedCommand: number; tokens: number;
  introElapsed?: number;
  introScene?: Scene;
  introBright?: boolean;
  panel?: PanelView | null;
  approval?: {tool:string;args:string;selected?:number} | null;
  statusLine?: string[];
  statusEntries?: StatusEntry[];
  runElapsed?:number;
  expandTools?:boolean;
  shortcuts?: Record<string,string>;
  preferences?: {status_fields:string[]; reduce_motion:boolean; ambient_art:boolean};
}
export interface Rendered {
  id: number; rows: string[]; input: Rect; execution: Rect; cursor: { x: number; y: number }; cursorVisible?:boolean;
  editorWidth: number;
  animate: boolean;
  artwork: Rect | null;
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
  const fg = light ? rgb(35, 35, 35) : rgb(233, 229, 220);
  const dim = light ? rgb(95, 95, 95) : rgb(145, 145, 145);
  const border = light ? rgb(190, 190, 190) : rgb(65, 65, 65);
  const accents = light
    ? { blue: rgb(20,105,210), green: rgb(20,135,80), violet: rgb(120,85,210), rose: rgb(190,55,90), orange: rgb(210,105,20) }
    : { blue: rgb(95,175,255), green: rgb(70,190,120), violet: rgb(185,140,255), rose: rgb(255,110,145), orange: rgb(235,155,105) };
  const accent = accents[v.accent as keyof typeof accents] ?? accents.orange;
  const bg = light ? '\x1b[48;2;240;240;240m' : '\x1b[48;2;30;30;30m';
  const paper=paperColors(light);
  const paperGraphics=getCapabilities().images==='kitty';
  const tint = (color: string) => (s: string) => color + s + reset;
  const markdown = (text: string, n: number) => new Markdown(clean(text), 0, 0, {
    heading: s => fg + bold(s) + reset, link: tint(accent), linkUrl: tint(dim), code: tint(accent),
    codeBlock: tint(fg), codeBlockBorder: tint(border), quote: tint(dim), quoteBorder: tint(border),
    hr: tint(border), listBullet: tint(accent), bold, italic: s => `\x1b[3m${s}\x1b[23m`,
    strikethrough: s => `\x1b[9m${s}\x1b[29m`, underline: s => `\x1b[4m${s}\x1b[24m`,
  }, { color: tint(fg) }).render(Math.max(1, n));
  const rows = Array<string>(h).fill('');
  const put = (y: number, text: string, x = gutter, n = width) => {
    if (y >= 0 && y < h) rows[y] = ' '.repeat(Math.min(x, w - 1)) + clip(text, Math.min(n, w - x));
  };
  const headerY = h < 15 || v.density === 'compact' ? 0 : v.density === 'spacious' ? 2 : 1;
  const headerHeight = h >= 15 && v.density !== 'compact' ? (v.density === 'spacious' ? 5 : 4) : h >= 8 ? 2 : 0;
  const header = [
    fg + bold('osmogrep') + reset + dim + ` v${clean(v.version)}` + (v.session ? ` · ${clean(v.session)}` : '') + reset,
    dim + clean(`${v.model} · ${v.provider}`) + reset,
    dim + clean(v.cwd) + (v.branch ? ` · ${clean(v.branch)}` : '') + (v.indexing ? ' · indexing…' : v.indexed ? ' · indexed ✓' : '') + reset,
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
  const previewHeight = attached.length && h >= 20 ? 5 : 0;
  const visible = Math.min(input.lines.length, Math.max(1, Math.min(8, Math.floor(h / 3) - previewHeight)));
  const verticalPad = h >= 8 ? 1 : 0;
  const composerHeight = Math.min(h, visible + verticalPad * 2 + previewHeight);
  const footerHeight = h > composerHeight + headerHeight + headerY + 1 ? 2 : 0;
  const composerY = Math.max(0, h - footerHeight - composerHeight);
  const inputRect = { x: gutter, y: composerY, width, height: composerHeight };
  const execution = { x: gutter, y: headerY + headerHeight, width, height: Math.max(0, composerY - headerY - headerHeight - (v.running || v.permission || v.update || v.voice ? 1 : 0)) };
  const approvalRows=v.approval?permissionLines(v.approval.tool,v.approval.args,v.approval.selected??0,width,accent,light):[];
  const approvalHeight=Math.min(approvalRows.length,Math.max(0,Math.floor(execution.height*.65)));
  execution.height=Math.max(0,execution.height-(approvalHeight?approvalHeight+1:0));
  const transcript: string[] = [];
  let paragraph: string[] = [];
  const flush = () => { if (paragraph.length) { transcript.push(...markdown(paragraph.join('\n'), width - pad * 2)); paragraph = []; } };
  for (let i = 0; i < v.logs.length; i++) {
    const entry = v.logs[i]!; const text = clean(entry.text);
    if (text.startsWith('USER|')) {
      flush(); let command = text.slice(5), prefix = '❯ ';
      if (v.logs[i + 1]?.text === 'SHELL : $ ' + command) { i++; prefix = '$ '; }
      transcript.push('', ...wrapTextWithAnsi(accent + prefix + reset + fg + bold(command) + reset, Math.max(1, width - pad * 2)), '');
    } else if (text.trimStart().startsWith('└ ')) {
      flush();const results=[text];
      while(v.logs[i+1]?.text.trimStart().startsWith('└ '))results.push(clean(v.logs[++i]!.text));
      const shown=v.expandTools?results:results.slice(0,3);
      shown.forEach(line=>transcript.push(...wrapTextWithAnsi(dim+line+reset,Math.max(1,width-pad*2))));
      if(shown.length<results.length)transcript.push(dim+`  … ${results.length-shown.length} more lines · ctrl+t expand`+reset);
    } else if (text.startsWith('● ') || text.trimStart().startsWith('└ ') || text.startsWith('· ') || entry.level !== 'Info') {
      flush();
      const color = entry.level === 'Error' ? rgb(230, 100, 100) : entry.level === 'Warn' ? rgb(225, 185, 75) : text.startsWith('● ') ? accent : dim;
      transcript.push(...wrapTextWithAnsi(color + text + reset, Math.max(1, width - pad * 2)));
    } else { paragraph.push(text); }
  }
  for(const diff of v.diffs)transcript.push(...diffLines(diff,Math.max(1,width-pad*2),light,!!v.expandTools));
  flush();
  if(v.stream)transcript.push(...markdown(v.stream+'▍',width-pad*2));
  if (v.plan.length) transcript.push('', ...v.plan.map(p => `${p.done ? '✓' : p.active ? '›' : '·'} ${clean(p.text)}`));
  const maxScroll = Math.max(0, transcript.length - execution.height);
  const start = v.followTail ? maxScroll : Math.max(0, maxScroll - v.scroll);
  transcript.slice(start, start + execution.height).forEach((line, i) => put(execution.y + i, line, gutter + pad, width - pad * 2));

  let artwork: Rect | null = null;
  const images: Rendered['images'] = [];
  const introElapsed = v.preferences?.reduce_motion ? introDuration : v.introElapsed ?? v.elapsed;
  if (v.preferences?.ambient_art!==false && !v.panel && !value && !v.masked && !transcript.length && !v.running && !v.permission && !v.update && !v.voice && !v.commands.length
    && width >= 36 && execution.height >= 16) {
    const height = Math.min(30, execution.height-2), columns = Math.min(64, height*2, width-4);
    artwork = {x:gutter+Math.floor((width-columns)/2),y:execution.y+Math.floor((execution.height-height)/2),width:columns,height};
    if (getCapabilities().images === 'kitty') {
      const graphicId = (process.pid*128+127)>>>0;
      const png=introPng(introElapsed,columns,height,light,v.introScene,v.introBright).toString('base64');
      const image=renderImage(png,{widthPx:columns*6,heightPx:height*12},{imageId:graphicId,maxWidthCells:columns,maxHeightCells:height,moveCursor:false});
      if(image) images.push({id:graphicId,x:artwork.x,y:artwork.y,sequence:image.sequence});
    } else {
      introFrame(introElapsed,columns,height,light,v.introScene,v.introBright).forEach((line,i)=>put(artwork!.y+i,line,artwork!.x,columns));
    }
  }

  const status = v.update ?? v.voice ?? (v.permission&&!v.approval?v.permission:null) ?? (v.running ? activity(v.runElapsed??v.elapsed,v.approval?'Waiting for permission':v.phase,v.approval||v.phase==='thinking'?null:v.tool,!!v.preferences?.reduce_motion) : '');
  if(status){const action=v.running?'esc interrupt':'';const left=clip(status,Math.max(1,width-action.length-4));put(composerY-1,accent+left+reset+(width>action.length+visibleWidth(left)+3?' '.repeat(width-visibleWidth(left)-action.length)+dim+action+reset:''));}
  if(approvalHeight){const y=composerY-approvalHeight-2;approvalRows.slice(0,approvalHeight).forEach((line,i)=>{const content=clip(line,width-2);put(y+i,bg+content+' '.repeat(Math.max(0,width-visibleWidth(content)))+reset);});}
  for (let y = composerY; y < composerY + composerHeight; y++) put(y, paperRow('',width,y-composerY,composerHeight,light,paperGraphics));
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
    const permission = v.planMode ? 'plan mode' : v.autoApprove ? 'auto-approve on' : 'ask approvals';
    const right = dim+(v.running?`${v.shortcuts?.queue??'tab'} queue`:'/help')+reset;
    const reserve=width>=60?visibleWidth(right)+3:0;
    const entries=v.statusEntries??(v.statusLine?v.statusLine.map((value,i)=>({id:value.startsWith('~')||value.startsWith('/')?'directory':i===0?'model':'optional',value})):[{id:'permissions',value:`${v.profile} · ${permission}`}]);
    const left=dim+statusText(entries,Math.max(1,width-pad*2-reserve))+reset;
    put(h-1,' '.repeat(pad)+left,width>=40?gutter:gutter,Math.max(1,width-pad));
    if(reserve)rows[h-1]+=' '.repeat(Math.max(1,width-pad*2-visibleWidth(left)-visibleWidth(right)))+right;

  }
  if (v.commands.length) {
    const bottom = composerY - (status ? 3 : 2);
    const available = Math.max(0, bottom - execution.y), chrome = available >= 10 ? 7 : 0;
    const count = Math.min(5, v.commands.length, Math.max(0, available - chrome));
    const selected = Math.max(0, Math.min(v.commands.length - 1, v.selectedCommand));
    const first = Math.max(0, Math.min(selected - Math.floor(count / 2), v.commands.length - count));
    const menuWidth = Math.max(1, Math.min(78, width - pad * 2 - 2)), menuY = bottom - count - chrome;
    const surface = light ? '\x1b[48;2;240;240;240m' : '\x1b[48;2;22;22;22m';
    const paint = (y: number, text: string) => {
      const line = clip(text, Math.max(1,menuWidth-3));
      put(y, surface + line + ' '.repeat(Math.max(0, menuWidth - visibleWidth(line))) + reset, gutter + pad, menuWidth);
    };
    if (count && chrome) {
      const title = '   Commands / quick actions', position = `${selected + 1} · ${v.commands.length}`;
      paint(menuY, '');
      paint(menuY+1, dim + (menuWidth >= 28 ? title + ' '.repeat(Math.max(1, menuWidth - 3 - title.length - position.length)) + position : title));
      paint(menuY + 2, '');
      paint(bottom - 4, '');
      paint(bottom - 3, dim + '  ↑↓ move   [enter] select   [esc] dismiss');
      paint(bottom - 2, '');paint(bottom - 1, '');
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
  if (v.panel && !v.permission && !v.update) {
    const panel=drawPanel(v.panel,w,h,light,accent);
    return {id:v.id,rows:panel.rows,input:inputRect,execution,editorWidth:textWidth,animate:false,artwork:null,cursor:panel.cursor,cursorVisible:v.panel.section!==8,images:[]};
  }

  return {
    id: v.id, rows: rows.map(row => {
      const clipped = clip(row, w);
      return rootBackground + fg + clipped.replaceAll(reset, reset + rootBackground + fg)
        + ' '.repeat(Math.max(0, w - visibleWidth(clipped))) + reset;
    }), input: inputRect, execution, editorWidth: textWidth, animate: v.running || !!artwork && introElapsed < introDuration, artwork,
    cursor: { x: Math.min(w - 1, gutter + pad + 2 + input.position.x), y: Math.min(h - 1, textY + input.position.y - topLine) },
    images,
  };
}
