"""Resize the approved supplied bitmap only; never redraw or recolor the mark."""
from pathlib import Path
import struct
import subprocess
root=Path(__file__).resolve().parent.parent
source=root/'assets/brand/learnharness-sync-app-icon-512.png'
icons=root/'src-tauri/icons'
icons.mkdir(exist_ok=True)
for size in (16,32,48,64,128,256,512,1024):
    target=icons/f'{size}x{size}.png'
    subprocess.run(['magick',str(source),'-resize',f'{size}x{size}',str(target)],check=True)
frames=[((icons/f'{size}x{size}.png').read_bytes(),size) for size in (16,32,48,64,128,256)]
header=struct.pack('<HHH',0,1,len(frames));offset=6+16*len(frames)
entries=[]
for data,size in frames:
    entries.append(struct.pack('<BBBBHHII',size if size<256 else 0,size if size<256 else 0,0,0,1,32,len(data),offset))
    offset+=len(data)
(icons/'icon.ico').write_bytes(header+b''.join(entries)+b''.join(data for data,_ in frames))
chunks=[]
for kind,size in ((b'ic07',128),(b'ic08',256),(b'ic09',512),(b'ic10',1024)):
    data=(icons/f'{size}x{size}.png').read_bytes()
    chunks.append(kind+struct.pack('>I',len(data)+8)+data)
body=b''.join(chunks)
(icons/'icon.icns').write_bytes(b'icns'+struct.pack('>I',len(body)+8)+body)
print('PNG/ICO/ICNS generated from the unchanged canonical bitmap; 1024px is an upsample, not vector artwork.')
