"""Export README previews from the real renderer with illustrative data.

Development only: Node >=22.19, Pillow, and Menlo or DejaVu Sans Mono.
Run from any directory: python3 terminal-ui/scripts/readme-previews.py
"""
import base64
import io
import json
from pathlib import Path
import re
import subprocess
import unicodedata

from PIL import Image, ImageDraw, ImageFont

root = Path(__file__).resolve().parents[2]
output = root/'docs/assets'
output.mkdir(parents=True, exist_ok=True)
source = r'''
import {render} from './terminal-ui/src/renderer.ts';
import {fixture} from './terminal-ui/src/test-fixture.ts';
import {websiteMark} from './terminal-ui/src/brand.ts';
import {setCapabilities} from '@earendil-works/pi-tui';
import {readFileSync} from 'node:fs';
setCapabilities({images:'kitty',trueColor:true,hyperlinks:false});
Date.now=()=>Date.parse('2026-11-15T12:00:00Z');
const version=readFileSync('Cargo.toml','utf8').match(/^version = "([^"]+)"/m)[1];
const base={...fixture(),width:112,height:29,version,cwd:'~/work/atlas',model:'gpt-5.2',provider:'openai',elapsed:10000,statusEntries:[{id:'model',value:'gpt-5.2'},{id:'directory',value:'~/work/atlas'},{id:'branch',value:'main'},{id:'permissions',value:'ask approvals'}]};
const logs=[{level:'Info',text:'USER|Handle HTTP 429 responses in the retry policy.'},{level:'Success',text:'● (Read) src/retry.ts'},{level:'Info',text:'  └ read 24 lines'},{level:'Success',text:'● (Patch) src/retry.ts'},{level:'Info',text:'  └ edited src/retry.ts'},{level:'Info',text:''},{level:'Info',text:'Updated **src/retry.ts** to retry rate-limited responses while keeping the existing retry limit.'},{level:'Info',text:'· Finished in 8.4s'}];
const diffs=[{file:'src/retry.ts',afterLog:4,added:1,removed:1,lines:[{kind:'Header',text:'@@ -1,4 +1,4 @@'},{kind:'Context',text:'export function shouldRetry(status: number, attempts: number) {',newLine:1},{kind:'Context',text:'  if (attempts >= 3) return false;',newLine:2},{kind:'Removed',text:'  return status >= 500;',oldLine:3},{kind:'Added',text:'  return status === 429 || status >= 500;',newLine:3},{kind:'Context',text:'}',newLine:4}]}];
const commandsSource=readFileSync('src/commands.rs','utf8').split('fn command_palette_items()')[1].split('#[cfg(test)]')[0];
const commands=[...commandsSource.matchAll(/cmd:\s*"([^"]+)",\s*desc:\s*"([^"]+)"/g)].map(m=>({cmd:m[1],desc:m[2]}));
if(commands.length<20)throw Error('Could not read the command registry');
const views=[['workspace',{...base,logs,diffs}],['commands',{...base,height:27,input:'/',cursor:1,commands,selectedCommand:2}],['cotton',{...base,theme:'light',logs,diffs}],['usage',{...base,height:44,panel:{title:'Stats',sections:['Commands','Appearance','Model','Permissions','Status line','Keymap','Skills','Usage','Stats','Status'],section:8,selected:0,query:'',rows:[],editing:false,notice:'',footer:'d sample / recorded · tab section · esc close',history:[],demo:true,elapsed:1000}}]];
const exportView=(view)=>{const frame=render(view);return {width:view.width,height:view.height,light:view.theme==='light',rows:frame.rows,cursor:frame.cursor,cursorVisible:frame.cursorVisible!==false,images:frame.images.map(image=>({rect:image.placement?frame.input:frame.artwork,data:[...image.sequence.matchAll(/\x1b_G[^;]*;([A-Za-z0-9+/=]+)\x1b\\/g)].map(m=>m[1]).join('')}))};};
const images=Object.fromEntries(views.map(([name,view])=>[name,exportView(view)]));
images.brand=[websiteMark(false),websiteMark(true)];
images.animation=Array.from({length:41},(_,i)=>exportView({...base,height:40,introElapsed:i*250,introScene:'horizon',introBright:true}));
console.log(JSON.stringify(images));
'''
# Resolve pi-tui from terminal-ui while keeping all file paths relative to the repo.
source = source.replace("from '@earendil-works/pi-tui'", "from './terminal-ui/node_modules/@earendil-works/pi-tui/dist/index.js'")
views = json.loads(subprocess.check_output(['node','--input-type=module','-'], input=source, text=True, cwd=root))
font_path = next((p for p in ['/System/Library/Fonts/Menlo.ttc','/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf'] if Path(p).exists()), None)
assert font_path, 'Install Menlo or DejaVu Sans Mono to export previews'
font = ImageFont.truetype(font_path,18)
fonts = {(False,False):font}
for bold,italic,index,suffix in [(True,False,1,'Bold'),(False,True,2,'Oblique'),(True,True,3,'BoldOblique')]:
    if font_path.endswith('.ttc'): fonts[bold,italic]=ImageFont.truetype(font_path,18,index=index)
    else:
        candidate=Path(font_path).with_name(f'DejaVuSansMono-{suffix}.ttf')
        fonts[bold,italic]=ImageFont.truetype(str(candidate),18) if candidate.exists() else font
cell_w, cell_h, margin = 11, 25, 22
esc = re.compile(r'(\x1b\[[0-9;]*m)')

