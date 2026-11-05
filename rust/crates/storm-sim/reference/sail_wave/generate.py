"""Unedited attached GoVWave/GoTWave and goWave dispatcher under UBSan.
SetGeometry/FillVertex/SetTexGrid initialize the full retained grid. Wind table,
phase/flex initialization, local wind, LOD and rope storage are explicit ports.
Rolled/free processors abort if reached; no native renderer, RNG or matrix claims.
"""
from pathlib import Path
import hashlib,json,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/rigging/src/sailone.cpp').is_file())
cpp=(repo/'src/libs/rigging/src/sailone.cpp').read_text();h=(repo/'src/libs/rigging/src/sailone.h').read_text()
def method(name):
 a=cpp.index('void SAILONE::'+name);return cpp[a:cpp.index('\n}',a)+2]
names=['SetGeometry()','FillVertex(SAILVERTEX','SetTexGrid(SAILVERTEX','GoVWave(SAILVERTEX','GoTWave(SAILVERTEX','goWave(SAILVERTEX']
parts={n:method(n) for n in names}
a=h.index('struct SAILGEOMETRY');parts['geometry']=h[a:h.index('struct ROTATEROPEDSAIL',a)]
a=h.index('#define SAIL_COL_MAX');parts['vertex']=h[a:h.index('// parameters for sail triangulation',a)]
source=r'''
#include <array>
#include <bit>
#include <cstring>
#include <iostream>
#include <cstdint>
#include <cstdlib>
#include "c_vector.h"
enum SAIL_TYPE{SAIL_TREANGLE,SAIL_SQUARE,SAIL_TRAPECIDAL,SAIL_GERBED,SAIL_ROLLING,SAIL_SPECIAL};
__DECLARATIONS__
struct Parent {
float fSHoleFlexDepend=.01f, fTHoleFlexDepend=.01f, TsailWindDepend=.5f, SsailWindDepend=.05f, fWindAdding=.3f, FLEXSPEED=.001f;
int WINDVECTOR_QUANTITY=13,WINDVECTOR_TINCR=3,WINDVECTOR_TADD=3,WINDVECTOR_SINCR=6,WINDVECTOR_SADD=3;
struct {float base;}globalWind;
float WindVect[13]={0.f,.25f,.5f,.75f,1.f,.5f,.25f,0.f,-.25f,-.75f,-1.f,-.5f,-.25f};
};
struct SAILONE {
SAILGEOMETRY sgeo{}; SAILROLLING*sroll=nullptr; Parent*pp;
bool bRolling=false,bFreeSail=false,WindUp=false; int VertIdx=0,HorzIdx=0,wind_incr=1,wind_add=1;
float SumWind=0,MaxSumWind=0,sailHeight=1,m_fHorzGeraldScale=1,m_fVertGeraldScale=1;
uint32_t m_dwRow=17,m_dwCol=13; CVECTOR sailWind,SailPnt[20]{};
struct {SAIL_TYPE eSailType; CVECTOR hardPoints[4],boxCenter,boxSize;struct {CVECTOR c;float r;}boundSphere;float fDeepZ,fDeepH,fDeepVh,fDeepVz;int holeCount;bool bYesLimitPoint;}ss{};
struct {bool pnttie[4]{};CVECTOR*pPos[4];}sailtrope;
void SetGeometry();void FillVertex(SAILVERTEX*);void SetTexGrid(SAILVERTEX*)const;
void GoVWave(SAILVERTEX*);void GoTWave(SAILVERTEX*);void goWave(SAILVERTEX*,uint32_t);
void DoSRollSail(SAILVERTEX*){std::abort();}void DoTRollSail(SAILVERTEX*){std::abort();}
void DoSFreeSail(SAILVERTEX*){std::abort();}void DoTFreeSail(SAILVERTEX*){std::abort();}
};
__METHODS__
uint32_t bits(float f){return std::bit_cast<uint32_t>(f);}
void vec(CVECTOR v){std::cout<<"["<<bits(v.x)<<","<<bits(v.y)<<","<<bits(v.z)<<"]";}
void geometry(SAILGEOMETRY&g){CVECTOR*v[]={&g.cv.dVv,&g.cv.ddVv,&g.cv.dddVv,&g.cv.dVh,&g.cv.ddVh,&g.cv.normL,&g.cv.dnormL,&g.cv.normR,&g.cv.dnormR,&g.cv.dopV};std::cout<<"[";for(int i=0;i<10;i++){if(i)std::cout<<",";vec(*v[i]);}std::cout<<"]";}
void snapshot(SAILONE&s,SAILVERTEX*v){
std::cout<<"{\"phase\":["<<s.VertIdx<<","<<s.HorzIdx<<"],\"wind_up\":"<<int(s.WindUp)<<",\"sum\":"<<bits(s.SumWind)<<",\"maximum\":"<<bits(s.MaxSumWind)<<",\"points\":[";
for(int i=0;i<20;i++){if(i)std::cout<<",";vec(s.SailPnt[i]);}
std::cout<<"],\"ropes\":[";for(int i=0;i<4;i++){if(i)std::cout<<",";vec(*s.sailtrope.pPos[i]);}
std::cout<<"],\"box_center\":";vec(s.ss.boxCenter);std::cout<<",\"box_size\":";vec(s.ss.boxSize);
std::cout<<",\"vertices\":[";int n=s.ss.eSailType==SAIL_TREANGLE?153:221;
for(int i=0;i<n;i++){if(i)std::cout<<",";std::cout<<"["<<bits(v[i].pos.x)<<","<<bits(v[i].pos.y)<<","<<bits(v[i].pos.z)<<","<<bits(v[i].norm.x)<<","<<bits(v[i].norm.y)<<","<<bits(v[i].norm.z)<<","<<bits(v[i].tu1)<<","<<bits(v[i].tv1)<<","<<bits(v[i].tu2)<<","<<bits(v[i].tv2)<<","<<bits(v[i].tu3)<<","<<bits(v[i].tv3)<<"]";}
std::cout<<"]}";
}
int main(){std::cout<<"[";for(int id=0;id<60;id++){
if(id)std::cout<<",";Parent p;SAILONE s;s.pp=&p;s.ss.eSailType=SAIL_TYPE(id%6);
float x=id*.125f,w=2.f+float(id%5),ht=1.f+float(id%4);
s.ss.hardPoints[0]=CVECTOR(x,2.f,-.25f);s.ss.hardPoints[1]=CVECTOR(x+w,2.25f,.125f);s.ss.hardPoints[2]=CVECTOR(x+.25f,2.f-ht,.5f);s.ss.hardPoints[3]=CVECTOR(x+w-.25f,2.f-ht-.125f,-.125f);
s.ss.fDeepZ=.125f*float(id%5)-.25f;s.ss.fDeepH=.03f*float(id%4);s.ss.fDeepVh=.01f*float(id%7);s.ss.fDeepVz=.02f*float(id%3);s.ss.holeCount=id%13;
s.ss.bYesLimitPoint=id%2;s.sailHeight=ht;
p.globalWind.base=std::array<float,5>{-.2f,0.f,.4f,1.f,1.5f}[id%5];
s.sailWind=CVECTOR((id%3-1)*.75f,.125f,(id%5-2)*.5f);
s.m_dwRow=std::array<uint32_t,5>{17,13,9,5,3}[(id/6)%5];s.m_dwCol=std::array<uint32_t,4>{13,10,7,4}[(id/6)%4];
s.VertIdx=id%13;s.HorzIdx=(id+7)%13;s.WindUp=id%2;s.SumWind=.01f*ht;s.MaxSumWind=.02f*ht;
CVECTOR ropes[4];for(int i=0;i<4;i++){ropes[i]=CVECTOR(float(i)+.25f,-1.f,.5f);s.sailtrope.pPos[i]=&ropes[i];s.sailtrope.pnttie[i]=((id>>i)&1)!=0;}
for(int i=0;i<20;i++)s.SailPnt[i]=CVECTOR(float(i)*.125f,-.25f,.5f);
s.SetGeometry();SAILVERTEX v[221]{};s.FillVertex(v);s.SetTexGrid(v);
if((id/6)%2){s.sroll=new SAILROLLING;s.sroll->oldgeo=s.sgeo;s.sroll->delta=.25f+float(id%3)*.25f;s.sroll->rollup=id%2;}
std::cout<<"{\"id\":"<<id<<",\"kind\":"<<id%6<<",\"hard_points\":[";for(int i=0;i<4;i++){if(i)std::cout<<",";vec(s.ss.hardPoints[i]);}
std::cout<<"],\"geometry\":";geometry(s.sgeo);
std::cout<<",\"parameters\":["<<bits(s.ss.fDeepZ)<<","<<bits(s.ss.fDeepH)<<","<<bits(s.ss.fDeepVh)<<","<<bits(s.ss.fDeepVz)<<"],\"holes\":"<<s.ss.holeCount<<",\"limit\":"<<int(s.ss.bYesLimitPoint)<<",\"height\":"<<bits(ht)<<",\"base\":"<<bits(p.globalWind.base)<<",\"wind\":";vec(s.sailWind);
std::cout<<",\"sphere\":["<<bits(s.ss.boundSphere.c.x)<<","<<bits(s.ss.boundSphere.c.y)<<","<<bits(s.ss.boundSphere.c.z)<<","<<bits(s.ss.boundSphere.r)<<"],\"lod\":["<<s.m_dwRow<<","<<s.m_dwCol<<"],\"ties\":[";for(int i=0;i<4;i++){if(i)std::cout<<",";std::cout<<int(s.sailtrope.pnttie[i]);}
std::cout<<"],\"rolling_delta\":";if(s.sroll)std::cout<<bits(s.sroll->delta);else std::cout<<"null";
std::cout<<",\"initial\":";snapshot(s,v);std::cout<<",\"frames\":[";
uint32_t times[]={0,17,1001};for(int i=0;i<3;i++){if(i)std::cout<<",";s.goWave(v,times[i]);snapshot(s,v);}
std::cout<<"]}";delete s.sroll;
}std::cout<<"]";}
'''
source=source.replace('__DECLARATIONS__',parts['vertex']+'\n'+parts['geometry']).replace('__METHODS__','\n'.join(parts[n] for n in names))
with tempfile.TemporaryDirectory(prefix='storm-sail-wave-') as folder:
 p=Path(folder);(p/'main.cpp').write_text(source)
 subprocess.run(['clang++','-std=c++20','-ffp-contract=off','-fsanitize=undefined','-fno-sanitize-recover=all','-I',str(repo/'src/libs/math/include'),str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
 cases=json.loads(subprocess.check_output([str(p/'oracle')],text=True))
assert len(cases)==60
(here/'cases.json').write_text(json.dumps(dict(scope=__doc__,retained_sha256={k:hashlib.sha256(v.encode()).hexdigest() for k,v in parts.items()},cases=cases),separators=(',',':'))+'\n')
print('60 attached-wave profiles / 180 complete frames / '+str(sum(len(s['vertices']) for c in cases for s in c['frames']))+' complete vertices under UBSan')
