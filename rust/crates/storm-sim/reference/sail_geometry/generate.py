"""Unedited SetGeometry, FillVertex, SetTexGrid and DoRollingStep under UBSan.
Full retained vector/vertex/geometry declarations; explicit state and flex ports.
No native RNG, SetSail lifecycle, oscillation, LOD or renderer execution.
"""
from pathlib import Path
import hashlib,json,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/rigging/src/sailone.cpp').is_file())
cpp=(repo/'src/libs/rigging/src/sailone.cpp').read_text();h=(repo/'src/libs/rigging/src/sailone.h').read_text()
def method(name):
 a=cpp.index('void SAILONE::'+name);return cpp[a:cpp.index('\n}',a)+2]
parts={name:method(name) for name in ['SetGeometry()','FillVertex(SAILVERTEX','SetTexGrid(SAILVERTEX','DoRollingStep(uint32_t']}
a=h.index('struct SAILGEOMETRY');parts['geometry']=h[a:h.index('struct ROTATEROPEDSAIL',a)]
a=h.index('#define SAIL_COL_MAX');parts['vertex']=h[a:h.index('// parameters for sail triangulation',a)]
source=r'''
#include <array>
#include <bit>
#include <cstring>
#include <iostream>
#include <cstdint>
#include "c_vector.h"
enum SAIL_TYPE{SAIL_TREANGLE,SAIL_SQUARE,SAIL_TRAPECIDAL,SAIL_GERBED,SAIL_ROLLING,SAIL_SPECIAL};
__DECLARATIONS__
struct SPHERE{CVECTOR c,rc;float r;};
struct Parent{float fSHoleFlexDepend;struct{float fRollingSpeed=.0003f;}gdata[1];};
struct SAILONE{
SAILGEOMETRY sgeo{};SAILROLLING*sroll=nullptr;Parent*pp;bool bRolling=false;int HostNum=0;float m_fHorzGeraldScale,m_fVertGeraldScale;
struct{SAIL_TYPE eSailType;CVECTOR hardPoints[4];SPHERE boundSphere;float fDeepZ,fDeepH,fDeepVh,fDeepVz;int holeCount;}ss{};
void SetGeometry();void FillVertex(SAILVERTEX*);void SetTexGrid(SAILVERTEX*)const;void DoRollingStep(uint32_t);
};
__METHODS__
uint32_t bits(float f){return std::bit_cast<uint32_t>(f);}
std::array<CVECTOR*,10> vectors(SAILGEOMETRY&g){return {&g.cv.dVv,&g.cv.ddVv,&g.cv.dddVv,&g.cv.dVh,&g.cv.ddVh,&g.cv.normL,&g.cv.dnormL,&g.cv.normR,&g.cv.dnormR,&g.cv.dopV};}
void vec(CVECTOR v){std::cout<<"["<<bits(v.x)<<","<<bits(v.y)<<","<<bits(v.z)<<"]";}
void geo(SAILGEOMETRY&g){std::cout<<"[";bool first=true;for(auto*v:vectors(g)){if(!first)std::cout<<",";first=false;vec(*v);}std::cout<<"]";}
void snapshot(SAILONE&s){
std::cout<<"{\"geometry\":";geo(s.sgeo);std::cout<<",\"transition\":";
if(s.sroll){std::cout<<"{\"geometry\":";geo(s.sroll->oldgeo);std::cout<<",\"delta\":"<<bits(s.sroll->delta)<<",\"up\":"<<int(s.sroll->rollup)<<"}";}else std::cout<<"null";
std::cout<<",\"rolled\":"<<int(s.bRolling)<<",\"sphere\":["<<bits(s.ss.boundSphere.c.x)<<","<<bits(s.ss.boundSphere.c.y)<<","<<bits(s.ss.boundSphere.c.z)<<","<<bits(s.ss.boundSphere.r)<<"],\"vertices\":[";
SAILVERTEX v[SAIL_COL_MAX*SAIL_ROW_MAX]{};s.FillVertex(v);s.SetTexGrid(v);int n=s.ss.eSailType==SAIL_TREANGLE?SAIL_ROW_MAX*(SAIL_ROW_MAX+1)/2:SAIL_ROW_MAX*SAIL_COL_MAX;
for(int i=0;i<n;i++){if(i)std::cout<<",";std::cout<<"["<<bits(v[i].pos.x)<<","<<bits(v[i].pos.y)<<","<<bits(v[i].pos.z)<<","<<bits(v[i].norm.x)<<","<<bits(v[i].norm.y)<<","<<bits(v[i].norm.z)<<","<<bits(v[i].tu1)<<","<<bits(v[i].tv1)<<","<<bits(v[i].tu2)<<","<<bits(v[i].tv2)<<","<<bits(v[i].tu3)<<","<<bits(v[i].tv3)<<"]";}
std::cout<<"]}";
}
int main(){std::cout<<"[";for(int id=0;id<48;id++){
if(id)std::cout<<",";Parent p;SAILONE s;s.pp=&p;s.ss.eSailType=SAIL_TYPE(id%6);
float x=id*.125f,w=2.f+float(id%5),ht=1.f+float(id%4);
s.ss.hardPoints[0]=CVECTOR(x,2.f,-.25f);s.ss.hardPoints[1]=CVECTOR(x+w,2.25f,.125f);s.ss.hardPoints[2]=CVECTOR(x+.25f,2.f-ht,.5f);s.ss.hardPoints[3]=CVECTOR(x+w-.25f,2.f-ht-.125f,-.125f);
if(id/6==7)for(auto&v:s.ss.hardPoints)v=s.ss.hardPoints[0];
s.ss.fDeepZ=.125f*float(id%5)-.25f;s.ss.fDeepH=.03f*float(id%4);s.ss.fDeepVh=.01f*float(id%7);s.ss.fDeepVz=.02f*float(id%3);s.ss.holeCount=id%13;p.fSHoleFlexDepend=std::array<float,3>{.01f,1.f/12.f,.2f}[id%3];s.m_fHorzGeraldScale=.5f+float(id%3);s.m_fVertGeraldScale=.25f+float(id%4);
int j=0;for(auto*v:vectors(s.sgeo)){*v=CVECTOR(float(j)*.125f,-float(j)*.25f,.5f);j++;}
if((id/6)%2){s.sroll=new SAILROLLING;s.sroll->oldgeo=s.sgeo;s.sroll->oldgeo.cv.dopV=CVECTOR(.25f,-.5f,.75f);s.sroll->delta=.75f;s.sroll->rollup=(id/12)%2;}
std::cout<<"{\"id\":"<<id<<",\"kind\":"<<id%6<<",\"hard_points\":[";for(int i=0;i<4;i++){if(i)std::cout<<",";vec(s.ss.hardPoints[i]);}
std::cout<<"],\"parameters\":["<<bits(s.ss.fDeepZ)<<","<<bits(s.ss.fDeepH)<<","<<bits(s.ss.fDeepVh)<<","<<bits(s.ss.fDeepVz)<<","<<bits(p.fSHoleFlexDepend)<<"],\"holes\":"<<s.ss.holeCount<<",\"gerald\":["<<bits(s.m_fHorzGeraldScale)<<","<<bits(s.m_fVertGeraldScale)<<"],\"initial_geometry\":";geo(s.sgeo);
std::cout<<",\"initial_transition\":";if(s.sroll){std::cout<<"{\"geometry\":";geo(s.sroll->oldgeo);std::cout<<",\"delta\":"<<bits(s.sroll->delta)<<",\"up\":"<<int(s.sroll->rollup)<<"}";}else std::cout<<"null";
std::cout<<",\"snapshots\":[";s.SetGeometry();snapshot(s);s.DoRollingStep(2000);std::cout<<",";snapshot(s);std::cout<<"]}";delete s.sroll;
}std::cout<<"]";}
'''
source=source.replace('__DECLARATIONS__',parts['vertex']+'\n'+parts['geometry']).replace('__METHODS__','\n'.join(parts[n] for n in ['SetGeometry()','FillVertex(SAILVERTEX','SetTexGrid(SAILVERTEX','DoRollingStep(uint32_t']))
with tempfile.TemporaryDirectory(prefix='storm-sail-geometry-') as folder:
 p=Path(folder);(p/'main.cpp').write_text(source)
 subprocess.run(['clang++','-std=c++20','-ffp-contract=off','-fsanitize=undefined','-fno-sanitize-recover=all','-I',str(repo/'src/libs/math/include'),str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
 cases=json.loads(subprocess.check_output([str(p/'oracle')],text=True))
assert len(cases)==48
(here/'cases.json').write_text(json.dumps(dict(scope=__doc__,retained_sha256={k:hashlib.sha256(v.encode()).hexdigest() for k,v in parts.items()},cases=cases),separators=(',',':'))+'\n')
print('48 native geometry profiles / 96 complete snapshots / '+str(sum(len(s['vertices']) for c in cases for s in c['snapshots']))+' complete vertices under UBSan')
