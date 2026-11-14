import {rgbPng} from './png.ts';
import { siteLogo } from './site-art.ts';
import { introDuration } from './brand.ts';
import {scenePoints,type Scene} from './scenes.ts';

const tau = Math.PI * 2;
const smooth = (a: number, b: number, t: number) => { const x = Math.max(0, Math.min(1, (t-a)/(b-a))); return x*x*x*(x*(x*6-15)+10); };
const bayer = [0,8,2,10,12,4,14,6,3,11,1,9,15,7,13,5];
const dots = [[1,8],[2,16],[4,32],[64,128]];
const pixel = (x: number, y: number) => siteLogo[Math.floor((y+1)*5)]?.[Math.floor((x/1.1+1)*5.5)] ?? '.';
const meshes: (Float32Array | undefined)[] = [];

// The point ordering remains stable throughout both morph directions.
function mesh(detail:1|3):Float32Array {
  if(meshes[detail])return meshes[detail]!;
  const columns=128*detail,rows=48*detail,points=new Float32Array(columns*rows*6);
  for(let u=0;u<columns;u++){
    const a=u/columns*tau,ca=Math.cos(a),sa=Math.sin(a);let inner=1.5,outer=0;
    for(let i=0;i<256;i++){const r=i/256*1.5;if(pixel(ca*r,sa*r)!=='.'){inner=Math.min(inner,r);outer=r;}}
    for(let v=0;v<rows;v++){
      const b=v/rows*tau,r=(outer+inner)/2+(outer-inner)/2*Math.cos(b);
      points.set([ca*r,sa*r,.16*Math.sin(b),.35+.5*Math.max(0,Math.sin(b)),pixel(ca*r,sa*r)==='a'?1:0,1],(u*rows+v)*6);
    }
  }return meshes[detail]=points;
}

function raster(elapsed:number,columns:number,rows:number,detail:1|3,scene:Scene,bright:boolean){
  const width=columns*2*detail,height=rows*4*detail,length=width*height;
  const depth=new Float32Array(length).fill(-Infinity),shade=new Float32Array(length),orange=new Uint8Array(length);
  const t=Math.max(0,Math.min(1,elapsed/introDuration));
  const morph=smooth(.12,.38,t)*(1-smooth(.64,.94,t));
  const turns=smooth(.07,.38,t)+smooth(.64,.95,t),yaw=turns===2?0:tau*turns;
  const pitch=.15*Math.sin(yaw)+morph*({horizon:0,interference:1.02,life:.24,lorenz:-.1}[scene]);
  const cy=Math.cos(yaw),sy=Math.sin(yaw),cx=Math.cos(pitch),sx=Math.sin(pitch);
  const scale=.6*(1+.025*Math.sin(tau*t)*morph)*Math.min(width/2.7,height/2.5),points=mesh(detail);
  const target=morph>0?scenePoints(scene,elapsed/1000,points.length/6):points;
  for(let i=0;i<points.length;i+=6){
    const curl=.18*Math.sin(Math.PI*morph)*Math.sin(Math.PI*t),angle=curl*Math.sin(i/6*2.399963);
    const mx=points[i]!*(1-morph)+target[i]!*morph,my=points[i+1]!*(1-morph)+target[i+1]!*morph;
    const x=mx*Math.cos(angle)-my*Math.sin(angle),y=mx*Math.sin(angle)+my*Math.cos(angle),z=points[i+2]!*(1-morph)+target[i+2]!*morph;
    const xx=x*cy+z*sy,zz=z*cy-x*sy,yy=y*cx-zz*sx,rotatedZ=y*sx+zz*cx;
    const perspective=4/(4-rotatedZ),px=Math.floor(width/2+xx*scale*perspective),py=Math.floor(height/2+yy*scale*perspective);
    const alpha=1-morph+target[i+5]!*morph;if(alpha<.05)continue;
    const illumination=(points[i+3]!*(1-morph)+target[i+3]!*morph)*alpha;
    for(let dy=0;dy<2;dy++)for(let dx=0;dx<2;dx++){
      const ax=px+dx,ay=py+dy;if(ax<0||ay<0||ax>=width||ay>=height)continue;
      const at=ay*width+ax;
      if(rotatedZ>depth[at]!){depth[at]=rotatedZ;shade[at]=illumination;orange[at]=points[i+4]!*(1-morph)+target[i+4]!*morph>.45?1:0;}
    }
  }
  const gain=.24+(bright?.62*(1-smooth(.82,1,t)):0);
  return {width,height,shade,orange,gain};
}

/** Eight dithered subpixels per cell on terminals without inline graphics. */
export function introFrame(elapsed: number, columns: number, rows: number, light = false,scene:Scene='horizon',bright=false): string[] {
  const {width,shade,orange,gain}=raster(elapsed,columns,rows,1,scene,bright);
  return Array.from({length:rows},(_,y)=>{
    let line='', last='';
    for(let x=0;x<columns;x++) {
      let mask=0, warm=0, total=0;
      for(let dy=0;dy<4;dy++) for(let dx=0;dx<2;dx++) {
        const px=x*2+dx, py=y*4+dy, at=py*width+px;
        if(shade[at]!>(bayer[(py%4)*4+px%4]!+.5)/16) {mask|=dots[dy]![dx]!;warm+=orange[at]!;total++;}
      }
      const rgb=warm>total/2 ? (light?[190,65,10]:[164,80,26]) : (light?[65,63,59]:[144,140,133]);
      const color=rgb.map(n=>Math.round(light?250-(250-n)*gain:n*gain)).join(';');
      if(mask&&color!==last) {line+=`\x1b[38;2;${color}m`;last=color;}
      line+=mask?String.fromCharCode(0x2800+mask):' ';
    }
    return line+'\x1b[0m';
  });
}

/** Three times the linear dot resolution, displayed through the existing image path. */
export function introPng(elapsed: number, columns: number, rows: number, light = false,scene:Scene='horizon',bright=false): Buffer {
  const {width,height,shade,orange,gain}=raster(elapsed,columns,rows,3,scene,bright);
  const raw=Buffer.alloc((width*3+1)*height,light?250:0);
  for(let y=0;y<height;y++) {
    raw[y*(width*3+1)]=0;
    for(let x=0;x<width;x++) {
      const at=y*width+x;
      // Leave a pixel of black between fine dots instead of filling the surface with crosshatching.
      if(x%2||y%2||shade[at]!<=(bayer[((y/2)%4)*4+(x/2)%4]!+.5)/16) continue;
      const rgb=orange[at] ? (light?[190,65,10]:[164,80,26]) : (light?[65,63,59]:[144,140,133]);
      const offset=y*(width*3+1)+1+x*3;
      for(let c=0;c<3;c++)raw[offset+c]=Math.round(light?250-(250-rgb[c]!)*gain:rgb[c]!*gain);
    }
  }
  return rgbPng(width,height,raw);
}
