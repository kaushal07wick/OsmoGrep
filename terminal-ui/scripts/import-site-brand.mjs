// Offline logo import; running the UI never contacts the website.
import { readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';

const [homePath] = process.argv.slice(2);
if (!homePath) throw Error('Usage: node scripts/import-site-brand.mjs homepage.html');
const home = readFileSync(homePath, 'utf8');
const svg = home.match(/<svg[^>]*viewBox="0 0 11 10".*?<\/svg>/s)?.[0];
if (!svg) throw Error('Live pixel logo not found');
const logo = Array.from({ length: 10 }, () => Array(11).fill('.'));
for (const [, x, y, style] of svg.matchAll(/<rect x="(\d+)" y="(\d+)"[^>]*style="([^"]+)"/g)) {
  if (Number(y) >= 10 || Number(x) >= 11) throw Error('Logo pixel outside its viewBox');
  logo[Number(y)][Number(x)] = style.includes('--color-accent') ? 'a' : 'i';
}
if (!logo.some(row => row.includes('a'))) throw Error('Orange logo segment not found');
writeFileSync('brand/logo.svg', svg.replace('<svg ', '<svg xmlns="http://www.w3.org/2000/svg" style="--color-ink:#18181B;--color-accent:#EA580C" ') + '\n');
writeFileSync('src/site-art.ts', '// Generated from the live osmogrep.com assets; see brand/provenance.json.\n'
  + 'export const siteLogo = ' + JSON.stringify(logo.map(row => row.join(''))) + ' as const;\n');
writeFileSync('brand/provenance.json', JSON.stringify({
  source: 'https://osmogrep.com/', captured: new Date().toISOString(),
  homepage_sha256: createHash('sha256').update(home).digest('hex'),
  logo: 'Exact deployed 11×10 SVG pixel grid, adapted to light ink on black.',
  animation: 'Terminal adaptation: orange segment orbits the fixed header ring for ten seconds, then returns to its deployed position.',
}, null, 2) + '\n');
console.log('Imported the live pixel logo.');
