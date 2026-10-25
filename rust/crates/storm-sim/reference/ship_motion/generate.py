"""Retained native movement/strength/message bodies and vector math under UBSan.
Character tree/float lookup, MESSAGE, construction and ShipRocking are adapters.
This does not execute native Mount, sail/wind speed providers, sea, collisions,
AI or complete SHIP::Execute. TouchMove is a prediction call, not contact resolution.
"""
from pathlib import Path
import hashlib,json,subprocess,tempfile,re
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/ship/src/ship.cpp').is_file())
ship=(repo/'src/libs/ship/src/ship.cpp').read_text();math=(repo/'src/libs/math/include/math_inlines.h').read_text()
def method(text,sig):
 start=text.index(sig);return text[start:text.index('\n}',start)+2]
sigs=['BOOL SHIP::CalculateNewSpeedVector(','BOOL SHIP::ApplyStrength(','BOOL SHIP::Move(','BOOL SHIP::TouchMove(','int32_t SHIP::AddStrength(','bool SHIP::DelStrength(','float SHIP::GetMaxSpeedZ(','float SHIP::GetMaxSpeedY(']
bodies={sig:method(ship,sig) for sig in sigs};bodies['rotate']=method(math,'inline void RotateAroundY(')
start=ship.index('    case MSG_SHIP_ADD_MOVE_IMPULSE:');bodies['message_case']=ship[start:ship.index('    case MSG_SHIP_SETLIGHTSOFF:',start)]
# Keep the full native state/strength/touch struct declarations and vector header.
header=(repo/'src/libs/ship/include/ship_base.h').read_text();structs=header[header.index('struct SHIP_STATE'):header.index('class SHIP_BASE')]
bodies['state_structs']=structs
ship_header=(repo/'src/libs/ship/src/ship.h').read_text()
messages=(repo/'src/libs/shared_headers/include/shared/messages.h').read_text()
constants={name:int(re.search(r'#define\s+'+name+r'\s+(\d+)',ship_header if name!='MSG_SHIP_ADD_MOVE_IMPULSE' else messages).group(1)) for name in ['MAX_STRENGTH','RESERVED_STRENGTH','MSG_SHIP_ADD_MOVE_IMPULSE']}
assert constants=={'MAX_STRENGTH':16,'RESERVED_STRENGTH':1,'MSG_SHIP_ADD_MOVE_IMPULSE':50308}
math_config='\n'.join(re.search(r'^#define '+name+r'[^\n]+',math,re.M).group(0) for name in ['SIGN','SIGNZ','KNOTS2METERS'])
math_config+='\n'+re.search(r'^#define DELTA_TIME\(x\)[^\n]+',ship_header,re.M).group(0)
math_config+='\n'+re.search(r'^constexpr float PI =[^\n]+',math,re.M).group(0)+'\n'+re.search(r'^constexpr float PId2 =[^\n]+',math,re.M).group(0)
bodies['math_config']=math_config
cases=[]
for i in range(64):
 cases.append({'id':i,'yaw':[-3.0,0.0,0.35,1.5707963705062866][i%4], 'delta_ms':[0,16,17,250,1000][i%5], 'collision':bool(i&1),'dead':bool(i&2),'fixed':bool(i&4),'perk':bool(i&8),'perk_angle':[-1.0,0.0,1.0][i%3], 'mass':[0.0,0.1,-0.25][i%3],'main_z':[0.0,0.00001,0.00002,-0.00001,0.75,-0.5][i%6],'main_x':[0.0,0.00001,-0.25][i%3]})
