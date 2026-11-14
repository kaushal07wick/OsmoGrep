import {truncateToWidth,visibleWidth,stripTerminalSequences} from '@earendil-works/pi-tui';
export const plain=(s:string)=>stripTerminalSequences(s).replace(/[\x00-\x1f\x7f-\x9f]/g,' ');
export const fit=(s:string,n:number)=>n>0?truncateToWidth(s,n,'…'):'';
export interface StatusEntry {id:string;value:string}

/** Shorten the path before omitting optional fields; never cut an arbitrary cell. */
export function statusText(entries:StatusEntry[],width:number):string {
  const fields=entries.map(e=>({...e,value:plain(e.value)}));
  const join=()=>fields.map(e=>e.value).filter(Boolean).join(' · ');
  const path=fields.find(e=>e.id==='directory');
  if(path&&visibleWidth(join())>width){
    const parts=path.value.split('/').filter(Boolean),last=parts.at(-1)??path.value;
    path.value=parts.length>2?`${path.value.startsWith('~')?'~':' '}/…/${last}`.trim():last;
  }
  for(const id of ['version','elapsed','tokens','activity','provider','context','session','branch','permissions']){
    if(visibleWidth(join())<=width||fields.length<=1)break;
    const at=fields.findIndex(e=>e.id===id);if(at>=0)fields.splice(at,1);
  }
  if(visibleWidth(join())>width){const model=fields.find(e=>e.id==='model');if(model)model.value=model.value.split(' · ')[0]!;}
  if(visibleWidth(join())>width&&path)path.value=fit(path.value,Math.max(4,width-Math.max(0,visibleWidth(join())-visibleWidth(path.value))));
  return fit(join(),width);
}

export function activity(elapsed:number,phase:string,tool:string|null,reduceMotion=false):string {
  const seconds=Math.max(0,Math.floor(elapsed/1000)),time=seconds>=60?`${Math.floor(seconds/60)}m ${seconds%60}s`:`${seconds}s`;
  const labels:Record<string,string>={list_dir:'Exploring files',read_file:'Reading source',edit_file:'Editing',shell:'Running command',test:'Testing'};
  const label=tool?(labels[tool]??tool.replaceAll('_',' ')):phase==='thinking'?'Thinking':phase==='starting'?'Preparing context':phase.replaceAll('_',' ');
  const frame=Math.floor(elapsed/110),glyphs=['·','⠂','⠆','⠖','⠶','⠷','⠶','⠖','⠆','⠂'];
  const wave=reduceMotion?'⠶⠶⠶':Array.from({length:5},(_,i)=>glyphs[(frame+i*2)%glyphs.length]).join('');
  return `${wave}  ${label}  ·  ${time}`;
}
