"""Unedited LoadShipParameters, numeric Mount slice, ship_t, SHIP_STATE and
CVECTOR under UBSan. Named attribute values, construction, geometry box info and
output are adapters. Full mounting/services and native string parsing are not
executed. Integer cases supply native DWORD bits; parsing is tested separately.
"""
from pathlib import Path
import hashlib,json,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/ship/src/ship.cpp').is_file())
ship=(repo/'src/libs/ship/src/ship.cpp').read_text()
a=ship.index('BOOL SHIP::LoadShipParameters()');load=ship[a:ship.index('\n}',a)+2]
a=ship.index('    State.vBoxSize.x = ginfo.boxsize.x;');b=ship.index('    State.fWeight = SP.fWeight;',a)
mount=ship[a:b+len('    State.fWeight = SP.fWeight;')]
h=(repo/'src/libs/ship/include/ship_base.h').read_text();state=h[h.index('struct SHIP_STATE'):h.index('struct STRENGTH')]
h=(repo/'src/libs/ship/src/ship_msg.h').read_text();params=h[h.index('struct ship_t'):h.index('//#define MSG_SHIP_CREATE')]
source=r'''
#include <array>
#include <bit>
#include <cassert>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <map>
#include <string>
#include <vector>
#include "c_vector.h"
using BOOL=int32_t;
#define MAX_CANNON_NAME_LENGTH 100
__STATE__
__PARAMS__
std::vector<std::string>calls;
struct ATTRIBUTES{std::map<std::string,float>f;std::map<std::string,uint32_t>d;
float GetAttributeAsFloat(const char*n){calls.push_back(n);return f.contains(n)?f.at(n):0.0f;}
uint32_t GetAttributeAsDword(const char*n){calls.push_back(n);return d.contains(n)?d.at(n):0;}};
struct SHIP{SHIP_STATE State{};ship_t SP{};float fGravity=0;ATTRIBUTES attributes;ATTRIBUTES*GetAShip(){return &attributes;}BOOL LoadShipParameters();
void Mount(CVECTOR size){struct{CVECTOR boxsize;}ginfo{size};__MOUNT__}};
__LOAD__
uint32_t bits(float v){return std::bit_cast<uint32_t>(v);}void vec(CVECTOR v){std::cout<<"["<<bits(v.x)<<","<<bits(v.y)<<","<<bits(v.z)<<"]";}
void snapshot(SHIP&s){auto&p=s.SP;auto&t=s.State;std::cout<<"{\"integers\":["<<p.iClass<<","<<p.iCrewMin<<","<<p.iCrewMax<<","<<p.iCapacity<<"],\"floats\":["<<bits(p.fSpeedRate)<<","<<bits(p.fTurnRate)<<","<<bits(p.fWeight)<<","<<bits(p.fSpeedDependW)<<","<<bits(p.fSubSeaDependW)<<","<<bits(p.fTurnDependW)<<","<<bits(p.fTurnDependS)<<","<<bits(p.fWaterLine)<<"],\"inertia\":[";vec(t.vInertiaAccel);std::cout<<",";vec(t.vInertiaBrake);std::cout<<"],\"mount\":["<<bits(p.fLength)<<","<<bits(p.fWidth)<<","<<bits(t.fLoad)<<","<<bits(t.fShipSpeedZ)<<","<<bits(t.fShipSpeedY)<<","<<bits(t.fInertiaFactor)<<","<<bits(t.fMassInertia)<<","<<bits(t.fWeight)<<","<<bits(s.fGravity)<<"],\"calls\":[";bool first=true;for(auto&c:calls){if(!first)std::cout<<",";first=false;std::cout<<"\""<<c<<"\"";}std::cout<<"]}\n";calls.clear();}
struct Case{float speed,turn,weight,waterline,scale,x,y,z;int32_t capacity;uint32_t cls;bool absent;};
int main(){std::vector<Case>cases={__CASES__};for(auto&c:cases){SHIP s;s.SP.fLength=99;s.SP.fWidth=88;std::strcpy(s.SP.name,"kept");s.State.vPos=CVECTOR(3.0f,7.0f,-5.0f);s.State.fLoad=6;s.State.fShipSpeedZ=4;s.State.fShipSpeedY=3;s.State.fInertiaFactor=8;s.State.fMassInertia=7;s.State.fWeight=5;
if(!c.absent){s.attributes.d={{"Class",c.cls},{"MinCrew",static_cast<uint32_t>(-1)},{"MaxCrew",120},{"Capacity",static_cast<uint32_t>(c.capacity)}};
s.attributes.f={{"SpeedRate",c.speed},{"TurnRate",c.turn},{"Weight",c.weight},{"SpeedDependWeight",0.125f},{"SubSeaDependWeight",-0.25f},{"TurnDependWeight",0.5f},{"TurnDependSpeed",0.75f},{"WaterLine",c.waterline},{"InertiaAccelerationY",c.scale},{"InertiaBrakingY",-c.scale},{"InertiaAccelerationX",2*c.scale},{"InertiaBrakingX",-2*c.scale},{"InertiaAccelerationZ",3*c.scale},{"InertiaBrakingZ",-3*c.scale}};}
s.LoadShipParameters();assert(s.SP.fLength==99&&s.SP.fWidth==88&&std::strcmp(s.SP.name,"kept")==0);assert(s.State.vPos.x==3&&s.State.vPos.y==7&&s.State.vPos.z==-5);snapshot(s);s.Mount(CVECTOR(c.x,c.y,c.z));snapshot(s);
s.attributes.f.erase("SpeedRate");s.attributes.f["InertiaAccelerationY"]=0.875f;s.LoadShipParameters();snapshot(s);}}
'''
cases=[]
for i in range(64):
 cases.append(dict(id=i,speed=[0.0,12.5,-3.25,18.75][i%4],turn=[0.0,35.52,-100.0][i%3],weight=50.0+i*1.25,
  waterline=[0.0,0.5,-0.75][i%3],scale=[0.0,0.125,-0.5,1.5][(i//4)%4],x=3.0+i*0.25,y=8.0,z=12.0+i,
  capacity=[0,1000,-1,2147483647][(i//2)%4],cls=[0,1,2147483648,4294967295][i%4],absent=i%7==0))
assert len({json.dumps({k:v for k,v in c.items() if k!='id'},sort_keys=True) for c in cases})==64
source=source.replace('__STATE__',state).replace('__PARAMS__',params).replace('__MOUNT__',mount).replace('__LOAD__',load)
source=source.replace('__CASES__',','.join('{'+','.join([str(c[k])+'f' for k in ['speed','turn','weight','waterline','scale','x','y','z']]+[str(c['capacity']),str(c['cls'])+'u',str(c['absent']).lower()])+'}' for c in cases))
with tempfile.TemporaryDirectory(prefix='storm-ship-parameters-') as folder:
 p=Path(folder);(p/'main.cpp').write_text(source)
 subprocess.run(['clang++','-std=c++20','-ffp-contract=off','-fsanitize=undefined','-fno-sanitize-recover=all','-I',str(repo/'src/libs/math/include'),str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
 rows=[json.loads(s) for s in subprocess.check_output([str(p/'oracle')],text=True).splitlines()]
assert len(rows)==192
for i,c in enumerate(cases):c['snapshots']=rows[i*3:(i+1)*3]
out={'scope':__doc__,'retained_sha256':{k:hashlib.sha256(v.encode()).hexdigest() for k,v in [('load',load),('mount',mount),('state',state),('parameters',params)]},'cases':cases}
(here/'cases.json').write_text(json.dumps(out,indent=2)+'\n')
print('64 distinct native parameter profiles / 192 complete snapshots under UBSan')
