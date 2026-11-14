import { crc32, deflateSync } from 'node:zlib';
import { siteLogo } from './site-art.ts';
import { introDuration } from './brand.ts';

const tau = Math.PI * 2;
const smooth = (a: number, b: number, t: number) => { const x = Math.max(0, Math.min(1, (t-a)/(b-a))); return x*x*(3-2*x); };
const bayer = [0,8,2,10,12,4,14,6,3,11,1,9,15,7,13,5];
const dots = [[1,8],[2,16],[4,32],[64,128]];
const pixel = (x: number, y: number) => siteLogo[Math.floor((y+1)*5)]?.[Math.floor((x/1.1+1)*5.5)] ?? '.';
const meshes: (Float32Array | undefined)[] = [];

// Folded paper facets: body, head, orange beak, forked tail and two wings.
const facets = [
  {a:[-.4,.18,.03],b:[.43,.08,.04],c:[-.08,.14,.34],wing:0,orange:0},
  {a:[-.4,.18,.03],b:[-.08,.14,.34],c:[-.1,.46,.08],wing:0,orange:0},
  {a:[-.08,.14,.34],b:[.43,.08,.04],c:[-.1,.46,.08],wing:0,orange:0},
  {a:[.05,.16,.1],b:[.37,-.29,.12],c:[.55,.09,.1],wing:0,orange:0},
  {a:[.37,-.29,.12],b:[.65,-.26,.12],c:[.55,.09,.1],wing:0,orange:0},
  {a:[.65,-.26,.12],b:[1.05,-.13,.06],c:[.57,-.08,.12],wing:0,orange:1},
  {a:[-.37,.2,.02],b:[-1.04,.48,.02],c:[-.35,.38,.04],wing:0,orange:0},
  {a:[-.35,.28,.04],b:[-.82,.68,.02],c:[-.1,.43,.08],wing:0,orange:0},
  {a:[-.22,.2,-.12],b:[-.25,-.92,-.18],c:[.16,-.32,-.3],wing:-1,orange:0},
  {a:[-.22,.2,.15],b:[-.88,-.72,.08],c:[-.24,-.28,.5],wing:1,orange:0},
  {a:[-.24,-.28,.5],b:[-.88,-.72,.08],c:[.18,-.43,.1],wing:1,orange:0},
  {a:[-.22,.2,.15],b:[-.24,-.28,.5],c:[.18,-.43,.1],wing:1,orange:0},
].map(face=>{
  const ab=face.b.map((n,i)=>n-face.a[i]!), ac=face.c.map((n,i)=>n-face.a[i]!);
  const normal=[ab[1]!*ac[2]!-ab[2]!*ac[1]!,ab[2]!*ac[0]!-ab[0]!*ac[2]!,ab[0]!*ac[1]!-ab[1]!*ac[0]!];
  const length=Math.hypot(...normal), facing=normal[2]!<0?-1:1;
  return {...face,area:length/2,normal:normal.map(n=>n/length*facing)};
});
const totalArea=facets.reduce((sum,face)=>sum+face.area,0);

// Pair ring surface points with area-weighted samples of the folded bird.
function mesh(detail: 1 | 3): Float32Array {
  if (meshes[detail]) return meshes[detail]!;
  const columns=256*detail, rows=80*detail, points=new Float32Array(columns*rows*15);
  for (let u=0; u<columns; u++) {
    const a=u/columns*tau, ca=Math.cos(a), sa=Math.sin(a);
    let inner=1.5, outer=0;
    for (let i=0; i<256; i++) {
      const r=i/256*1.5;
      if (pixel(ca*r,sa*r)!=='.') { inner=Math.min(inner,r); outer=r; }
    }
    let area=u/columns*totalArea, face=facets[0]!;
    for(const candidate of facets) {
      face=candidate;
      if(area<candidate.area) break;
      area-=candidate.area;
    }
    const radial=Math.min(1,area/face.area);
    for (let v=0; v<rows; v++) {
      const b=v/rows*tau-Math.PI, cb=Math.cos(b), sb=Math.sin(b);
      const r=(outer+inner)/2+(outer-inner)/2*cb, at=(u*rows+v)*15;
      const target=face.a.map((n,i)=>n*(1-radial)+radial*((1-v/rows)*face.b[i]!+v/rows*face.c[i]!));
      points.set([ca*r,sa*r,.16*sb,ca*cb,sa*cb,sb,
        ...target,...face.normal,pixel(ca*r,sa*r)==='a'?1:0,face.orange,face.wing],at);
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
  const gesture=smooth(.42,.64,t), fold=gesture>0&&gesture<1 ? .95*Math.sin(Math.PI*gesture) : 0;
  const cf=Math.cos(fold), sf=Math.sin(fold);
  const scale=.8*Math.min(width/2.7,height/2.5), points=mesh(detail);
  for (let i=0; i<points.length; i+=15) {
    const wing=points[i+14]!, lift=sf*wing, hingeZ=wing>0?.15:-.12;
    const targetY=wing ? .2+(points[i+7]!-.2)*cf-(points[i+8]!-hingeZ)*lift : points[i+7]!;
    const targetZ=wing ? hingeZ+(points[i+7]!-.2)*lift+(points[i+8]!-hingeZ)*cf : points[i+8]!;
    const targetNY=wing ? points[i+10]!*cf-points[i+11]!*lift : points[i+10]!;
    const targetNZ=wing ? points[i+10]!*lift+points[i+11]!*cf : points[i+11]!;
    const x=points[i]!*(1-morph)+points[i+6]!*morph, y=points[i+1]!*(1-morph)+targetY*morph, z=points[i+2]!*(1-morph)+targetZ*morph;
    const xx=x*cy+z*sy, zz=z*cy-x*sy, yy=y*cx-zz*sx, rotatedZ=y*sx+zz*cx;
    const nx=points[i+3]!*(1-morph)+points[i+9]!*morph, ny=points[i+4]!*(1-morph)+targetNY*morph, nz=points[i+5]!*(1-morph)+targetNZ*morph;
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
      const color=warm>total/2 ? (light?'190;65;10':'164;62;8') : (light?'65;63;59':'123;119;112');
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
      const rgb=orange[at] ? (light?[190,65,10]:[164,62,8]) : (light?[65,63,59]:[144,140,133]);
      const offset=y*(width*3+1)+1+x*3;
      raw[offset]=rgb[0]!;raw[offset+1]=rgb[1]!;raw[offset+2]=rgb[2]!;
    }
  }
  const header=Buffer.alloc(13);header.writeUInt32BE(width);header.writeUInt32BE(height,4);header[8]=8;header[9]=2;
  return Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]),chunk('IHDR',header),chunk('IDAT',deflateSync(raw,{level:1})),chunk('IEND',Buffer.alloc(0))]);
}
