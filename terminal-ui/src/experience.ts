import { sliceByColumn, stripTerminalSequences, truncateToWidth, visibleWidth, wrapTextWithAnsi } from '@earendil-works/pi-tui';
import type { Rendered } from './renderer.ts';
export interface WorkspaceView {
    page: string;
    tabs: string[];
    query: string;
    cursor: number;
    selected: number;
    rows: {
        id: string;
        label: string;
        detail: string;
        kind: string;
    }[];
    preview: string;
    preview_offset: number;
    notice: string;
    buttons: {
        id: string;
        label: string;
    }[];
    busy: boolean;
    editing: boolean;
}
const reset = '\x1b[0m', orange = '\x1b[38;2;235;155;105m', blue = '\x1b[38;2;112;166;224m';
const safe = (s: string) => stripTerminalSequences(s).replace(/[\x00-\x08\x0b-\x1f\x7f-\x9f]/g, '').replace(/\t/g, '    ');
export function workspaceFrame(id: number, p: WorkspaceView, w: number, h: number, light: boolean, reduced: boolean, elapsed: number): Rendered {
    const fg = light ? '\x1b[38;2;20;20;20m' : '\x1b[38;2;255;255;255m', dim = light ? '\x1b[38;2;100;100;100m' : '\x1b[38;2;145;145;145m';
    const rows = Array<string>(h).fill(''), controls: NonNullable<Rendered['controls']> = [];
    const margin = w >= 40 ? 2 : 0, inner = Math.max(1, w - 2 * margin);
    const put = (x: number, y: number, s: string, n = inner) => { if (y >= 0 && y < h && x < w) {
        const prefix = sliceByColumn(rows[y]!, 0, x);
        rows[y] = prefix + ' '.repeat(Math.max(0, x - visibleWidth(prefix))) + truncateToWidth(s, Math.max(1, Math.min(n, w - x)), '');
    } };
    const action = (x: number, y: number, label: string, choice: string, n = inner) => {
        const text = truncateToWidth(label, Math.max(1, Math.min(n, w - x)), '');
        put(x, y, orange + text + reset, n);
        if (y >= 0 && y < h && visibleWidth(text) > 0)
            controls.push({ id: 'workspace', label: choice, x, y, width: visibleWidth(text), height: 1 });
        return visibleWidth(text);
    };
    // Tabs wrap on small terminals and remain directly selectable.
    let x = margin, y = 0;
    for (const tab of p.tabs) {
        const size = Math.min(inner, visibleWidth(tab) + 2);
        if (x > margin && x + size > w - margin) {
            x = margin;
            y++;
        }
        action(x, y, `${tab.toLowerCase() === p.page ? '● ' : ''}${tab}`, `tab:${tab}`, size);
        x += size + 1;
    }
    const queryY = Math.min(h - 1, y + 2), label = p.editing ? 'Edit queued message' : 'Find';
    put(margin, queryY, dim + label + reset);
    const queryWidth = Math.max(1, inner - (p.editing ? 0 : 7)), queryX = p.editing ? margin : Math.min(w - 1, margin + 7);
    const queryLines = wrapTextWithAnsi(fg + safe(p.query) + reset, queryWidth);
    const queryRows = p.editing ? Math.min(Math.max(1, Math.floor(h / 3)), queryLines.length) : 1;
    const before = Array.from(p.query).slice(0, p.cursor).join('');
    const beforeLines = wrapTextWithAnsi(safe(before), queryWidth);
    const cursorRow = p.editing ? Math.min(queryRows - 1, beforeLines.length - 1) : 0;
    const queryStart = p.editing ? Math.max(0, beforeLines.length - queryRows) : 0;
    for (let i = 0; i < queryRows; i++)
        put(queryX, queryY + (p.editing ? 1 : 0) + i, p.editing ? (queryLines[queryStart + i] ?? '') : fg + sliceByColumn(safe(p.query), Math.max(0, visibleWidth(safe(before)) - queryWidth + 1), queryWidth) + reset, queryWidth);
    const top = queryY + queryRows + (p.editing ? 3 : 2), bottom = Math.max(top, h - 5), height = Math.max(0, bottom - top);
    const split = w >= 95 && !p.editing, listWidth = split ? Math.floor(inner * .43) : inner;
    const listHeight = split ? height : Math.min(7, Math.max(0, Math.floor(height * .4)));
    const offset = Math.max(0, Math.min(Math.max(0, p.rows.length - listHeight), p.selected - Math.floor(listHeight / 2)));
    for (let i = 0; i < listHeight; i++) {
        const row = p.rows[offset + i];
        if (!row)
            break;
        const active = offset + i === p.selected;
        const text = `${active ? '›' : ' '} ${safe(row.label)}`;
        put(margin, top + i, (active ? orange : fg) + truncateToWidth(text, listWidth, '…') + reset, listWidth);
        if (!p.editing)
            controls.push({ id: 'workspace', label: `select:${row.id}`, x: margin, y: top + i, width: listWidth, height: 1 });
    }
    const px = split ? margin + listWidth + 3 : margin, py = split ? top : top + listHeight + 1, pw = split ? Math.max(1, inner - listWidth - 3) : inner, ph = Math.max(0, bottom - py);
    const selected = p.rows[p.selected];
    if (selected)
        put(px, py - 1, dim + truncateToWidth(safe(selected.detail || selected.kind), pw, '…') + reset, pw);
    const lines = safe(p.preview).split('\n').flatMap(line => wrapTextWithAnsi(line, pw));
    const scroll = Math.min(p.preview_offset, Math.max(0, lines.length - ph));
    for (let i = 0; i < ph; i++) {
        const line = lines[scroll + i];
        if (line == null)
            break;
        const color = p.page === 'changes' ? (line.startsWith('+ ') ? '\x1b[38;2;100;200;140m' : line.startsWith('- ') ? '\x1b[38;2;225;125;125m' : fg) : fg;
        put(px, py + i, color + line + reset, pw);
    }
    let bx = margin, by = Math.max(0, h - 4);
    if (p.editing)
        put(margin, by, orange + 'Enter save · shift+enter newline · esc cancel' + reset);
    else
        for (const [i, button] of p.buttons.entries()) {
            let size = Math.min(inner, visibleWidth(button.label) + 2);
            if (bx > margin && bx + size > w - margin) {
                bx = margin;
                by++;
            }
            if (by >= h - 1)
                break;
            bx += action(bx, by, `${i + 1} ${button.label}`, button.id, size + 2) + 3;
        }
    if (p.notice)
        put(margin, h - 2, blue + safe(p.notice) + reset);
    put(margin, h - 1, dim + (p.busy ? `${reduced ? '·' : ['◐', '◓', '◑', '◒'][Math.floor(elapsed / 150) % 4]} Searching locally · ` : '') + '↑↓ select · alt+1… actions · pgup/pgdn preview · esc close' + reset);
    const root = light ? '\x1b[48;2;250;250;250m' : '\x1b[48;2;0;0;0m';
    const empty = { x: 0, y: 0, width: w, height: 0 };
    const cx = p.editing ? queryX + Math.min(queryWidth - 1, visibleWidth(beforeLines.at(-1) ?? '')) : queryX + Math.min(queryWidth - 1, visibleWidth(safe(before)));
    return { id, rows: rows.map(row => root + fg + row + ' '.repeat(Math.max(0, w - visibleWidth(row))) + reset), input: empty, execution: empty, editorWidth: Math.max(1, w), cursor: { x: Math.min(w - 1, cx), y: Math.min(h - 1, queryY + (p.editing ? 1 : 0) + cursorRow) }, cursorVisible: true, animate: p.busy && !reduced, animationIntervalMs: 150, artwork: null, images: [], controls, workspaceScroll: { requested: p.preview_offset, offset: scroll } };
}
