"""Retained living Execute frame/attribute/root slices plus five full movement
methods, RotateAroundY, native structs, SetAttributeUseFloat and CVECTOR under
UBSan. Character tree/storage, speed values, construction, ShipRocking and CMatrix
argument recording are adapters. Native trig observations separate exact frame
arithmetic from production portable scalar kernels. This is not complete Execute.
"""
from pathlib import Path
import hashlib,json,re,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/ship/src/ship.cpp').is_file())
ship=(repo/'src/libs/ship/src/ship.cpp').read_text()
math=(repo/'src/libs/math/include/math_inlines.h').read_text()
attr=(repo/'src/libs/core/src/attributes.cpp').read_text()
def method(text,sig):
    a=text.index(sig);return text[a:text.index('\n}',a)+2]
sigs=['BOOL SHIP::CalculateNewSpeedVector(','BOOL SHIP::ApplyStrength(','BOOL SHIP::Move(','float SHIP::GetMaxSpeedZ(','float SHIP::GetMaxSpeedY(']
retained={s:method(ship,s) for s in sigs}
retained['rotate']=method(math,'inline void RotateAroundY(')
retained['float_writer']=method(attr,'bool ATTRIBUTES::SetAttributeUseFloat(')
a=ship.index('    vPos = State.vPos;',ship.index('void SHIP::Execute('))
b=ship.index('    /*if (isDead())',a)
retained['living_and_speed']=ship[a:b]
a=ship.index('    auto *pAPos = GetACharacter()->FindAClass(GetACharacter(), "ship.pos");',b)
b=ship.index('    // check strand',a)
retained['position_angles']=ship[a:b]
a=ship.index('    mRoot = CMatrix(vAng.x + vCurDeadDir.x')
retained['root']=ship[a:ship.index(';',a)+1]
header=(repo/'src/libs/ship/include/ship_base.h').read_text()
retained['structs']=header[header.index('struct SHIP_STATE'):header.index('class SHIP_BASE')]
ship_header=(repo/'src/libs/ship/src/ship.h').read_text()
macros='\n'.join(re.search(r'^#define '+n+r'[^\n]+',math,re.M).group(0) for n in ['SIGN','SIGNZ','KNOTS2METERS'])
macros+='\n'+re.search(r'^#define DELTA_TIME\(x\)[^\n]+',ship_header,re.M).group(0)
macros+='\n'+re.search(r'^constexpr float PI =[^\n]+',math,re.M).group(0)+'\n'+re.search(r'^constexpr float PId2 =[^\n]+',math,re.M).group(0)
retained['macros']=macros
source=r'''
#include <array>
#include <bit>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cassert>
#include <iostream>
#include <map>
#include <memory>
#include <string>
#include <vector>
#include "c_vector.h"
using BOOL=int32_t;
#define MAX_STRENGTH 16
#define RESERVED_STRENGTH 1
#define Assert assert
__MACROS__
__STRUCTS__
std::vector<std::array<uint32_t,3>>trig;
std::vector<std::string>calls,created;
std::vector<std::array<std::string,2>>writes;
uint32_t bits(float f){return std::bit_cast<uint32_t>(f);}
float observed_sin(float x){float v=::sinf(x);trig.push_back({0,bits(x),bits(v)});return v;}
float observed_cos(float x){float v=::cosf(x);trig.push_back({1,bits(x),bits(v)});return v;}
#define sinf observed_sin
#define cosf observed_cos
template<size_t N>void sprintf_s(char(&buf)[N],const char*format,float value){std::snprintf(buf,N,format,value);}
struct ATTRIBUTES{
 float z=12.0f,y=0.08f,value=0;std::string path;std::map<std::string,std::unique_ptr<ATTRIBUTES>>nodes;
 ATTRIBUTES*FindAClass(ATTRIBUTES*,const char*p){std::string s(p);if(s=="Ship.MaxSpeedZ"||s=="Ship.MaxSpeedY"){calls.push_back(s=="Ship.MaxSpeedZ"?"z":"y");auto&n=nodes["__speed"];if(!n)n=std::make_unique<ATTRIBUTES>();n->value=s=="Ship.MaxSpeedZ"?z:y;return n.get();}auto it=nodes.find(s);return it==nodes.end()?nullptr:it->second.get();}
 ATTRIBUTES*CreateSubAClass(ATTRIBUTES*,const char*p){auto n=std::make_unique<ATTRIBUTES>();n->path=p;created.push_back(p);auto*r=n.get();nodes[p]=std::move(n);return r;}
 float GetAttributeAsFloat(){return value;}
 size_t SetAttribute(const char*name,const char*text){writes.push_back({path+"."+name,text});return 1;}
 void SetValue(const char*){std::abort();}bool SetAttributeUseFloat(const char*,float);
};
struct CMatrix{CVECTOR position=0.0f,angles=0.0f;CMatrix()=default;CMatrix(float ax,float ay,float az,float x,float y,float z):position(x,y,z),angles(ax,ay,az){}};
struct SHIP{
 SHIP_STATE State{};std::array<STRENGTH,16>Strength{};CVECTOR vSpeedAccel=0.0f,vPos=0.0f,vAng=0.0f,vCurDeadDir=0.0f;CMatrix mRoot;struct{float fWaterLine=0.5f;}SP;
 float fXOffset=1000,fZOffset=-2000,fFixedSpeed=9,fInitialPerkAngle=0;bool bSetFixed=false,bPerkTurnActive=false;ATTRIBUTES character;
 bool isDead(){return false;}ATTRIBUTES*GetACharacter(){return &character;}
 CVECTOR ShipRocking(float dt){calls.push_back("rock");State.vPos.y+=dt*0.5f;auto a=State.vAng;a.x+=0.125f*dt;a.z-=0.25f*dt;return a;}
 BOOL CalculateNewSpeedVector(CVECTOR*,CVECTOR*);BOOL ApplyStrength(float,BOOL);BOOL Move(uint32_t,BOOL);float GetMaxSpeedZ();float GetMaxSpeedY();
 void Frame(uint32_t DeltaTime){__FRAME__}
};
__BODIES__
void vec(CVECTOR v){std::cout<<"["<<bits(v.x)<<","<<bits(v.y)<<","<<bits(v.z)<<"]";}
void row(SHIP&s){std::cout<<"{\"state\":[";vec(s.State.vPos);std::cout<<",";vec(s.State.vAng);std::cout<<",";vec(s.State.vSpeed);std::cout<<",";vec(s.State.vRotate);std::cout<<"],\"pose\":[";vec(s.vPos);std::cout<<",";vec(s.vAng);std::cout<<"],\"root\":[";vec(s.mRoot.position);std::cout<<",";vec(s.mRoot.angles);std::cout<<"],\"accel\":";vec(s.vSpeedAccel);std::cout<<",\"strengths\":[";bool first=true;for(auto&t:s.Strength){if(!first)std::cout<<",";first=false;std::cout<<"["<<(t.bUse?"true":"false")<<","<<(t.bInertia?"true":"false")<<",";vec(t.vSpeed);std::cout<<",";vec(t.vRotate);std::cout<<"]";}std::cout<<"],\"calls\":[";first=true;for(auto&c:calls){if(!first)std::cout<<",";first=false;std::cout<<"\""<<c<<"\"";}std::cout<<"],\"trig\":[";first=true;for(auto&t:trig){if(!first)std::cout<<",";first=false;std::cout<<"["<<t[0]<<","<<t[1]<<","<<t[2]<<"]";}std::cout<<"],\"created\":[";first=true;for(auto&c:created){if(!first)std::cout<<",";first=false;std::cout<<"\""<<c<<"\"";}std::cout<<"],\"writes\":[";first=true;for(auto&w:writes){if(!first)std::cout<<",";first=false;std::cout<<"[\""<<w[0]<<"\",\""<<w[1]<<"\"]";}std::cout<<"]}\n";calls.clear();trig.clear();created.clear();writes.clear();}
struct Case{float yaw,turn,mainz;bool fixed,perk;float angle;};
int main(){std::vector<Case>cases={__CASES__};for(auto&c:cases){SHIP s;s.State.vPos=CVECTOR(3.0f,7.0f,-5.0f);s.State.vAng=CVECTOR(0.125f,c.yaw,-0.2f);s.State.vSpeed=CVECTOR(2.0f,0.5f,8.0f);s.State.vRotate=CVECTOR(0.1f,c.turn,-0.1f);s.State.vInertiaAccel=CVECTOR(0.5f,0.8f,0.25f);s.State.vInertiaBrake=CVECTOR(0.2f,0.7f,0.45f);s.State.fMassInertia=0.1f;s.State.fShipImmersion=0.75f;s.bSetFixed=c.fixed;s.bPerkTurnActive=c.perk;s.fInitialPerkAngle=c.angle;s.Strength[0].bInertia=true;s.Strength[0].vSpeed=CVECTOR(0.0f,0.0f,c.mainz);s.Strength[0].vRotate=CVECTOR(0.0f,0.3f,0.0f);
int step=0;for(auto dt:{0u,17u,16u,250u,1000u,17u}){if(step==2){s.Strength[1].bUse=true;s.Strength[1].vSpeed=CVECTOR(0.375f,-1.0f,2.0f);s.Strength[1].vRotate=CVECTOR(1.0f,0.8f,-0.2f);}if(step==4){s.Strength[0].vSpeed.z=-s.Strength[0].vSpeed.z;s.Strength[0].vRotate.y=-s.Strength[0].vRotate.y;}s.Frame(dt);row(s);step++;}}}
'''
source=source.replace('__MACROS__',macros).replace('__STRUCTS__',retained['structs'])
source=source.replace('__FRAME__',retained['living_and_speed']+retained['position_angles']+retained['root'])
source=source.replace('__BODIES__','\n'.join(retained[k] for k in [*sigs,'rotate','float_writer']))
cases=[]
for i in range(48):
 cases.append(dict(id=i,yaw=[-3.0,0.0,0.35,1.5707963705062866][i%4],turn=[-0.6,-0.25,-0.0,0.0,0.25,0.6][i%6],mainz=[0.75,-0.5][i%2]+0.125*(i//12),fixed=bool(i&1),perk=bool(i&2),angle=[-1.0,0.0,1.0][i%3]))
assert len({json.dumps({k:v for k,v in c.items() if k!='id'},sort_keys=True) for c in cases})==48
source=source.replace('__CASES__',','.join('{'+','.join([str(c[k])+'f' for k in ['yaw','turn','mainz']]+[str(c[k]).lower() for k in ['fixed','perk']]+[str(c['angle'])+'f'])+'}' for c in cases))
with tempfile.TemporaryDirectory(prefix='storm-ship-presentation-') as folder:
 p=Path(folder);(p/'main.cpp').write_text(source)
 subprocess.run(['clang++','-std=c++20','-ffp-contract=off','-fsanitize=undefined','-fno-sanitize-recover=all','-I',str(repo/'src/libs/math/include'),str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
 rows=[json.loads(s) for s in subprocess.check_output([str(p/'oracle')],text=True).splitlines()]
assert len(rows)==288
for i,c in enumerate(cases):c['snapshots']=rows[i*6:(i+1)*6]
metadata={'scope':__doc__,'retained_sha256':{k:hashlib.sha256(v.encode()).hexdigest() for k,v in retained.items()},'vector_header_sha256':hashlib.sha256((repo/'src/libs/math/include/c_vector.h').read_bytes()).hexdigest()}
parts=[json.dumps(metadata,indent=2)[:-2]+',\n  "cases": [']
for i,c in enumerate(cases):
 parts.append('    '+json.dumps({k:v for k,v in c.items() if k!='snapshots'},separators=(',',':'))[:-1]+',"snapshots":[')
 parts.extend('      '+json.dumps(s,separators=(',',':'))+(',' if j<5 else '') for j,s in enumerate(c['snapshots']))
 parts.append('    ]}'+(',' if i+1<len(cases) else ''))
parts.append('  ]\n}')
(here/'cases.json').write_text('\n'.join(parts)+'\n')
print('48 native living-frame sequences / 288 complete snapshots under UBSan')
