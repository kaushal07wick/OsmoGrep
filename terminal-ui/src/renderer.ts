import { Image, Markdown, getCapabilities, renderImage, stripTerminalSequences, truncateToWidth, visibleWidth, wrapTextWithAnsi } from '@earendil-works/pi-tui';
import { introDuration, websiteMark } from './brand.ts';
import { introFrame, introPng } from './intro.ts';
import { drawPanel, type PanelView } from './panels.ts';

export interface Rect { x: number; y: number; width: number; height: number }
export interface Attachment { id: number; label: string; thumbnail: string; width: number; height: number }
export interface View {
  id: number; width: number; height: number; version: string; cwd: string;
  model: string; provider: string; branch: string; indexed: boolean; indexing: boolean;
  theme: string; accent: string; density: string; session: string | null; elapsed: number; profile: string; autoApprove: boolean; planMode: boolean;
  logs: { text: string; level: string }[]; plan: { text: string; done: boolean; active: boolean }[];
  diffs: { file: string; lines: { text: string; kind: string }[] }[];
  input: string; cursor: number; selection: [number, number] | null; masked: boolean;
  inputMode: string; attachments: Attachment[]; running: boolean; phase: string; tool: string | null;
  followTail: boolean; scroll: number; stream: string; permission: string | null; update: string | null;
  voice: string | null; commands: { cmd: string; desc: string }[]; selectedCommand: number; tokens: number;
  introElapsed?: number;
  panel?: PanelView | null;
  approval?: {tool:string;args:string} | null;
  statusLine?: string[];
  shortcuts?: Record<string,string>;
  preferences?: {status_fields:string[]; reduce_motion:boolean; ambient_art:boolean};
}
export interface Rendered {
  id: number; rows: string[]; input: Rect; execution: Rect; cursor: { x: number; y: number };
  editorWidth: number;
  animate: boolean;
  artwork: Rect | null;
  images: { id: number; x: number; y: number; sequence: string }[];
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
  const transcript: string[] = [];
  let paragraph: string[] = [];
  const flush = () => { if (paragraph.length) { transcript.push(...markdown(paragraph.join('\n'), width - pad * 2)); paragraph = []; } };
  for (let i = 0; i < v.logs.length; i++) {
    const entry = v.logs[i]!; const text = clean(entry.text);
    if (text.startsWith('USER|')) {
      flush(); let command = text.slice(5), prefix = '❯ ';
      if (v.logs[i + 1]?.text === 'SHELL : $ ' + command) { i++; prefix = '$ '; }
      transcript.push('', ...wrapTextWithAnsi(accent + prefix + reset + fg + bold(command) + reset, Math.max(1, width - pad * 2)), '');
    } else if (text.startsWith('● ') || text.trimStart().startsWith('└ ') || text.startsWith('· ') || entry.level !== 'Info') {
      flush();
      const color = entry.level === 'Error' ? rgb(230, 100, 100) : entry.level === 'Warn' ? rgb(225, 185, 75) : text.startsWith('● ') ? accent : dim;
      transcript.push(...wrapTextWithAnsi(color + text + reset, Math.max(1, width - pad * 2)));
    } else { paragraph.push(text); }
  }
  flush();
  if (v.stream) transcript.push(...markdown(v.stream + '▍', width - pad * 2));
  for (const diff of v.diffs) {
    transcript.push('', accent + clean(diff.file) + reset);
    for (const line of diff.lines) {
      const color = line.kind === 'Added' ? rgb(70, 190, 120) : line.kind === 'Removed' ? rgb(230, 100, 100) : dim;
      const prefix = line.kind === 'Added' ? '+ ' : line.kind === 'Removed' ? '- ' : '  ';
      transcript.push(...wrapTextWithAnsi(color + prefix + clean(line.text) + reset, Math.max(1, width - pad * 2)));
    }
  }
  if (v.plan.length) transcript.push('', ...v.plan.map(p => `${p.done ? '✓' : p.active ? '›' : '·'} ${clean(p.text)}`));
  const maxScroll = Math.max(0, transcript.length - execution.height);
  const start = v.followTail ? maxScroll : Math.max(0, maxScroll - v.scroll);
  transcript.slice(start, start + execution.height).forEach((line, i) => put(execution.y + i, line, gutter + pad, width - pad * 2));

  let artwork: Rect | null = null;
  const images: Rendered['images'] = [];
  const introElapsed = v.introElapsed ?? v.elapsed;
  if (!value && !v.masked && !transcript.length && !v.running && !v.permission && !v.update && !v.voice && !v.commands.length
    && width >= 36 && execution.height >= 16) {
    const height = Math.min(30, execution.height-2), columns = Math.min(64, height*2, width-4);
    artwork = {x:gutter+Math.floor((width-columns)/2),y:execution.y+Math.floor((execution.height-height)/2),width:columns,height};
    if (getCapabilities().images === 'kitty') {
      const graphicId = (process.pid*128+127)>>>0;
      const png=introPng(introElapsed,columns,height,light).toString('base64');
      const image=renderImage(png,{widthPx:columns*6,heightPx:height*12},{imageId:graphicId,maxWidthCells:columns,maxHeightCells:height,moveCursor:false});
      if(image) images.push({id:graphicId,x:artwork.x,y:artwork.y,sequence:image.sequence});
    } else {
      introFrame(introElapsed,columns,height,light).forEach((line,i)=>put(artwork!.y+i,line,artwork!.x,columns));
    }
  }

