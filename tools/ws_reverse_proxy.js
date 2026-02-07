const http = require('http');
const fs = require('fs');
const net = require('net');
const path = require('path');

const LISTEN_PORT = Number(process.env.PROXY_PORT || 8080);
const WS_TARGET_HOST = process.env.WS_TARGET_HOST || '127.0.0.1';
const WS_TARGET_PORT = Number(process.env.WS_TARGET_PORT || 7001);
const MIC_HTML = process.env.MIC_HTML ||
  '/home/wiredleap/kaushal_workspace/voxtral-mic/mic.html';

function serveMicHtml(res) {
  fs.readFile(MIC_HTML, (err, data) => {
    if (err) {
      res.writeHead(500, { 'Content-Type': 'text/plain' });
      res.end('Failed to read mic.html');
      return;
    }
    res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    res.end(data);
  });
}

const server = http.createServer((req, res) => {
  if (req.url === '/' || req.url.startsWith('/mic.html')) {
    return serveMicHtml(res);
  }

  res.writeHead(404, { 'Content-Type': 'text/plain' });
  res.end('Not found');
});

server.on('upgrade', (req, socket, head) => {
  const target = net.connect(WS_TARGET_PORT, WS_TARGET_HOST, () => {
    const lines = [];
    lines.push(`${req.method} ${req.url} HTTP/${req.httpVersion}`);
    for (let i = 0; i < req.rawHeaders.length; i += 2) {
      const key = req.rawHeaders[i];
      const val = req.rawHeaders[i + 1];
      if (!key || !val) continue;
      if (key.toLowerCase() === 'host') {
        lines.push(`Host: ${WS_TARGET_HOST}:${WS_TARGET_PORT}`);
      } else {
        lines.push(`${key}: ${val}`);
      }
    }
    lines.push('', '');
    target.write(lines.join('\r\n'));
    if (head && head.length) {
      target.write(head);
    }
    socket.pipe(target);
    target.pipe(socket);
  });

  target.on('error', () => socket.destroy());
  socket.on('error', () => target.destroy());
});

server.listen(LISTEN_PORT, '0.0.0.0', () => {
  console.log(
    `Reverse proxy listening on 0.0.0.0:${LISTEN_PORT} -> ${WS_TARGET_HOST}:${WS_TARGET_PORT}`
  );
  console.log(`Serving mic.html from ${MIC_HTML}`);
});
