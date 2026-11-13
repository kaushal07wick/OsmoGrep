import { build } from 'esbuild';
import { readFileSync } from 'node:fs';
const licenses = readFileSync(new URL('./bundle/THIRD_PARTY_LICENSES.txt', import.meta.url), 'utf8');
await build({
  entryPoints: ['src/main.ts'], bundle: true, platform: 'node', format: 'esm', target: 'node22',
  minify: true, outfile: 'bundle/renderer.mjs', legalComments: 'eof',
  banner: { js: '/*!\n' + licenses.replaceAll('*/', '* /') + '\n*/' },
});
