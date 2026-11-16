import {activityInfo} from './activity.ts';
import {truncateToWidth,visibleWidth,stripTerminalSequences} from '@earendil-works/pi-tui';
export const plain=(s:string)=>stripTerminalSequences(s).replace(/[\x00-\x1f\x7f-\x9f]/g,' ');
export const fit=(s:string,n:number)=>n>0?truncateToWidth(s,n,'…'):'';
export interface StatusEntry {id:string;value:string}

/** Shorten the path before omitting optional fields; never cut an arbitrary cell. */
export function statusLayout(entries:StatusEntry[],width:number) {
  const fields=entries.map(e=>({...e,value:plain(e.value)}));
  const join=()=>fields.map(e=>e.value).filter(Boolean).join(' · ');
  const path=fields.find(e=>e.id==='directory');
  if(path&&visibleWidth(join())>width){
    const parts=path.value.split('/').filter(Boolean),last=parts.at(-1)??path.value;
    path.value=parts.length>2?`${path.value.startsWith('~')?'~':' '}/…/${last}`.trim():last;
  }
  for(const id of ['version','elapsed','tokens','activity','context','session','branch','permissions']){
    if(visibleWidth(join())<=width||fields.length<=1)break;
    const at=fields.findIndex(e=>e.id===id);if(at>=0)fields.splice(at,1);
  }
  if(visibleWidth(join())>width){const model=fields.find(e=>e.id==='model');if(model)model.value=model.value.split(' · ')[0]!;}
  if(visibleWidth(join())>width&&path)path.value=fit(path.value,Math.max(4,width-Math.max(0,visibleWidth(join())-visibleWidth(path.value))));
  const text=fit(join(),width);let x=0;
  const items=fields.filter(e=>e.value).map(e=>{const item={id:e.id,x,width:Math.max(0,Math.min(visibleWidth(e.value),visibleWidth(text)-x))};x+=visibleWidth(e.value)+3;return item;}).filter(e=>e.width>0);
  return {text,items};
}

export const statusText=(entries:StatusEntry[],width:number)=>statusLayout(entries,width).text;

export function activity(elapsed:number,phase:string,tool:string|null,reduceMotion=false):string {
  const info=activityInfo(elapsed,phase,tool,reduceMotion);
  return `· ${info.label}  ·  ${info.time}`;
}
