import { siteLogo } from './site-art.ts';

const reset = '\x1b[0m';
const foreground = (rgb: number[]) => `\x1b[38;2;${rgb.join(';')}m`;
const background = (rgb: number[]) => `\x1b[48;2;${rgb.join(';')}m`;
export const brandOrange = [234, 88, 12];
export const introDuration = 10_000;

export function websiteMark(light = false): string[] {
  const palette: Record<string, number[]> = { i: light ? [24,24,27] : [233,229,220], a: brandOrange };
  // The deployed SVG's actual pixels, sampled to a compact 7×6 header mark.
  const pixel = (x: number, y: number) => siteLogo[Math.min(9,Math.floor((y+.5)*10/6))]![Math.min(10,Math.floor((x+.5)*11/7))]!;
  return Array.from({length:3},(_,y)=>Array.from({length:7},(_,x)=>{
    const top=palette[pixel(x,y*2)], bottom=palette[pixel(x,y*2+1)];
    if (!top && !bottom) return ' ';
    if (!top) return foreground(bottom!)+'▄'+reset;
    if (!bottom) return foreground(top)+'▀'+reset;
    if (top===bottom) return foreground(top)+'█'+reset;
    return foreground(top)+background(bottom)+'▀'+reset;
  }).join(''));
}
