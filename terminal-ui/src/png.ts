import {crc32,deflateSync} from 'node:zlib';
function chunk(kind:string,data:Buffer):Buffer {
  const result=Buffer.alloc(data.length+12);result.writeUInt32BE(data.length);result.write(kind,4);data.copy(result,8);
  result.writeUInt32BE(crc32(result.subarray(4,-4)),result.length-4);return result;
}
export function rgbPng(width:number,height:number,scanlines:Buffer):Buffer {
  if(!Number.isInteger(width)||!Number.isInteger(height)||width<1||height<1||width*height>4_000_000||scanlines.length!==(width*3+1)*height)throw Error('Invalid PNG dimensions');
  const header=Buffer.alloc(13);header.writeUInt32BE(width);header.writeUInt32BE(height,4);header[8]=8;header[9]=2;
  return Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]),chunk('IHDR',header),chunk('IDAT',deflateSync(scanlines,{level:1})),chunk('IEND',Buffer.alloc(0))]);
}
