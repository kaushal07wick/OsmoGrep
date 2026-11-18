# OsmoGrep

A terminal coding agent for building features, understanding code, and fixing bugs.

```sh
npm install -g osmogrep
# or
pnpm add -g osmogrep

osmogrep
```

You can also use `npx osmogrep` or `pnpm dlx osmogrep`. GitHub releases include
the same launcher as `osmogrep-npm.tgz` for direct tarball installation.

Requires Node.js 22.19+. This small package downloads the matching, checksummed
Rust executable on its first launch and reuses it afterward. It does not depend
on install scripts, so pnpm's blocked lifecycle scripts do not prevent setup.
Supported releases: macOS Apple silicon, Linux x64 with glibc, Windows x64.
macOS and iPhone companions are optional downloads inside Settings → Workspace;
they are not included in the npm package or the Rust executable.

[Documentation](https://github.com/kaushal07wick/OsmoGrep#readme) · [Issues](https://github.com/kaushal07wick/OsmoGrep/issues)
