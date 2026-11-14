import { siteLogo } from './site-art.ts';
import { introDuration } from './brand.ts';

const tau = Math.PI * 2;
const smooth = (a: number, b: number, t: number) => { const x = Math.max(0, Math.min(1, (t-a)/(b-a))); return x*x*(3-2*x); };
const bayer = [0,8,2,10,12,4,14,6,3,11,1,9,15,7,13,5];
const dots = [[1,8],[2,16],[4,32],[64,128]];
const pixel = (x: number, y: number) => siteLogo[Math.floor((y+1)*5)]?.[Math.floor((x/1.1+1)*5.5)] ?? '.';

// Sample the actual pixel ring once; the running animation only transforms points.
const points: number[][] = [];
for (let u=0; u<256; u++) {
  const a=u/256*tau, ca=Math.cos(a), sa=Math.sin(a);
  let inner=1.5, outer=0;
  for (let i=0; i<256; i++) {
    const r=i/256*1.5;
    if (pixel(ca*r,sa*r)!=='.') { inner=Math.min(inner,r); outer=r; }
  }
  for (let v=0; v<80; v++) {
    const b=v/80*tau-Math.PI, cb=Math.cos(b), sb=Math.sin(b), latitude=b/2;
    const r=(outer+inner)/2+(outer-inner)/2*cb;
    points.push([ca*r,sa*r,.16*sb,ca*cb,sa*cb,sb,
      ca*Math.cos(latitude)*.95,sa*Math.cos(latitude)*.95,Math.sin(latitude)*.95,
      pixel(ca*r,sa*r)==='a'?1:0]);
  }
}

/** Eight dithered subpixels per cell; no graphics protocol, assets or random flicker. */
export function introFrame(elapsed: number, columns: number, rows: number, light = false, pointer: {x:number;y:number}|null = null): string[] {
  const width=columns*2, height=rows*4, length=width*height;
  const depth=new Float32Array(length).fill(-Infinity), shade=new Float32Array(length), orange=new Uint8Array(length);
  const t=Math.max(0,Math.min(1,elapsed/introDuration));
  const morph=smooth(.2,.43,t)*(1-smooth(.57,.82,t));
  const yaw=tau*smooth(0,1,t)+(pointer?.x??0)*.5;
  const pitch=.3*Math.sin(tau*t)+(pointer?.y??0)*.35;
  const cy=Math.cos(yaw), sy=Math.sin(yaw), cx=Math.cos(pitch), sx=Math.sin(pitch);
  const scale=Math.min(width/2.7,height/2.5);
  for (const p of points) {
    const x=p[0]!*(1-morph)+p[6]!*morph, y=p[1]!*(1-morph)+p[7]!*morph, z=p[2]!*(1-morph)+p[8]!*morph;
    const xx=x*cy+z*sy, zz=z*cy-x*sy, yy=y*cx-zz*sx, rotatedZ=y*sx+zz*cx;
    const nx=p[3]!*(1-morph)+p[6]!/ .95*morph, ny=p[4]!*(1-morph)+p[7]!/ .95*morph, nz=p[5]!*(1-morph)+p[8]!/ .95*morph;
    const normalX=nx*cy+nz*sy, normalZ=nz*cy-nx*sy;
    const normalY=ny*cx-normalZ*sx, normalFront=ny*sx+normalZ*cx;
    const illumination=.25+.6*Math.max(0,(-normalX*.35-normalY*.45+normalFront*.82)/Math.max(.001,Math.hypot(nx,ny,nz)));
    const perspective=4/(4-rotatedZ);
    const px=Math.floor(width/2+xx*scale*perspective), py=Math.floor(height/2+yy*scale*perspective);
    for (let dy=0;dy<2;dy++) for(let dx=0;dx<2;dx++) {
      const ax=px+dx, ay=py+dy;
      if(ax<0||ay<0||ax>=width||ay>=height) continue;
      const at=ay*width+ax;
      if(rotatedZ>depth[at]!) { depth[at]=rotatedZ; shade[at]=illumination; orange[at]=p[9]!; }
    }
  }
  return Array.from({length:rows},(_,y)=>{
    let line='', last='';
    for(let x=0;x<columns;x++) {
      let mask=0, warm=0, total=0;
      for(let dy=0;dy<4;dy++) for(let dx=0;dx<2;dx++) {
        const px=x*2+dx, py=y*4+dy, at=py*width+px;
        if(shade[at]!>(bayer[(py%4)*4+px%4]!+.5)/16) {mask|=dots[dy]![dx]!;warm+=orange[at]!;total++;}
      }
      const color=warm>total/2 ? (light?'190;65;10':'234;88;12') : (light?'65;63;59':'175;170;160');
      if(mask&&color!==last) {line+=`\x1b[38;2;${color}m`;last=color;}
      line+=mask?String.fromCharCode(0x2800+mask):' ';
    }
    return line+'\x1b[0m';
  });
}
