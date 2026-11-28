"""Create a new asset-free-game fixture using the licensed upstream cannon GM.

Only the repository's existing tool mesh is copied; texture pixels, engine.ini
and Storm source are newly generated. The destination must not already exist.
"""
from pathlib import Path
import argparse,struct
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('destination',type=Path)
args=parser.parse_args()
root=Path(__file__).resolve().parents[4]
source=(root/'tools/modding-tool/Meshes/cannon_x.gm').read_bytes()
assert source[:4]==b'50.1'
destination=args.destination
destination.mkdir(parents=True,exist_ok=False)
(destination/'RESOURCE/MODELS').mkdir(parents=True)
(destination/'RESOURCE/Textures').mkdir()
(destination/'PROGRAM').mkdir()
(destination/'RESOURCE/MODELS/cannon.gm').write_bytes(source)
word=lambda i:struct.unpack_from('<I',source,i*4)[0]
names=source[72:72+word(2)];table=72+word(2)+word(3)*4
for i in range(word(4)):
    offset=struct.unpack_from('<I',source,table+i*4)[0]
    name=names[offset:names.index(b'\0',offset)].decode('cp1251').replace('\\','/')
    assert not name.startswith('/') and ':' not in name and '..' not in Path(name).parts
    texture=destination/'RESOURCE/Textures'/(name if name.lower().endswith('.tx') else name+'.tx')
    texture.parent.mkdir(parents=True,exist_ok=True)
    texture.write_bytes(struct.pack('<6I',0,1,1,1,21,4)+bytes([16,160,220,255]))
(destination/'engine.ini').write_text('''program_directory=PROGRAM
run=seadogs.c
screen_x=320
screen_y=240
window_borders=1
run_in_background=1
max_fps=120
vsync=0
''',encoding='utf-8')
(destination/'PROGRAM/seadogs.c').write_text('''// Newly written compatibility source, not commercial game scripts.
object cannon;int frames;
void Main(){
    CreateEntity(&cannon,"MODELR");
    int loaded=SendMessage(cannon,"ls",20500,"cannon");Trace("model-loaded="+loaded);
    LayerAddObject(1,cannon,0);
    SetEventHandler("frame","OnFrame",0);
    SetEventHandler("DestroyWindow","OnDestroy",0);
    SetEventHandler("ExitApplication","OnExit",0);
}
void OnFrame(){frames++;Trace("frame="+frames);Trace("delta="+GetDeltaTime());}
void OnDestroy(){Trace("destroy");}
void OnExit(){Trace("exit");}
''',encoding='utf-8')
print('Created static-model engine fixture:',destination)
