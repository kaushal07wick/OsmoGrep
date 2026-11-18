import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {createWriteStream} from 'node:fs';
import {access, chmod, lstat, mkdir, mkdtemp, readFile, rename, rm} from 'node:fs/promises';
import {homedir} from 'node:os';
import {join} from 'node:path';
import {Readable, Transform} from 'node:stream';
import {pipeline} from 'node:stream/promises';

const releases = 'https://github.com/kaushal07wick/OsmoGrep/releases/download';
const targets = {
  'darwin-arm64': 'aarch64-apple-darwin',
  'linux-x64': 'x86_64-unknown-linux-gnu',
  'win32-x64': 'x86_64-pc-windows-msvc',
};

export function targetFor(platform, arch) {
  const target = targets[`${platform}-${arch}`];
  if (!target) throw new Error(`No prebuilt OsmoGrep binary for ${platform}/${arch}. Install from source with Cargo instead.`);
  return target;
}

export function checksumFor(text, asset) {
  for (const line of text.trim().split(/\r?\n/)) {
    const [hash, name] = line.trim().split(/\s+/);
    if (/^[a-f\d]{64}$/i.test(hash ?? '') && (!name || name.replace(/^\*/, '') === asset)) return hash.toLowerCase();
  }
  throw new Error(`Release checksum does not identify ${asset}`);
}

async function download(url, file, maxBytes, fetchImpl) {
  for (let attempt = 0; ; attempt++) {
    try {
      const response = await fetchImpl(url, {signal: AbortSignal.timeout(180_000)});
      if (!response.ok) {
        await response.body?.cancel();
        const error = new Error(`Download returned HTTP ${response.status}: ${url}`);
        error.retryable = response.status >= 500 || [408, 429].includes(response.status);
        throw error;
      }
      if (!response.body) throw new Error('Release download has no body');
      let received = 0;
      const limit = new Transform({transform(chunk, encoding, done) {
        received += chunk.length;
        done(received > maxBytes ? new Error('Release download exceeds its size limit') : null, chunk);
      }});
      await pipeline(Readable.fromWeb(response.body), limit, createWriteStream(file, {mode: 0o600}));
      return;
    } catch (error) {
      if (attempt >= 2 || error.retryable === false || /size limit/.test(error.message)) throw error;
      await new Promise(resolve => setTimeout(resolve, 500 * (attempt + 1)));
    }
  }
}

export function extractBinary(archive, directory, name) {
  // Extract only the release's named executable; archive paths cannot choose another destination.
  const result = spawnSync('tar', ['-xf', archive, '-C', directory, '--', name], {encoding: 'utf8', windowsHide: true, timeout: 30_000});
  if (result.error || result.status !== 0) throw new Error(`Cannot unpack OsmoGrep: ${result.error?.message ?? result.stderr}. Ensure tar is available.`);
}

export async function ensureBinary({version, platform = process.platform, arch = process.arch,
  cacheRoot = process.env.LOCALAPPDATA && platform === 'win32' ? process.env.LOCALAPPDATA : process.env.XDG_CACHE_HOME ?? join(homedir(), '.cache'),
  fetchImpl = fetch, extract = extractBinary, onDownload = () => {}}) {
  if (!/^\d+\.\d+\.\d+(?:-[a-z\d.-]+)?$/i.test(version)) throw new Error('Invalid OsmoGrep package version');
  const target = targetFor(platform, arch);
  const name = platform === 'win32' ? 'osmogrep.exe' : 'osmogrep';
  const directory = join(cacheRoot, 'osmogrep', 'npm', version, target);
  const binary = join(directory, name);
  try {
    const cached = await lstat(binary);
    if (cached.isFile() && !cached.isSymbolicLink()) {await access(binary, platform === 'win32' ? 0 : 1); return binary;}
  } catch (error) {
    if (!['ENOENT', 'EACCES'].includes(error.code)) throw error;
  }
  await mkdir(directory, {recursive: true, mode: 0o700});
  const temporary = await mkdtemp(join(directory, 'download-'));
  const asset = `osmogrep-${target}.${platform === 'win32' ? 'zip' : 'tar.gz'}`;
  const archive = join(temporary, asset), checksum = `${archive}.sha256`;
  try {
    onDownload(target);
    await download(`${releases}/v${version}/${asset}.sha256`, checksum, 4096, fetchImpl);
    const expected = checksumFor(await readFile(checksum, 'utf8'), asset);
    await download(`${releases}/v${version}/${asset}`, archive, 128 * 1024 * 1024, fetchImpl);
    if (createHash('sha256').update(await readFile(archive)).digest('hex') !== expected) throw new Error('OsmoGrep download checksum mismatch; nothing was installed.');
    extract(archive, temporary, name);
    const candidate = join(temporary, name), stat = await lstat(candidate);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.nlink !== 1) throw new Error('Release executable must be a regular file without links');
    await chmod(candidate, 0o755);
    try {await rename(candidate, binary);} catch (error) {
      if (error.code !== 'EEXIST' || !(await lstat(binary)).isFile()) throw error;
    }
    return binary;
  } finally {
    await rm(temporary, {recursive: true, force: true});
  }
}
