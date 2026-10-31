"""Unedited SetRolling, DoRollingStep and TurnSail with full retained geometry
and vector declarations under UBSan. Node matrices are observed call ports;
the rope entity service is absent. Matrix construction, rope-constrained turning,
SetGeometry and cloth vertex generation are excluded.
"""
from pathlib import Path
import hashlib,json,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/rigging/src/sailone.cpp').is_file())
text=(repo/'src/libs/rigging/src/sailone.cpp').read_text()
def method(name):
 a=text.index(name);return text[a:text.index('\n}',a)+2]
parts={k:method('void SAILONE::'+k) for k in ['SetRolling(bool','DoRollingStep(uint32_t','TurnSail(float']}
h=(repo/'src/libs/rigging/src/sailone.h').read_text();a=h.index('struct SAILGEOMETRY');geometry=h[a:h.index('struct ROTATEROPEDSAIL',a)]
source=r'''
#include <array>
#include <bit>
#include <cstring>
#include <iostream>
#include <vector>
#include <cstdint>
#include <cstdlib>
#include "c_vector.h"
using entid_t=uint64_t;
enum SAIL_TYPE{SAIL_TREANGLE,SAIL_SQUARE,SAIL_TRAPECIDAL,SAIL_GERBED,SAIL_ROLLING,SAIL_SPECIAL};
__GEOMETRY__
uint32_t bits(float f){return std::bit_cast<uint32_t>(f);}
std::vector<std::vector<uint32_t>>calls;int globals=0;
struct CMatrix{CVECTOR p;CVECTOR Pos(){calls.push_back({0});return p;}void SetPosition(float x,float y,float z){p=CVECTOR(x,y,z);calls.push_back({1,bits(x),bits(y),bits(z)});}void SetPosition(CVECTOR v){SetPosition(v.x,v.y,v.z);}void RotateY(float a){calls.push_back({2,bits(a)});}void EqMultiply(const CMatrix&,const CMatrix&){calls.push_back({3});globals++;}};
struct NODE{CMatrix loc_mtx,glob_mtx;NODE*parent=nullptr;};
struct ROPE_BASE{void GetEndPoint(CVECTOR*,int,entid_t){std::abort();}};
struct Core{entid_t GetEntityId(const char*){return 0;}void*GetEntityPointer(entid_t){std::abort();}}core;
struct ROPE{int tiePoint=1,ropenum=0;float r1=1,r2=1;CVECTOR b;};
struct Parent{float TURNSTEPANGL=.002f,WINDANGL_DISCRETE=.01f;struct{float fRollingSpeed=.0003f;entid_t modelEI=0;}gdata[1];};
struct SAILONE{
SAILGEOMETRY sgeo{};SAILROLLING*sroll=nullptr;bool bRolling=false;float oldWindAngl=0,m_fMinAngle=.6f,m_fMaxAngle=.6f;Parent*pp;int HostNum=0;
struct{SAIL_TYPE eSailType;CVECTOR hardPoints[4];}ss;
NODE*hostNode;CVECTOR sailWind;struct{bool pnttie[4]{};CVECTOR*pPos[4]{};ROPE*rrs[2]{};}sailtrope;
void SetRolling(bool);void DoRollingStep(uint32_t);void TurnSail(float);void SetGeometry(){std::abort();}
};
__METHODS__
std::array<CVECTOR*,10>vectors(SAILGEOMETRY&g){return {&g.cv.dVv,&g.cv.ddVv,&g.cv.dddVv,&g.cv.dVh,&g.cv.ddVh,&g.cv.normL,&g.cv.dnormL,&g.cv.normR,&g.cv.dnormR,&g.cv.dopV};}
void vec(CVECTOR v){std::cout<<"["<<bits(v.x)<<","<<bits(v.y)<<","<<bits(v.z)<<"]";}
void geo(SAILGEOMETRY&g){std::cout<<"[";bool first=true;for(auto*v:vectors(g)){if(!first)std::cout<<",";first=false;vec(*v);}std::cout<<"]";}
void snapshot(SAILONE&s){std::cout<<"{\"geometry\":";geo(s.sgeo);std::cout<<",\"rolling\":"<<int(s.bRolling)<<",\"angle\":"<<bits(s.oldWindAngl)<<",\"transition\":";if(s.sroll){std::cout<<"{\"geometry\":";geo(s.sroll->oldgeo);std::cout<<",\"delta\":"<<bits(s.sroll->delta)<<",\"up\":"<<int(s.sroll->rollup)<<"}";}else std::cout<<"null";
std::cout<<",\"position\":";vec(s.hostNode->loc_mtx.p);std::cout<<",\"globals\":"<<globals<<",\"ties\":[";for(int i=0;i<4;i++){if(i)std::cout<<",";if(s.sailtrope.pnttie[i])vec(*s.sailtrope.pPos[i]);else std::cout<<"null";}std::cout<<"],\"calls\":[";bool first=true;for(auto&c:calls){if(!first)std::cout<<",";first=false;std::cout<<"[";for(size_t i=0;i<c.size();i++){if(i)std::cout<<",";std::cout<<c[i];}std::cout<<"]";}std::cout<<"]}\n";calls.clear();}
int main(){for(int id=0;id<64;id++){
Parent p;NODE parent,node;node.parent=&parent;node.loc_mtx.p=CVECTOR(3.f+id,-2.f,.25f);SAILONE s;s.pp=&p;s.hostNode=&node;s.ss.eSailType=SAIL_TYPE(id%6);s.bRolling=id%2;s.oldWindAngl=(id%3-1)*.125f;s.m_fMinAngle=.6f+float(id%4)*.125f;s.m_fMaxAngle=.6f-float(id%3)*.2f;p.gdata[0].fRollingSpeed=std::array<float,4>{.0003f,.001f,0.f,-.0001f}[id%4];
CVECTOR ties[4];for(int i=0;i<4;i++){s.ss.hardPoints[i]=CVECTOR(float(i),float(id)*.25f,-float(i));ties[i]=CVECTOR(-float(i),1.f,2.f);s.sailtrope.pnttie[i]=(id&(1<<i))!=0;s.sailtrope.pPos[i]=&ties[i];}
int j=0;for(auto*v:vectors(s.sgeo)){*v=CVECTOR(float(id)*.125f+j,-float(j)*.25f,float(id%5)*.5f+.125f);j++;}globals=0;snapshot(s);
auto set=[&](bool up){s.SetRolling(up);snapshot(s);};auto step=[&](uint32_t ms){s.DoRollingStep(ms);snapshot(s);};auto turn=[&](float x,float limit){s.sailWind.x=x;s.TurnSail(limit);snapshot(s);};
set(!(id%2));set(!(id%2));step(0);step(1000);turn(float(id%5)*.5f-1.f,.034f);step(1000);set(id%2);step(2000);step(2000);turn(1.f-float(id%5)*.5f,.1f);set(false);step(4000);step(0);step(17);set(true);step(4000);
delete s.sroll;
}}
'''
source=source.replace('__GEOMETRY__',geometry).replace('__METHODS__','\n'.join(parts.values()))
with tempfile.TemporaryDirectory(prefix='storm-sail-motion-') as folder:
 p=Path(folder);(p/'main.cpp').write_text(source)
 subprocess.run(['clang++','-std=c++20','-ffp-contract=off','-fsanitize=undefined','-fno-sanitize-recover=all','-I',str(repo/'src/libs/math/include'),str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
 rows=[json.loads(x) for x in subprocess.check_output([str(p/'oracle')],text=True).splitlines()]
assert len(rows)==64*17
cases=[dict(id=i,snapshots=rows[i*17:(i+1)*17]) for i in range(64)]
parts['geometry']=geometry
(here/'cases.json').write_text(json.dumps(dict(scope=__doc__,retained_sha256={k:hashlib.sha256(v.encode()).hexdigest() for k,v in parts.items()},cases=cases),separators=(',',':'))+'\n')
print('64 distinct native rolling/turn profiles / 1088 complete represented snapshots under UBSan')
