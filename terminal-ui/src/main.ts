import { createInterface } from 'node:readline';
import { render, type View } from './renderer.ts';

// The Rust process owns stdin, permissions, tools and terminal cleanup.
const lines = createInterface({ input: process.stdin, crlfDelay: Infinity });
lines.on('line', raw => {
  try { process.stdout.write(JSON.stringify(render(JSON.parse(raw) as View)) + '\n'); }
  catch { process.stdout.write(JSON.stringify({ error: 'Terminal renderer failed' }) + '\n'); }
});
