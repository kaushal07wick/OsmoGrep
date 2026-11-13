import { Image, Markdown, stripTerminalSequences, truncateToWidth, visibleWidth, wrapTextWithAnsi } from '@earendil-works/pi-tui';

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
}
export interface Rendered {
  id: number; rows: string[]; input: Rect; execution: Rect; cursor: { x: number; y: number };
  editorWidth: number;
  images: { id: number; x: number; y: number; sequence: string }[];
}
const reset = '\x1b[0m';
const rgb = (r: number, g: number, b: number) => `\x1b[38;2;${r};${g};${b}m`;
const bold = (s: string) => `\x1b[1m${s}\x1b[22m`;
export function clean(s: string): string {
  return stripTerminalSequences(s).replace(/[\x00-\x08\x0b-\x1f\x7f-\x9f]/g, '').replace(/\t/g, '    ');
}
const clip = (s: string, width: number) => width > 0 ? truncateToWidth(s, width, '') : '';

export function orb(angle: number, columns = 30, rows = 13): string[] {
  const depth = new Float64Array(columns * rows).fill(-Infinity);
  const pixels = Array<string>(columns * rows).fill(' ');
  for (let a = 0; a < Math.PI * 2; a += .055) {
    for (let b = 0; b < Math.PI * 2; b += .12) {
      const radius = 1.6 + .58 * Math.cos(b);
      const x = radius * Math.cos(a), y = radius * Math.sin(a), z = .58 * Math.sin(b);
      const rx = x * Math.cos(angle) + z * Math.sin(angle);
      const rz = z * Math.cos(angle) - x * Math.sin(angle);
      const ry = y * .65 - rz * .76, zz = y * .76 + rz * .65;
      const scale = 1 / (5 - zz);
      const px = Math.round(columns / 2 + rx * scale * columns * .85);
      const py = Math.round(rows / 2 + ry * scale * rows * .85);
      const index = py * columns + px;
      if (px < 0 || px >= columns || py < 0 || py >= rows || zz <= depth[index]!) continue;
      depth[index] = zz;
      const light = Math.max(0, Math.min(1, (zz + 2) / 4));
      pixels[index] = rgb(Math.round(55 + light * 170), Math.round(85 + light * 150), Math.round(125 + light * 125)) + '·' + reset;
    }
  }
  return Array.from({ length: rows }, (_, y) => pixels.slice(y * columns, (y + 1) * columns).join(''));
}

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
  const fg = light ? rgb(35, 35, 35) : rgb(220, 220, 220);
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
    fg + bold('OsmoGrep') + reset + dim + ` v${clean(v.version)}` + (v.session ? ` · ${clean(v.session)}` : '') + reset,
    dim + clean(`${v.model} · ${v.provider}`) + reset,
    dim + clean(v.cwd) + (v.branch ? ` · ${clean(v.branch)}` : '') + (v.indexing ? ' · indexing…' : v.indexed ? ' · indexed ✓' : '') + reset,
  ];
  const mark = [' ▄██▄  ', '██  ██ ', ' ▀██▀╲ '];
  if (headerHeight >= 4 && width >= 40) {
    header.forEach((line, i) => put(headerY + i, accent + mark[i] + reset + '  ' + line));
  } else if (headerHeight) {
    put(headerY, header[0]!); put(headerY + 1, dim + clean(v.cwd) + reset);
  }

  const pad = width >= 20 ? 1 : 0, textWidth = Math.max(1, width - pad * 2 - 2);
  const value = clean(v.input);
  const input = inputLines(value, Math.min(v.cursor, Array.from(value).length), textWidth);
  const attached = v.attachments.filter(a => value.includes(a.label));
  const previewHeight = attached.length && h >= 20 ? 5 : 0;
  const visible = Math.min(input.lines.length, Math.max(1, Math.min(8, Math.floor(h / 3) - previewHeight)));
  const composerHeight = Math.min(h, visible + 2 + previewHeight);
  const footerHeight = h > composerHeight + headerHeight + headerY ? 1 : 0;
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

  if (!v.logs.length && !value && !v.running && execution.height >= 17 && width >= 64) {
    const picture = orb(Math.min(v.elapsed, 2200) / 900);
    const y = execution.y + Math.floor((execution.height - picture.length) / 2);
    const x = gutter + Math.floor((width - 30) / 2);
    picture.forEach((line, i) => put(y + i, line, x, 30));
  }
  const status = v.permission ?? v.update ?? v.voice ?? (v.running ? `${['✻', '✼', '✶', '✳'][Math.floor(v.elapsed / 130) % 4]} ${v.tool ?? v.phase}…  esc to interrupt` : '');
  if (status) put(composerY - 1, accent + clean(status) + reset);
  for (let y = composerY; y < composerY + composerHeight; y++) put(y, bg + ' '.repeat(width) + reset);
  put(composerY, border + '─'.repeat(width) + reset);
  if (composerHeight >= 2) put(composerY + composerHeight - 1, border + '─'.repeat(width) + reset);
  const images: Rendered['images'] = [];
  if (previewHeight) {
    let x = gutter + pad + 2;
    for (const a of attached) {
      if (x + 16 > gutter + width) break;
      const graphicId = (process.pid * 128 + a.id) >>> 0;
      const image = new Image(a.thumbnail, 'image/png', { fallbackColor: tint(dim) }, { imageId: graphicId, maxWidthCells: 14, maxHeightCells: 4 });
      const picture = image.render(16);
      if (picture.some(s => s.includes('\x1b_G') || s.includes('\x1b]1337;'))) {
        images.push({ id: graphicId, x, y: composerY + 1, sequence: picture.join('') });
      } else { put(composerY + 2, bg + dim + a.label + reset, x, 16); }
      x += 18;
    }
  }
  const topLine = Math.max(0, input.position.y - visible + 1);
  const textY = composerY + 1 + previewHeight;
  for (let i = 0; i < visible && textY + i < composerY + composerHeight - 1; i++) {
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
    const content = ' '.repeat(pad) + fg + prefix + styled;
    put(textY + i, bg + content + ' '.repeat(Math.max(0, width - visibleWidth(content))) + reset);
  }
  if (footerHeight) {
    const permission = v.planMode ? 'plan mode' : v.autoApprove ? 'auto-approve on' : 'ask approvals';
    const left = accent + `${v.profile} · ${permission}` + reset + dim + ' · /help commands' + reset;
    const right = dim + (v.running ? 'tab queue · esc cancel' : '! shell · shift+enter newline') + reset;
    put(h - 1, left);
    if (width >= visibleWidth(left) + visibleWidth(right) + 4) {
      rows[h - 1] = rows[h - 1]! + ' '.repeat(width - visibleWidth(left) - visibleWidth(right)) + right;
    }
  }
  if (v.commands.length) {
    const count = Math.min(10, v.commands.length, Math.max(0, composerY - execution.y - 2));
    const first = Math.max(0, v.selectedCommand - count + 1);
    for (let i = 0; i < count; i++) {
      const index = first + i, item = v.commands[index]!;
      const text = clean(item.cmd).padEnd(18) + clean(item.desc);
      put(composerY - count - 1 + i, (index === v.selectedCommand ? '\x1b[7m' : dim) + clip(text, Math.min(80, width)) + reset);
    }
  }
  const rootBackground = '\x1b[48;2;250;250;250m';
  return {
    id: v.id, rows: rows.map(row => {
      const clipped = clip(row, w);
      return light ? rootBackground + fg + clipped.replaceAll(reset, reset + rootBackground + fg)
        + ' '.repeat(Math.max(0, w - visibleWidth(clipped))) + reset : clipped;
    }), input: inputRect, execution, editorWidth: textWidth,
    cursor: { x: Math.min(w - 1, gutter + pad + 2 + input.position.x), y: Math.min(h - 1, textY + input.position.y - topLine) },
    images,
  };
}
