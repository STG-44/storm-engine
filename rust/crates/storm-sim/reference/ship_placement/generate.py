"""Unedited CalculateImmersion and retained waterline/root expressions under UBSan.
Attributes, SHIP construction and CMatrix component storage are adapters.
This records Euler/translation arguments, not native matrix construction or Execute.
"""
from pathlib import Path
import hashlib,json,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/ship/src/ship.cpp').is_file())
text=(repo/'src/libs/ship/src/ship.cpp').read_text()
start=text.index('void SHIP::CalculateImmersion()')
method=text[start:text.index('\n}',start)+2]
start=text.index('    mRoot = CMatrix(vAng.x + vCurDeadDir.x')
root=text[start:text.index(';',start)+1]
waterline='    SP.fWaterLine = GetAFloat("WaterLine");'
assert waterline in text
source=r'''
#include <array>
#include <bit>
#include <iostream>
#include <cstdint>
#include <string>
#include "c_vector.h"
struct ATTRIBUTES{bool present=false;float value=0;ATTRIBUTES*FindAClass(ATTRIBUTES*,const char*p){if(std::string(p)!="Ship.Immersion")std::abort();return present?this:nullptr;}float GetAttributeAsFloat(const char* =nullptr){return value;}};
struct CMatrix{std::array<float,3>angles{},position{};CMatrix()=default;CMatrix(float ax,float ay,float az,float x,float y,float z):angles{ax,ay,az},position{x,y,z}{}};
struct SHIP{struct{float fShipImmersion=0;}State;struct{float fWaterLine=0;}SP;ATTRIBUTES character,ship;CVECTOR vPos,vAng,vCurDeadDir;float fXOffset=0,fZOffset=0;CMatrix mRoot;ATTRIBUTES*GetACharacter(){return &character;}ATTRIBUTES*GetAShip(){return &ship;}void CalculateImmersion();void Load(){
#define GetAFloat(x) GetAShip()->GetAttributeAsFloat(x)
__WATERLINE__
}void Place(){__ROOT__}};
__METHOD__
uint32_t bits(float f){return std::bit_cast<uint32_t>(f);}
void vec(std::array<float,3>v){std::cout<<"["<<bits(v[0])<<","<<bits(v[1])<<","<<bits(v[2])<<"]";}
struct Case{float immersion,waterline,x,y,z,yaw,offsetx,offsetz,tiltx,tiltz;bool present;};
int main(){std::array<Case,64>cases={{__CASES__}};for(auto&c:cases){SHIP s;s.character.present=c.present;s.character.value=c.immersion;s.ship.value=c.waterline;s.Load();s.vPos=CVECTOR(c.x,c.y,c.z);s.vAng=CVECTOR(0.125f,c.yaw,-0.25f);s.vCurDeadDir=CVECTOR(c.tiltx,0.0f,c.tiltz);s.fXOffset=c.offsetx;s.fZOffset=c.offsetz;
for(int step=0;step<3;step++){if(step==1){s.character.present=true;s.character.value=-c.immersion;}if(step==2)s.character.present=false;s.CalculateImmersion();s.Place();std::cout<<"{\"immersion\":"<<bits(s.State.fShipImmersion)<<",\"position\":";vec(s.mRoot.position);std::cout<<",\"angles\":";vec(s.mRoot.angles);std::cout<<"}\n";}}}
'''
cases=[]
for i in range(64):
    cases.append(dict(id=i,immersion=[0.0,0.75,-1.25,2.5][i%4],waterline=[0.0,0.5,-0.375,3.0][(i//4)%4],
        x=[0.0,3.5,-10.0,100000000.0][(i//3)%4],y=[0.0,7.25,-3.5,100000000.0][(i//5)%4],z=-5.5,
        yaw=[0.0,-1.25,3.0][i%3],offsetx=[0.0,1000.0,-100000000.0][i%3],offsetz=[0.0,-2000.0,25.0][i%3],
        tiltx=[0.0,0.0625,-0.25][i%3],tiltz=[0.125,0.0,-0.0625][i%3],present=i%5!=0))
keys=['immersion','waterline','x','y','z','yaw','offsetx','offsetz','tiltx','tiltz']
source=source.replace('__METHOD__',method).replace('__ROOT__',root).replace('__WATERLINE__',waterline)
source=source.replace('__CASES__',','.join('{'+','.join([str(c[k])+'f' for k in keys]+[str(c['present']).lower()])+'}' for c in cases))
with tempfile.TemporaryDirectory(prefix='storm-ship-placement-') as folder:
    p=Path(folder);(p/'main.cpp').write_text(source)
    subprocess.run(['clang++','-std=c++20','-ffp-contract=off','-fsanitize=undefined','-fno-sanitize-recover=all','-I',str(repo/'src/libs/math/include'),str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
    rows=[json.loads(s) for s in subprocess.check_output([str(p/'oracle')],text=True).splitlines()]
assert len(rows)==192
for i,c in enumerate(cases):c['snapshots']=rows[i*3:(i+1)*3]
out={'scope':__doc__,'retained_sha256':{k:hashlib.sha256(v.encode()).hexdigest() for k,v in [('calculate',method),('root',root),('waterline',waterline)]},'cases':cases}
(here/'cases.json').write_text(json.dumps(out,indent=2)+'\n')
print('64 native placement sequences / 192 complete snapshots under UBSan')
