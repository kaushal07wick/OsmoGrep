// Portable geometry for the four approved gallery concepts. No DOM or external assets.
export const scenes=['horizon','interference','life','lorenz'] as const;
export type Scene=typeof scenes[number];
const tau=2*Math.PI;
const fract=(x:number)=>x-Math.floor(x);
const hash=(i:number)=>fract(Math.sin(i*127.1+311.7)*43758.5453);
const cache=new Map<number,{out:Float32Array;noise:Float32Array}>();
let paths:Float32Array[]|undefined,life: number[][][]|undefined;

function lorenzPath(delta:number){
  let x=.1+delta,y=0,z=0;const out=new Float32Array(60000*3),dt=.005;
  const f=(a:number,b:number,c:number)=>[10*(b-a),a*(28-c)-b,a*b-8*c/3];
  for(let i=0;i<65000;i++){
    const a=f(x,y,z),b=f(x+a[0]!*dt/2,y+a[1]!*dt/2,z+a[2]!*dt/2),c=f(x+b[0]!*dt/2,y+b[1]!*dt/2,z+b[2]!*dt/2),d=f(x+c[0]!*dt,y+c[1]!*dt,z+c[2]!*dt);
    x+=dt*(a[0]!+2*b[0]!+2*c[0]!+d[0]!)/6;y+=dt*(a[1]!+2*b[1]!+2*c[1]!+d[1]!)/6;z+=dt*(a[2]!+2*b[2]!+2*c[2]!+d[2]!)/6;
    if(i>=5000)out.set([x/22,-(z-25)/25,y/30],(i-5000)*3);
  }return out;
}
export function lifeStates(){
  if(life)return life;
  const w=76,h=44;let cells=new Uint8Array(w*h);
  const seed=[[24,0],[22,1],[24,1],[12,2],[13,2],[20,2],[21,2],[34,2],[35,2],[11,3],[15,3],[20,3],[21,3],[34,3],[35,3],[0,4],[1,4],[10,4],[16,4],[20,4],[21,4],[0,5],[1,5],[10,5],[14,5],[16,5],[17,5],[22,5],[24,5],[10,6],[16,6],[24,6],[11,7],[15,7],[12,8],[13,8]];
  for(const[x,y]of seed)cells[(y!+6)*w+x!+5]=1;
  life=[];
  for(let generation=0;generation<240;generation++){
    const alive:number[][]=[],next=new Uint8Array(w*h);
    for(let y=1;y<h-1;y++)for(let x=1;x<w-1;x++){
      const at=y*w+x;if(cells[at])alive.push([x,y]);
      const n=cells[at-w-1]!+cells[at-w]!+cells[at-w+1]!+cells[at-1]!+cells[at+1]!+cells[at+w-1]!+cells[at+w]!+cells[at+w+1]!;
      next[at]=n===3||cells[at]&&n===2?1:0;
    }life.push(alive);cells=next;
  }return life;
}

/** Six floats per point: position, luminance, warm accent, opacity. Scratch is reused. */
export function scenePoints(scene:Scene,seconds:number,count:number):Float32Array{
  if(!cache.has(count))cache.set(count,{out:new Float32Array(count*6),noise:Float32Array.from({length:count},(_,i)=>hash(i))});
  const {out,noise}=cache.get(count)!;
  if(scene==='lorenz'&&!paths)paths=[lorenzPath(0),lorenzPath(.00001)];
  const alive=scene==='life'?lifeStates()[Math.floor(seconds*18)%240]!:[];
  for(let i=0;i<count;i++){
    const at=i*6,q=i/count,n=noise[i]!;
    let x=0,y=0,z=0,shade=.7,warm=0,alpha=1;
    if(scene==='horizon'){
      if(q<.73){
        const r=.51+Math.pow(n,.65)*.85,a=q/.73*tau*31+seconds*(.3+1.1/(r*r));
        x=r*Math.cos(a);const yy=r*Math.sin(a);y=yy*.23;z=yy*.95;
        shade=.28+.6*(1-(r-.51)/.85);warm=Math.max(0,Math.min(1,(.86-r)*3));
        alpha=x*x+y*y<.205&&z<0?0:.8;
      }else{
        const a=(q-.73)/.27*tau,r=.46+n*.065;
        x=Math.cos(a)*r;y=Math.sin(a)*r;z=.08;shade=.35+.6*Math.pow(Math.max(0,Math.sin(a-seconds*.5)),3);warm=.18;
      }
    }else if(scene==='interference'){
      const cols=Math.ceil(Math.sqrt(count*2.5)),rows=Math.ceil(count/cols);
      x=(i%cols/(cols-1)-.5)*2.5;y=(Math.floor(i/cols)/(rows-1)-.5)*1.8;
      const r1=Math.hypot(x+.7,y-.28),r2=Math.hypot(x+.7,y+.28);
      const wave=Math.sin(17*r1-3*seconds)+Math.sin(17*r2-3*seconds);
      z=wave*.115;shade=.35+.3*(1+wave/2);warm=Math.pow(Math.abs(wave)/2,5)*.7;
    }else if(scene==='life'){
      const cell=alive[i%alive.length]!,face=Math.floor(i/alive.length);
      x=(cell[0]!/75-.5)*2.6+(n-.5)*.025;y=(cell[1]!/43-.5)*1.5+(fract(n*97)-.5)*.025;
      z=(face%3-1)*.025;shade=.4+.4*fract(n*13);warm=cell[0]!>34?.8:0;
    }else{
      const twin=i%2,path=paths![twin]!,age=Math.floor(i/2)/(count/2),end=18000+Math.floor(seconds*1600)%36000;
      const index=(end-Math.floor(age*16000))*3;
      x=path[index]!+(n-.5)*.008;y=path[index+1]!+(fract(n*19)-.5)*.008;z=path[index+2]!;
      shade=.35+.6*(1-age);warm=twin?.78:0;alpha=.3+.65*(1-age);
    }
    out[at]=x;out[at+1]=y;out[at+2]=z;out[at+3]=shade;out[at+4]=warm;out[at+5]=alpha;
  }return out;
}