def brand_pixels(rows):
    pixels = []
    for row, line in enumerate(rows):
        fg, bg, column = None, None, 0
        for token in esc.split(line):
            if token.startswith('\x1b['):
                codes = [int(n) for n in token[2:-1].split(';')]
                if codes == [0]: fg, bg = None, None
                else:
                    assert len(codes) == 5 and codes[:2] in ([38,2],[48,2]), codes
                    color = '#%02x%02x%02x' % tuple(codes[2:])
                    if codes[0] == 38: fg = color
                    else: bg = color
                continue
            for ch in token:
                assert ch in ' █▀▄', ch
                for half, color in enumerate((fg if ch in '█▀' else bg, fg if ch in '█▄' else bg)):
                    if color: pixels.append((column,row*2+half,color))
                column += 1
        assert column == 7, column
    assert len(rows) == 3 and len(pixels) == 14, pixels
    return pixels

# Export the actual terminal glyphs, including their half-cell proportions.
for light, rows in enumerate(views.pop('brand')):
    pixels = brand_pixels(rows)
    ink = '#18181b' if light else '#e9e5dc'
    assert {color for _,_,color in pixels} == {ink,'#ea580c'}
    rects = '\n'.join(f'    <rect x="{x}" y="{y}" width="1" height="1" fill="{color}"/>' for x,y,color in pixels)
    mark = f'  <g transform="translate(0 0) scale(11 12.5)" shape-rendering="crispEdges">\n{rects}\n  </g>'
    (output/f'logo-{"light" if light else "dark"}.svg').write_text(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 77 75" role="img" aria-label="OsmoGrep">\n{mark}\n</svg>\n')
    if not light:
        mark = mark.replace('translate(0 0)', 'translate(29.5 18.5)')
        (output/'readme-brand.svg').write_text(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 432 112" width="432" height="112" role="img" aria-labelledby="title">\n  <title id="title">OsmoGrep</title>\n{mark}\n  <text x="136" y="75" fill="{ink}" font-family="Menlo, Consolas, monospace" font-size="56" font-weight="700" textLength="270" lengthAdjust="spacingAndGlyphs">osmogrep</text>\n</svg>\n')

def preview(view):
    default = (250,250,250) if view['light'] else (0,0,0)
    canvas = Image.new('RGB',(view['width']*cell_w+2*margin,view['height']*cell_h+2*margin),default)
    draw = ImageDraw.Draw(canvas)
    glyphs = []
    for row, line in enumerate(view['rows']):
        fg, bg, inverse, column = (225,220,210), default, False, 0
        bold,italic=False,False
        for token in esc.split(line):
            if token.startswith('\x1b['):
                codes = [int(n or 0) for n in token[2:-1].split(';')]
                i = 0
                while i < len(codes):
                    code = codes[i]
                    if code == 0:
                        fg,bg,inverse=(225,220,210),default,False
                        bold,italic=False,False
                    elif code == 1: bold=True
                    elif code == 22: bold=False
                    elif code == 3: italic=True
                    elif code == 23: italic=False
                    elif code == 7:
                        inverse=True
                    elif code == 27:
                        inverse=False
                    elif code == 49:
                        bg=None
                    elif code in [38,48] and codes[i+1:i+2] == [2]:
                        color=tuple(codes[i+2:i+5])
                        if code == 38: fg=color
                        else: bg=color
                        i+=4
                    i+=1
                continue
            for ch in token:
                cells=2 if unicodedata.east_asian_width(ch) in ['W','F'] else 1
                x,y=margin+column*cell_w,margin+row*cell_h
                ink,paper=(bg or default,fg) if inverse else (fg,bg)
                if paper is not None: draw.rectangle((x,y,x+cell_w*cells-1,y+cell_h-1),fill=paper)
                if ch != ' ': glyphs.append((x,y,ch,ink,bold,italic))
                column+=cells
        assert column == view['width'], (column,view['width'])
    # Inline graphics sit beneath real text. Default-background cells reveal the paper.
    for graphic in view['images']:
        rect=graphic['rect']
        assert rect and graphic['data'], 'Expected a real PNG from the renderer'
        png=Image.open(io.BytesIO(base64.b64decode(graphic['data'])))
        canvas.paste(png.resize((rect['width']*cell_w,rect['height']*cell_h),Image.Resampling.LANCZOS),(margin+rect['x']*cell_w,margin+rect['y']*cell_h))
    draw=ImageDraw.Draw(canvas)
    for x,y,ch,ink,bold,italic in glyphs:
        if ch in ['█','▀','▄','▎']:
            left,top,right,bottom=x,y,x+cell_w-1,y+cell_h-1
            if ch == '▀': bottom=y+cell_h//2-1
            elif ch == '▄': top=y+cell_h//2
            elif ch == '▎': right=x+2
            draw.rectangle((left,top,right,bottom),fill=ink)
        elif 0x2800<=ord(ch)<=0x28ff:
            mask=ord(ch)-0x2800
            for yy,bits in enumerate([[1,8],[2,16],[4,32],[64,128]]):
                for xx,bit in enumerate(bits):
                    if mask&bit:
                        px,py=x+2+xx*5,y+4+yy*5
                        draw.rectangle((px,py,px+1,py+1),fill=ink)
        else: draw.text((x,y+2),ch,font=fonts[bold,italic],fill=ink)
    if view['cursorVisible']:
        cursor=view['cursor'];x,y=margin+cursor['x']*cell_w,margin+cursor['y']*cell_h
        draw.rectangle((x,y+3,x+1,y+cell_h-3),fill=(83,51,28) if view['light'] else (240,214,190))
    return canvas

for name, view in views.items():
    if name != 'animation': preview(view).save(output/f'terminal-{name}.png',optimize=True)
frames=[preview(view) for view in views['animation']]
frames[0].save(output/'terminal-morph.gif',save_all=True,append_images=frames[1:],duration=[250]*40+[1750],loop=0,optimize=True)
print('Exported four terminal previews and one click-replay animation from the current renderer.')
