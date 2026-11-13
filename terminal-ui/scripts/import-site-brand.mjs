// Offline asset conversion, not part of the running UI. Inputs are downloaded live-site assets.
import { readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { gzipSync } from 'node:zlib';
import vm from 'node:vm';
import { transform } from 'esbuild';

const [homePath, scenePath, cssPath] = process.argv.slice(2);
if (!cssPath) throw Error('Usage: node scripts/import-site-brand.mjs homepage.html page-chunk.js stylesheet.css');
const home = readFileSync(homePath, 'utf8'), source = readFileSync(scenePath, 'utf8'), css = readFileSync(cssPath, 'utf8');
const svg = home.match(/<svg[^>]*viewBox="0 0 11 10".*?<\/svg>/s)?.[0];
if (!svg) throw Error('Live pixel logo not found');
const logo = Array.from({ length: 10 }, () => Array(11).fill('.'));
for (const [, x, y, style] of svg.matchAll(/<rect x="(\d+)" y="(\d+)"[^>]*style="([^"]+)"/g)) {
  logo[Number(y)][Number(x)] = style.includes('--color-accent') ? 'a' : 'i';
}
writeFileSync('brand/logo.svg', svg.replace('<svg ', '<svg xmlns="http://www.w3.org/2000/svg" style="--color-ink:#18181B;--color-accent:#EA580C" ') + '\n');

// shortcut: recognizes the current deployed, pure-math hero closure; review if its structure changes.
const readable = (await transform(source, { loader: 'js', minify: false })).code;
const start = readable.indexOf('let r = (function() {');
const end = readable.indexOf('}, draw: function(', start);
if (start < 0 || end < 0) throw Error('Live hero structure changed; inspect the deployed animation');
let code = readable.slice(start, end);
if (/\b(?:document|window|process|require|fetch|eval)\b/.test(code)) throw Error('Hero extraction is not pure drawing math');
const result = code.lastIndexOf('c2;');
code = code.slice(0, result) + '({density:t2,colors:s2,glyphs:a2,overlay:n2,overlayColors:r2,bug:i2,sprites:{walk:[R,S,R,O],scared:[L,T],caught:[B,H]}});' + code.slice(result + 3);
code += '} }; })();globalThis.engine=r;';
const context = vm.createContext(Object.create(null), { codeGeneration: { strings: false, wasm: false } });
new vm.Script(code).runInContext(context, { timeout: 3000 });

const width = 64, height = 28, fps = 8, frames = [], shades = ' .:-=+*#%@';
for (let frame = 0; frame < fps * 10; frame++) {
  const f = context.engine.frame(frame / fps, null);
  const chars = Array(width * height).fill(' '), colors = Array(width * height).fill('0');
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    let sum = 0, strongest = 0, color = 0, glyph = 0;
    for (let sy = 0; sy < 3; sy++) for (let sx = 0; sx < 3; sx++) {
      const px = Math.min(607, Math.floor((x + (sx + .5) / 3) * 608 / width));
      const py = Math.min(303, Math.floor((y + (sy + .5) / 3) * 304 / height));
      const index = py * 608 + px, value = f.density[index];
      sum += value;
      if (value > strongest) { strongest = value; color = f.colors[index]; glyph = f.glyphs[index]; }
    }
    const level = Math.min(9, Math.floor(sum / 9 * 10));
    const at = y * width + x;
    chars[at] = glyph && level > 2 ? String.fromCharCode(glyph) : shades[level];
    colors[at] = chars[at] === ' ' ? '0' : String(color);
  }
  // The site's separately drawn pixel bug follows the same exported pose/timing.
  if (f.bug?.sc > .03) {
    const b = f.bug, sprites = f.sprites[b.mood], sprite = sprites[Math.floor(b.t / (b.held ? .22 : b.mood === 'scared' ? .16 : .3)) % sprites.length];
    for (let sy = 0; sy < sprite.length; sy++) for (let sx = 0; sx < sprite[sy].length; sx++) {
      const ink = sprite[sy][sx]; if (!'odkw'.includes(ink)) continue;
      let dx = (sx - (b.held ? 2.4 : 7)) * .235 * b.sc, dy = (sy - (b.held ? 5.4 : 5)) * .235 * b.sc;
      if (b.held) [dx, dy] = [dx * Math.cos(b.angle) - dy * Math.sin(b.angle), dx * Math.sin(b.angle) + dy * Math.cos(b.angle)];
      const x = Math.round(((b.held ? b.gripX : b.x) + dx) / 45.6 * width);
      const y = Math.round(((b.held ? b.gripY : b.y) + dy) / 38 * height);
      if (x < 0 || x >= width || y < 0 || y >= height) continue;
      chars[y * width + x] = ink === 'k' ? '·' : '█'; colors[y * width + x] = ink === 'o' ? '3' : ink === 'd' ? '7' : ink === 'w' ? '4' : '1';
    }
  }
  for (let y = 0; y < 38; y++) for (let x = 0; x < 76; x++) {
    let text = f.overlay[y * 76 + x]; if (!text || text === ' ') continue;
    const color = String(f.overlayColors[y * 76 + x]), startX = x;
    while(x+1<76 && f.overlay[y*76+x+1] && f.overlay[y*76+x+1]!==' ') text += f.overlay[y*76+(++x)];
    const at = Math.floor(y * height / 38) * width + Math.min(width-text.length, Math.floor(startX * width / 76));
    for(let i=0;i<text.length;i++){chars[at+i]=text[i];colors[at+i]=color;}
  }
  frames.push([chars.join(''), colors.join('')]);
}
const sha = text => createHash('sha256').update(text).digest('hex');
writeFileSync('src/site-art.ts', '// Generated from the live osmogrep.com assets; see brand/provenance.json.\n'
  + 'export const siteLogo = ' + JSON.stringify(logo.map(row => row.join(''))) + ' as const;\n'
  + 'export const siteScene = ' + JSON.stringify({width,height,fps,data:gzipSync(JSON.stringify(frames)).toString('base64')}) + ' as const;\n');
writeFileSync('brand/provenance.json', JSON.stringify({
  source:'https://osmogrep.com/', captured:new Date().toISOString(),
  scene:'Woodpecker taps a tree, finds a bug and catches it; sampled from the deployed hero geometry.',
  homepage_sha256:sha(home), scene_chunk_sha256:sha(source), stylesheet_sha256:sha(css),
  logo:'Exact deployed 11×10 SVG pixel grid, adapted to light ink on black.',
  texture:'Engraved feather and woodgrain densities sampled from the same live scene.',
  terminal_sampling:{width,height,fps,frames:frames.length},
}, null, 2)+'\n');
console.log(`Imported the live pixel logo and ${frames.length} engraved animation frames.`);
