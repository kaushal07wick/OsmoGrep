import { test } from 'node:test';
import assert from 'node:assert/strict';
import { sourceLinks } from './source-links.ts';
import { stripTerminalSequences } from '@earendil-works/pi-tui';
test('source links preserve cell positions, ANSI and source coordinates', () => {
    const text = '🦀 \x1b[31msrc/main.rs:12:4\x1b[0m bad', result = sourceLinks(text, false);
    assert.equal(result.links.length, 1);
    assert.equal(result.links[0]!.x, 3);
    assert.deepEqual(JSON.parse(result.links[0]!.label), { path: 'src/main.rs', line: 12 });
    assert.equal(stripTerminalSequences(result.text), stripTerminalSequences(text));
    assert.ok(result.text.includes('112;166;224'));
    assert.equal(sourceLinks('https://localhost:4173/path and 12:30', false).links.length, 0);
    assert.equal(sourceLinks('x.rs:0 x.rs:99999999999999999', false).links.length, 0);
});