  const status = v.permission ?? v.update ?? v.voice ?? (v.running ? `${['✻', '✼', '✶', '✳'][Math.floor(v.elapsed / 130) % 4]} ${v.tool ?? v.phase}…  esc to interrupt` : '');
  if (status) put(composerY - 1, accent + clean(status) + reset);
  for (let y = composerY; y < composerY + composerHeight; y++) put(y, bg + ' '.repeat(width) + reset);
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
    if (!value && !v.masked) styled = dim + 'Try "explain this codebase"' + fg;
    const prefix = index === 0 ? '❯ ' : '  ';
    const content = ' '.repeat(pad) + accent + prefix + fg + styled;
    put(textY + i, bg + content + ' '.repeat(Math.max(0, width - visibleWidth(content))) + reset);
  }
  if (footerHeight) {
    const permission = v.planMode ? 'plan mode' : v.autoApprove ? 'auto-approve on' : 'ask approvals';
    const left = accent + (v.statusLine ? v.statusLine.map(clean).join(' · ') : `${v.profile} · ${permission}`) + reset + dim + ' · /help' + reset;
    const right = dim + (v.running ? `${v.shortcuts?.queue??'tab'} queue · esc cancel` : `! shell · ${v.shortcuts?.newline??'shift+enter'} newline`) + reset;
    put(h - 1, left);
    if (width >= visibleWidth(left) + visibleWidth(right) + 4) {
      rows[h - 1] = rows[h - 1]! + ' '.repeat(width - visibleWidth(left) - visibleWidth(right)) + right;
    }
  }
  if (v.commands.length) {
    const bottom = composerY - (status ? 2 : 1);
    const available = Math.max(0, bottom - execution.y), chrome = available >= 5 ? 3 : 0;
    const count = Math.min(6, v.commands.length, Math.max(0, available - chrome));
    const selected = Math.max(0, Math.min(v.commands.length - 1, v.selectedCommand));
    const first = Math.max(0, Math.min(selected - Math.floor(count / 2), v.commands.length - count));
    const menuWidth = Math.max(1, Math.min(86, width - pad * 2)), menuY = bottom - count - chrome;
    const surface = light ? '\x1b[48;2;240;240;240m' : '\x1b[48;2;22;22;22m';
    const highlight = light ? '\x1b[48;2;222;222;222m' : '\x1b[48;2;42;42;42m';
    const paint = (y: number, text: string, selected = false) => {
      const line = clip(text, menuWidth);
      put(y, (selected ? highlight : surface) + line + ' '.repeat(Math.max(0, menuWidth - visibleWidth(line))) + reset, gutter + pad, menuWidth);
    };
    if (count && chrome) {
      const title = '  Commands', position = `${selected + 1} / ${v.commands.length}  `;
      paint(menuY, dim + (menuWidth >= 28 ? title + ' '.repeat(Math.max(1, menuWidth - title.length - position.length)) + position : title));
      paint(menuY + 1, '');
      paint(bottom - 1, dim + '  ↑↓ navigate · enter run · esc close');
    }
    const commandWidth = Math.min(26, Math.floor(menuWidth * .4), Math.max(...v.commands.map(item => visibleWidth(clean(item.cmd)))));
    for (let i = 0; i < count; i++) {
      const index = first + i, item = v.commands[index]!, active = index === selected;
      const command = clip(clean(item.cmd), commandWidth);
      const label = menuWidth >= 44 ? command + ' '.repeat(commandWidth - visibleWidth(command) + 3) : clean(item.cmd);
      const description = menuWidth >= 44 ? (active ? fg : dim) + clean(item.desc) : '';
      paint(menuY + (chrome ? 2 : 0) + i, (active ? accent + ' › ' + bold(label) : fg + '   ' + label) + description, active);
    }
  }
  const rootBackground = light ? '\x1b[48;2;250;250;250m' : '\x1b[48;2;0;0;0m';
  if (v.panel && !v.permission && !v.update) {
    const panel=drawPanel(v.panel,w,h,light,accent);
    return {id:v.id,rows:panel.rows,input:inputRect,execution,editorWidth:textWidth,animate:false,artwork:null,cursor:panel.cursor,images:[]};
  }
  if(v.approval){
    const panel=drawPanel({sections:['Permission required'],section:0,query:'',selected:0,editing:false,
      rows:[{label:'Y · Allow once',description:clean(v.approval.tool)+' · '+clean(v.approval.args),value:'one tool call'},
        {label:'N · Deny',description:'The agent receives the denial and can continue safely',value:'keep policy'},
        {label:'A · Auto-approve',description:'Allow dangerous tools for the rest of this session',value:'session-wide'}],
      notice:'Tool policy applies · Escape cancels the run',footer:'y allow once · n deny · a auto-approve · esc cancel'},w,h,light,accent);
    return {id:v.id,rows:panel.rows,input:inputRect,execution,editorWidth:textWidth,animate:false,artwork:null,cursor:panel.cursor,images:[]};
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