source=r'''
#include <array>
#include <bit>
#include <cstdint>
#include <iostream>
#include <vector>
#include <string>
#include "c_vector.h"
using BOOL=int32_t;
#define MAX_STRENGTH 16
#define RESERVED_STRENGTH 1
#define SIGN(x) ((x)<0?-1.0f:1.0f)
#define SIGNZ(x) ((x)==0.0f?0.0f:((x)<0?-1.0f:1.0f))
#define KNOTS2METERS(x) ((x)*1853.0f/3600.0f)
#define DELTA_TIME(x) ((x)*0.001f)
constexpr float PId2=3.14159265358979323846f/2.0f;
__STRUCTS__
std::vector<std::string> calls;
struct ATTRIBUTES {float z=12.0f,y=0.08f,value=0;ATTRIBUTES*FindAClass(ATTRIBUTES*,const char* p){std::string s(p);calls.push_back(s=="Ship.MaxSpeedZ"?"z":"y");value=s=="Ship.MaxSpeedZ"?z:y;return this;}float GetAttributeAsFloat(){return value;}};
struct MESSAGE {bool inertia;std::array<float,6> values;size_t cursor=0;int32_t Long(){return inertia?-7:0;}float Float(){return values.at(cursor++);}};
struct SHIP {SHIP_STATE State{};std::array<STRENGTH,MAX_STRENGTH> Strength{};CVECTOR vSpeedAccel=0.0f;float fXOffset=1000,fZOffset=-2000,fFixedSpeed=9,fInitialPerkAngle=0;bool bSetFixed=false,bPerkTurnActive=false,dead=false;ATTRIBUTES character;ATTRIBUTES*GetACharacter(){return &character;}bool isDead(){return dead;}
 CVECTOR ShipRocking(float dt){calls.push_back("rock");State.vPos.y+=dt*0.5f;auto a=State.vAng;a.x+=0.125f*dt;a.z-=0.25f*dt;return a;}
 BOOL CalculateNewSpeedVector(CVECTOR*,CVECTOR*);BOOL ApplyStrength(float,BOOL);BOOL Move(uint32_t,BOOL);BOOL TouchMove(uint32_t,TOUCH_PARAMS*,TOUCH_PARAMS*);int32_t AddStrength(STRENGTH*);bool DelStrength(int32_t);float GetMaxSpeedZ();float GetMaxSpeedY();uint64_t ProcessMessage(MESSAGE&message){switch(50308){__CASE__}return 0;}
};
#define MSG_SHIP_ADD_MOVE_IMPULSE 50308
__BODIES__
uint32_t bits(float f){return std::bit_cast<uint32_t>(f);}void vec(CVECTOR v){std::cout<<"["<<bits(v.x)<<","<<bits(v.y)<<","<<bits(v.z)<<"]";}
void touch(TOUCH_PARAMS*p){if(!p){std::cout<<"null";return;}std::cout<<"[";vec(p->vSpeed);std::cout<<",";vec(p->vRotate);std::cout<<",";vec(p->vPos);std::cout<<",";vec(p->vAng);std::cout<<"]";}
void snapshot(SHIP&s,int result,TOUCH_PARAMS*old=nullptr,TOUCH_PARAMS*next=nullptr){std::cout<<"{\"state\":[";vec(s.State.vPos);std::cout<<",";vec(s.State.vAng);std::cout<<",";vec(s.State.vSpeed);std::cout<<",";vec(s.State.vRotate);std::cout<<",";vec(s.State.vInertiaAccel);std::cout<<",";vec(s.State.vInertiaBrake);std::cout<<","<<bits(s.State.fMassInertia)<<"],\"strengths\":[";bool first=true;for(auto&t:s.Strength){if(!first)std::cout<<",";first=false;std::cout<<"["<<(t.bUse?"true":"false")<<","<<(t.bInertia?"true":"false")<<",";vec(t.vSpeed);std::cout<<",";vec(t.vRotate);std::cout<<"]";}std::cout<<"],\"accel\":";vec(s.vSpeedAccel);std::cout<<",\"calls\":[";first=true;for(auto&c:calls){if(!first)std::cout<<",";first=false;std::cout<<"\""<<c<<"\"";}calls.clear();std::cout<<"],\"result\":"<<result<<",\"old\":";touch(old);std::cout<<",\"new\":";touch(next);std::cout<<"}\n";}
struct Case{float yaw;uint32_t delta;bool collision,dead,fixed,perk;float angle,mass,mainz,mainx;};
int main(){std::vector<Case> cases={__CASES__};for(auto&c:cases){SHIP s;s.State.vPos=CVECTOR(3,7,-5);s.State.vAng=CVECTOR(0.1f,c.yaw,-0.2f);s.State.vSpeed=CVECTOR(2,0.5f,8);s.State.vRotate=CVECTOR(0.1f,0.02f,-0.1f);s.State.vInertiaAccel=CVECTOR(0.5f,0.8f,0.25f);s.State.vInertiaBrake=CVECTOR(0.2f,0.7f,0.45f);s.State.fMassInertia=c.mass;s.Strength[0].bInertia=true;s.Strength[0].vSpeed=CVECTOR(c.mainx,0,c.mainz);s.Strength[0].vRotate=CVECTOR(0,0.3f,0);s.bSetFixed=c.fixed;s.bPerkTurnActive=c.perk;s.fInitialPerkAngle=c.angle;s.dead=c.dead;snapshot(s,0);
 STRENGTH t{};t.bInertia=false;t.vSpeed=CVECTOR(0.375f,-1,2);t.vRotate=CVECTOR(1,0.05f,-0.2f);int result=s.AddStrength(&t);t.bInertia=true;t.vSpeed=CVECTOR(5,4,3);t.vRotate=CVECTOR(0.5f,0.25f,0.75f);result=s.AddStrength(&t);snapshot(s,result);
 MESSAGE m{c.collision,{1.25f,-0.5f,2.5f,-0.25f,0.125f,0.5f}};result=s.ProcessMessage(m);snapshot(s,result);
 TOUCH_PARAMS old{},next{};result=s.TouchMove(c.delta,&old,&next);snapshot(s,result,&old,&next);
 result=s.Move(c.delta,c.collision);snapshot(s,result);
 result=s.Move(1000,false);snapshot(s,result);
 result=s.DelStrength(2);snapshot(s,result);
 t.bInertia=false;for(int i=0;i<17;i++)result=s.AddStrength(&t);snapshot(s,result);
 result=s.DelStrength(-1);result=s.DelStrength(0);result=s.DelStrength(16);snapshot(s,result);
 s.DelStrength(1);result=s.AddStrength(&t);snapshot(s,result);
 for(auto dt:{17u,16u,17u,32u})s.Move(dt,false);snapshot(s,1);
 }}
'''
# Disambiguate adapter literals against the retained float/double CVECTOR constructors.
source=re.sub(r'CVECTOR\(([^()\n]+)\)', lambda m:'CVECTOR('+','.join((part.strip()+'.0f') if re.fullmatch(r'-?\d+',part.strip()) else part for part in m.group(1).split(','))+')',source)
source=re.sub(r'#define SIGN\(x\)[\s\S]+?constexpr float PId2=[^\n]+',math_config,source,count=1)
source=source.replace('__STRUCTS__',structs).replace('__CASE__',bodies['message_case']).replace('__BODIES__','\n'.join(v for k,v in bodies.items() if k not in ['message_case','state_structs','math_config']))
source=source.replace('struct SHIP {','inline void RotateAroundY(float&,float&,float,float);\nstruct SHIP {').replace('#define MSG_SHIP_ADD_MOVE_IMPULSE 50308\n','')
source=source.replace('struct MESSAGE {','#define MSG_SHIP_ADD_MOVE_IMPULSE 50308\nstruct MESSAGE {')
source=source.replace('__CASES__',','.join('{'+','.join([str(c['yaw'])+'f',str(c['delta_ms'])]+[str(c[k]).lower() for k in ['collision','dead','fixed','perk']]+[str(c[k])+'f' for k in ['perk_angle','mass','main_z','main_x']])+'}' for c in cases))
with tempfile.TemporaryDirectory(prefix='storm-ship-motion-') as folder:
 p=Path(folder);(p/'main.cpp').write_text(source)
 subprocess.run(['clang++','-std=c++20','-ffp-contract=off','-fsanitize=undefined','-fno-sanitize-recover=all','-I',str(repo/'src/libs/math/include'),str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
 rows=[json.loads(line) for line in subprocess.check_output([str(p/'oracle')],text=True).splitlines()]
assert len(rows)==len(cases)*11
for i,c in enumerate(cases):c['snapshots']=rows[i*11:(i+1)*11]
metadata={'scope':__doc__,'retained_sha256':{k:hashlib.sha256(v.encode()).hexdigest() for k,v in bodies.items()},'constants':constants,'vector_header_sha256':hashlib.sha256((repo/'src/libs/math/include/c_vector.h').read_bytes()).hexdigest()}
# One complete snapshot per line keeps generated evidence reviewable.
parts=[json.dumps(metadata,indent=2)[:-2]+',\n  "cases": [']
for index,case in enumerate(cases):
 header={k:v for k,v in case.items() if k!='snapshots'}
 parts.append('    '+json.dumps(header,separators=(',',':'))[:-1]+',"snapshots":[')
 for n,row in enumerate(case['snapshots']):parts.append('      '+json.dumps(row,separators=(',',':'))+(',' if n+1<len(case['snapshots']) else ''))
 parts.append('    ]}'+(',' if index+1<len(cases) else ''))
parts.append('  ]\n}')
(here/'cases.json').write_text('\n'.join(parts)+'\n')
print(str(len(cases))+' native movement sequences / '+str(len(rows))+' complete state/strength/touch snapshots under UBSan')
