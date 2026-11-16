import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {inflateSync} from 'node:zlib';
import {orb} from './activity.ts';

test('all 32 frames preserve the exact approved original orb pixels', () => {
  const hash = createHash('sha256');
  for (let frame = 0; frame < 32; frame++) {
    const png = orb(frame * 125).png;
    const chunks: Buffer[] = [];
    for (let offset = 8; offset < png.length;) {
      const length = png.readUInt32BE(offset);
      if (png.toString('ascii', offset + 4, offset + 8) === 'IDAT') chunks.push(png.subarray(offset + 8, offset + 8 + length));
      offset += length + 12;
    }
    hash.update(inflateSync(Buffer.concat(chunks)));
  }
  // Captured from the original preview explicitly approved on November 15.
  assert.equal(hash.digest('hex'), 'a3b43887add5d637c4e698941af685c9d444dbb24d50aada92f4f1e521c763dd');
  assert.strictEqual(orb(4000), orb(0));
  assert.strictEqual(orb(2000, false, true), orb(0));
});
