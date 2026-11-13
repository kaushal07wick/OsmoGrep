import { gunzipSync } from 'node:zlib';
import { siteLogo, siteScene } from './site-art.ts';

const reset = '\x1b[0m';
const foreground = (rgb: number[]) => `\x1b[38;2;${rgb.join(';')}m`;
const background = (rgb: number[]) => `\x1b[48;2;${rgb.join(';')}m`;
export const brandOrange = [234, 88, 12];
let frames: [string, string][] | undefined;
function sceneFrames() {
  return frames ??= JSON.parse(gunzipSync(Buffer.from(siteScene.data, 'base64')).toString()) as [string, string][];
}
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

export function websiteScene(elapsed: number, light = false): string[] {
  const all = sceneFrames(), frame = all[Math.min(all.length-1,Math.floor(Math.max(0,elapsed)*siteScene.fps/1000))]!;
  const ink = light ? [29,27,25] : [222,216,203];
  const palette: Record<string,string> = {
    '0':reset, '1':foreground(ink), '2':foreground([165,141,112]),
    '3':foreground(brandOrange), '4':background(brandOrange)+foreground([255,255,255]),
    '6':foreground(light ? [82,82,91] : [143,137,127]), '7':foreground([169,128,90]),
  };
  return Array.from({length:siteScene.height},(_,y)=>{
    let row='', previous='';
    for(let x=0;x<siteScene.width;x++){
      const at=y*siteScene.width+x, color=frame[1][at]!;
      if(color!==previous){row+=reset+(palette[color]??palette['1']);previous=color;}
      row+=frame[0][at];
    }
    return row+reset;
  });
}

export function brandTexture(width: number): string {
  return foreground([49,45,40])+Array.from({length:width},(_,x)=>x%4===0?'·':' ').join('')+reset;
}
