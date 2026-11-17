import {wrapTextWithAnsi} from '@earendil-works/pi-tui';
import {plain} from './chrome.ts';

export function teamNoticeActive(text: string, now: number): boolean {
  try {
    const at = JSON.parse(text.slice(5)).message.at;
    return Number.isFinite(at) && now >= at && now - at < 7000;
  } catch { return false; }
}

export function teamMessageLines(text: string, width: number, light: boolean, accent: string): string[] | null {
  if (!text.startsWith('TEAM|')) return null;
  try {
    const {message, outgoing, recipient_name} = JSON.parse(text.slice(5));
    if (typeof outgoing !== 'boolean' || typeof message?.body !== 'string'
      || typeof message.sender !== 'string' || typeof message.sender_title !== 'string'
      || !Number.isFinite(message.at)) return null;
    const reset = '\x1b[0m';
    const ink = light ? '\x1b[38;2;45;45;40m' : '\x1b[38;2;215;213;203m';
    const dim = light ? '\x1b[38;2;115;108;96m' : '\x1b[38;2;138;133;121m';
    const sender = message.sender_title.startsWith('/') ? `Agent · ${message.sender.slice(0, 8)}` : plain(message.sender_title);
    const identity = outgoing ? `to ${typeof recipient_name === 'string' ? plain(recipient_name) : message.recipient?.slice(0, 8) ?? 'all agents'}` : sender;
    const time = new Date(message.at).toISOString().slice(11, 16);
    const header = `${accent}${outgoing ? '↗' : '↙'} ${identity}${reset}${dim} · ${time} UTC${reset}`;
    const body: string[] = message.body.split('\n').flatMap((line: string) =>
      wrapTextWithAnsi(ink + plain(line) + reset, Math.max(1, width - 2)));
    return ['', ...wrapTextWithAnsi(header, Math.max(1, width)),
      ...body.map(line => `${accent}│${reset} ${line}`), ''];
  } catch {
    return null;
  }
}
