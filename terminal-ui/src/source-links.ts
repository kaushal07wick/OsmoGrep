import { sliceByColumn, stripTerminalSequences, visibleWidth } from '@earendil-works/pi-tui';
export function sourceLinks(row: string, light: boolean) {
    const plain = stripTerminalSequences(row), links: {
        label: string;
        x: number;
        width: number;
    }[] = [];
    // Exclude URLs, drive prefixes are accepted; Rust validates the workspace path again.
    for (const m of plain.matchAll(/(?:^|[\s("'`])((?:[A-Za-z]:[\\/])?[\w.@~/-]+\.[\w+-]+):(\d+)(?::(\d+))?/g)) {
        const path = m[1]!, line = Number(m[2]);
        if (!Number.isSafeInteger(line) || line < 1 || line > 4294967295)
            continue;
        const start = m.index! + m[0].indexOf(path), x = visibleWidth(plain.slice(0, start));
        links.push({ label: JSON.stringify({ path, line }), x, width: visibleWidth(m[0].trimStart().replace(/^[('"`]/, '')) });
    }
    let text = '', at = 0;
    const blue = light ? '\x1b[38;2;38;103;175m' : '\x1b[38;2;112;166;224m';
    for (const link of links) {
        text += sliceByColumn(row, at, link.x - at) + blue + stripTerminalSequences(sliceByColumn(row, link.x, link.width)) + '\x1b[39m';
        at = link.x + link.width;
    }
    return { text: links.length ? text + sliceByColumn(row, at, Math.max(0, visibleWidth(row) - at)) : row, links };
}
