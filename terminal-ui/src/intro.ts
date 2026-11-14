import { crc32, deflateSync } from 'node:zlib';
import { siteLogo } from './site-art.ts';
import { introDuration } from './brand.ts';

const tau = Math.PI * 2;
const smooth = (a: number, b: number, t: number) => { const x = Math.max(0, Math.min(1, (t-a)/(b-a))); return x*x*(3-2*x); };
const bayer = [0,8,2,10,12,4,14,6,3,11,1,9,15,7,13,5];
const dots = [[1,8],[2,16],[4,32],[64,128]];
const pixel = (x: number, y: number) => siteLogo[Math.floor((y+1)*5)]?.[Math.floor((x/1.1+1)*5.5)] ?? '.';
const meshes: (Float32Array | undefined)[] = [];

// Match surface points along the ring and the three strokes of the terminal mark.
function mesh(detail: 1 | 3): Float32Array {
  if (meshes[detail]) return meshes[detail]!;
  const columns=256*detail, rows=80*detail, points=new Float32Array(columns*rows*14);
  const diagonal=Math.hypot(.6,.55), total=diagonal*2+.85;
  for (let u=0; u<columns; u++) {
    const a=u/columns*tau, ca=Math.cos(a), sa=Math.sin(a);
    let inner=1.5, outer=0;
    for (let i=0; i<256; i++) {
      const r=i/256*1.5;
      if (pixel(ca*r,sa*r)!=='.') { inner=Math.min(inner,r); outer=r; }
    }
    const distance=u/columns*total;
    const segment=distance<diagonal ? [-.8,-.55,.6,.55,distance/diagonal]
      : distance<diagonal*2 ? [-.2,0,-.6,.55,(distance-diagonal)/diagonal]
      : [0,.55,.85,0,(distance-diagonal*2)/.85];
    const [x,y,dx,dy,f]=segment as [number,number,number,number,number];
    const length=Math.hypot(dx,dy), nx=-dy/length, ny=dx/length;
    for (let v=0; v<rows; v++) {
      const b=v/rows*tau-Math.PI, cb=Math.cos(b), sb=Math.sin(b);
      const r=(outer+inner)/2+(outer-inner)/2*cb, at=(u*rows+v)*14;
      points.set([ca*r,sa*r,.16*sb,ca*cb,sa*cb,sb,
        x+dx*f+nx*.12*cb,y+dy*f+ny*.12*cb,.12*sb,nx*cb,ny*cb,sb,
        pixel(ca*r,sa*r)==='a'?1:0,distance>=diagonal*2?1:0],at);
    }
  }
  return meshes[detail]=points;
}

function raster(elapsed: number, columns: number, rows: number, detail: 1 | 3) {
  const width=columns*2*detail, height=rows*4*detail, length=width*height;
  const depth=new Float32Array(length).fill(-Infinity), shade=new Float32Array(length), orange=new Uint8Array(length);
  const t=Math.max(0,Math.min(1,elapsed/introDuration));
  const morph=smooth(.2,.42,t)*(1-smooth(.64,.94,t));
  const turns=smooth(.12,.42,t)+smooth(.64,.94,t), yaw=turns===2?0:tau*turns;
  const pitch=.25*Math.sin(yaw);
  const cy=Math.cos(yaw), sy=Math.sin(yaw), cx=Math.cos(pitch), sx=Math.sin(pitch);
  const scale=Math.min(width/2.7,height/2.5), points=mesh(detail);
  for (let i=0; i<points.length; i+=14) {
    const x=points[i]!*(1-morph)+points[i+6]!*morph, y=points[i+1]!*(1-morph)+points[i+7]!*morph, z=points[i+2]!*(1-morph)+points[i+8]!*morph;
    const xx=x*cy+z*sy, zz=z*cy-x*sy, yy=y*cx-zz*sx, rotatedZ=y*sx+zz*cx;
    const nx=points[i+3]!*(1-morph)+points[i+9]!*morph, ny=points[i+4]!*(1-morph)+points[i+10]!*morph, nz=points[i+5]!*(1-morph)+points[i+11]!*morph;
    const normalX=nx*cy+nz*sy, normalZ=nz*cy-nx*sy;
    const normalY=ny*cx-normalZ*sx, normalFront=ny*sx+normalZ*cx;
    const illumination=.22+.65*Math.max(0,(-normalX*.35-normalY*.45+normalFront*.82)/Math.max(.001,Math.hypot(nx,ny,nz)));
    const perspective=4/(4-rotatedZ);
    const px=Math.floor(width/2+xx*scale*perspective), py=Math.floor(height/2+yy*scale*perspective);
    for (let dy=0;dy<2;dy++) for(let dx=0;dx<2;dx++) {
      const ax=px+dx, ay=py+dy;
      if(ax<0||ay<0||ax>=width||ay>=height) continue;
      const at=ay*width+ax;
      if(rotatedZ>depth[at]!) { depth[at]=rotatedZ; shade[at]=illumination; orange[at]=points[i+(morph>.5?13:12)]!; }
    }
  }
  return {width,height,shade,orange};
}

/** Eight dithered subpixels per cell on terminals without inline graphics. */
export function introFrame(elapsed: number, columns: number, rows: number, light = false): string[] {
  const {width,shade,orange}=raster(elapsed,columns,rows,1);
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

function chunk(kind: string, data: Buffer): Buffer {
  const result=Buffer.alloc(data.length+12);
  result.writeUInt32BE(data.length);result.write(kind,4);data.copy(result,8);
  result.writeUInt32BE(crc32(result.subarray(4,-4)),result.length-4);
  return result;
}

/** Three times the linear dot resolution, displayed through the existing image path. */
export function introPng(elapsed: number, columns: number, rows: number, light = false): Buffer {
  const {width,height,shade,orange}=raster(elapsed,columns,rows,3);
  const raw=Buffer.alloc((width*3+1)*height,light?250:0);
  for(let y=0;y<height;y++) {
    raw[y*(width*3+1)]=0;
    for(let x=0;x<width;x++) {
      const at=y*width+x;
      if(shade[at]!<=(bayer[(y%4)*4+x%4]!+.5)/16) continue;
      const rgb=orange[at] ? (light?[190,65,10]:[234,88,12]) : (light?[65,63,59]:[205,200,190]);
      const offset=y*(width*3+1)+1+x*3;
      raw[offset]=rgb[0]!;raw[offset+1]=rgb[1]!;raw[offset+2]=rgb[2]!;
    }
  }
  const header=Buffer.alloc(13);header.writeUInt32BE(width);header.writeUInt32BE(height,4);header[8]=8;header[9]=2;
  return Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]),chunk('IHDR',header),chunk('IDAT',deflateSync(raw,{level:1})),chunk('IEND',Buffer.alloc(0))]);
}
