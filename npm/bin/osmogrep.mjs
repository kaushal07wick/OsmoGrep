#!/usr/bin/env node
import {spawn} from 'node:child_process';
import {readFile} from 'node:fs/promises';
import {delimiter, dirname} from 'node:path';
import {ensureBinary} from '../native.mjs';

try {
  const {version} = JSON.parse(await readFile(new URL('../package.json', import.meta.url), 'utf8'));
  if (process.platform === 'linux' && !process.report.getReport().header.glibcVersionRuntime) {
    throw new Error('The Linux prebuilt binary requires glibc. Build OsmoGrep with Cargo on musl/Alpine.');
  }
  const binary = await ensureBinary({version, onDownload: target => {
    process.stderr.write(`Downloading OsmoGrep ${version} for ${target} (first launch only)…\n`);
  }});
  const child = spawn(binary, process.argv.slice(2), {stdio: 'inherit', env: {
    ...process.env, PATH: dirname(process.execPath) + delimiter + (process.env.PATH ?? ''),
  }});
  // The terminal delivers Ctrl+C to both processes; let Rust restore its terminal before exiting.
  const interrupt = () => {}, terminate = () => child.kill('SIGTERM');
  process.on('SIGINT', interrupt);
  process.on('SIGTERM', terminate);
  child.once('error', error => {process.stderr.write(`osmogrep: ${error.message}\n`); process.exitCode = 1;});
  child.once('exit', (code, signal) => {
    process.removeListener('SIGINT', interrupt);
    process.removeListener('SIGTERM', terminate);
    if (signal) process.kill(process.pid, signal);
    else process.exitCode = code ?? 1;
  });
} catch (error) {
  process.stderr.write(`osmogrep: ${error.message}\n`);
  process.exitCode = 1;
}
