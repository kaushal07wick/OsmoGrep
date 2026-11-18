import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {mkdtemp, readFile, readdir, rm} from 'node:fs/promises';
import {writeFileSync,linkSync,symlinkSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {checksumFor, ensureBinary, targetFor} from './native.mjs';

test('platform selection matches the three published Rust targets',()=>{
  assert.equal(targetFor('darwin','arm64'),'aarch64-apple-darwin');
  assert.throws(()=>targetFor('darwin','x64'),/Install from source/);
  assert.equal(targetFor('linux','x64'),'x86_64-unknown-linux-gnu');
  assert.equal(targetFor('win32','x64'),'x86_64-pc-windows-msvc');
  assert.throws(()=>targetFor('linux','arm64'),/No prebuilt/);
});

test('checksum names must match the selected release asset',()=>{
  const hash='a'.repeat(64);
  assert.equal(checksumFor(`${hash}  binary.tar.gz`,'binary.tar.gz'),hash);
  assert.throws(()=>checksumFor(`${hash}  different.tar.gz`,'binary.tar.gz'),/does not identify/);
});

test('first launch verifies bytes, caches outside the package, and later launches work offline',async()=>{
  const cacheRoot=await mkdtemp(join(tmpdir(),'osmogrep-npm-'));
  try {
    const bytes=Buffer.from('controlled release archive'), hash=createHash('sha256').update(bytes).digest('hex');
    const calls=[];
    const fetchImpl=async url=>{calls.push(url);return new Response(url.endsWith('.sha256')?hash:bytes);};
    const options={version:'0.3.15',platform:'darwin',arch:'arm64',cacheRoot,fetchImpl,
      extract: (_archive,directory,name)=>writeFileSync(join(directory,name),'native binary')};
    const binary=await ensureBinary(options);
    assert.equal(await readFile(binary,'utf8'),'native binary');
    assert.equal(calls.length,2);
    assert.equal(await ensureBinary({...options,fetchImpl:()=>{throw Error('offline');}}),binary);
    assert.deepEqual(await readdir(join(binary,'..')),['osmogrep']);
    await assert.rejects(ensureBinary({...options,version:'../escape'}),/Invalid/);
  } finally {await rm(cacheRoot,{recursive:true,force:true});}
});

test('bad hashes and linked executables never reach the installed cache',async()=>{
  const cacheRoot=await mkdtemp(join(tmpdir(),'osmogrep-npm-invalid-'));
  try {
    const bytes=Buffer.from('fixture');let extracted=false;
    const options={version:'0.3.15',platform:'darwin',arch:'arm64',cacheRoot,
      fetchImpl:async url=>new Response(url.endsWith('.sha256')?'a'.repeat(64):bytes),
      extract:()=>{extracted=true;}};
    await assert.rejects(ensureBinary(options),/checksum mismatch/);
    assert.equal(extracted,false);
    await assert.rejects(ensureBinary({...options,
      fetchImpl:async url=>new Response(url.endsWith('.sha256')?createHash('sha256').update(bytes).digest('hex'):bytes),
      extract:(_archive,directory,name)=>{
        writeFileSync(join(directory,'outside'),'linked file');
        linkSync(join(directory,'outside'),join(directory,name));
      },
    }),/regular file/);
    if(process.platform!=='win32') await assert.rejects(ensureBinary({...options,
      fetchImpl:async url=>new Response(url.endsWith('.sha256')?createHash('sha256').update(bytes).digest('hex'):bytes),
      extract:(_archive,directory,name)=>symlinkSync(join(directory,'outside'),join(directory,name)),
    }),/regular file/);
  } finally {await rm(cacheRoot,{recursive:true,force:true});}
});

test('missing release assets fail clearly without unpacking or retrying a 404',async()=>{
  const cacheRoot=await mkdtemp(join(tmpdir(),'osmogrep-npm-missing-'));let requests=0;
  try {
    await assert.rejects(ensureBinary({version:'0.3.15',platform:'darwin',arch:'arm64',cacheRoot,
      fetchImpl:async()=>{requests++;return new Response('missing',{status:404});},
      extract:()=>assert.fail('A missing release cannot be unpacked'),
    }),/HTTP 404/);
    assert.equal(requests,1);
  } finally {await rm(cacheRoot,{recursive:true,force:true});}
});
