"""Unedited SetTurnLimits, TurnSail, SetGeometry, ROPE::GetEndPoint and the
MSG_SAIL_ROPE_TIE numeric loop under UBSan. Node matrix accumulation and entity
registration are observed ports; endpoint matrices are explicit inputs.
"""
from pathlib import Path
import hashlib,json,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/rigging/src/sailone.cpp').is_file())
cpp=(repo/'src/libs/rigging/src/sailone.cpp').read_text();h=(repo/'src/libs/rigging/src/sailone.h').read_text();matrix=(repo/'src/libs/math/include/matrix.h').read_text();sail=(repo/'src/libs/rigging/src/sail.cpp').read_text();rope=(repo/'src/libs/rigging/src/rope.cpp').read_text()
def method(text,name):
 a=text.index(name);return text[a:text.index('\n}',a)+2]
parts={n:method(cpp,'void SAILONE::'+n) for n in ['SetTurnLimits()','TurnSail(float','SetGeometry()','DoRollingStep(uint32_t']}
parts['GetEndPoint']=method(rope,'void ROPE::GetEndPoint(')
parts['matrix_point']=method(matrix,'inline CVECTOR CMatrix::operator*(const CVECTOR')
parts['MulToInv']=method(matrix,'inline void CMatrix::MulToInv(')
a=h.index('struct SAILGEOMETRY');parts['geometry']=h[a:h.index('enum SAIL_TYPE',a)]
a=sail.index('        for (i = 0; i < sailQuantity; i++)',sail.index('case MSG_SAIL_ROPE_TIE:'))
parts['tie_loop']=sail[a:sail.index('\n        break;',a)]
source=r'''
#include <array>
#include <bit>
#include <cmath>
#include <iostream>
#include <vector>
#include <cstdint>
#include "c_vector.h"
using entid_t=uint64_t;
enum SAIL_TYPE{SAIL_TREANGLE,SAIL_SQUARE,SAIL_TRAPECIDAL,SAIL_GERBED,SAIL_ROLLING,SAIL_SPECIAL};
constexpr int SAIL_ROW_MAX=17,SAIL_COL_MAX=13;
__GEOMETRY__
uint32_t bits(float f){return std::bit_cast<uint32_t>(f);}
std::vector<std::vector<uint32_t>>calls;std::vector<int>queries;int globals=0,rebuilds=0;
struct CMatrix{float matrix[16]{1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1};CVECTOR p;CVECTOR operator*(const CVECTOR&);void MulToInv(const CVECTOR&,CVECTOR&);CVECTOR Pos(){calls.push_back({0});return p;}void SetPosition(float x,float y,float z){p=CVECTOR(x,y,z);calls.push_back({1,bits(x),bits(y),bits(z)});}void SetPosition(CVECTOR v){SetPosition(v.x,v.y,v.z);}void RotateY(float a){calls.push_back({2,bits(a)});}void EqMultiply(const CMatrix&,const CMatrix&){calls.push_back({3});globals++;}};
__MATRIX__
namespace GEOS{struct INFO{CVECTOR boxsize;};}
struct GeoPort{float width=0;void GetInfo(GEOS::INFO&inf){inf.boxsize=CVECTOR(width,0.f,0.f);}};
struct NODE{CMatrix loc_mtx,glob_mtx;NODE*parent=nullptr;GeoPort*geo;};
struct ROPE_BASE{virtual void GetEndPoint(CVECTOR*,int,entid_t)=0;};
struct Group{entid_t modelEI=77;float fRollingSpeed=.0003f;};
struct RopeData{int ropeNum=0,HostGroup=0;CVECTOR pBeg,pEnd;CMatrix*bMatWorld,*eMatWorld;};
struct ROPE:ROPE_BASE{int ropeQuantity=3;RopeData*rlist[3];Group gdata[2];void NativePoint(CVECTOR*,int,entid_t);void GetEndPoint(CVECTOR*v,int n,entid_t m)override{queries.push_back(n);NativePoint(v,n,m);}} registry;
#define GetEndPoint NativePoint
__ENDPOINT__
#undef GetEndPoint
struct Core{bool present=true;entid_t GetEntityId(const char*){return present?1:0;}void*GetEntityPointer(entid_t){return static_cast<ROPE_BASE*>(&registry);}}core;
struct Parent{float TURNSTEPANGL=.002f,WINDANGL_DISCRETE=.01f,MAXTURNANGL=.6f,TsailWindDepend=.5f,fSHoleFlexDepend=.01f;Group gdata[1];};
struct SPHERE{CVECTOR c,rc;float r;};
struct SAILONE{SAILGEOMETRY sgeo{};SAILROLLING*sroll=nullptr;bool bRolling=false;float oldWindAngl=0,m_fMinAngle=.6f,m_fMaxAngle=.6f;Parent*pp;int HostNum=0,groupNum=1;NODE*hostNode;CVECTOR sailWind;SAILTIEROPE sailtrope{};struct{SAIL_TYPE eSailType;CVECTOR hardPoints[4];bool turningSail=true;SPHERE boundSphere;float fDeepZ=.125f,fDeepH=.03f,fDeepVh=.01f,fDeepVz=.02f;int holeCount=0;}ss;
void SetTurnLimits();void TurnSail(float);void DoRollingStep(uint32_t);void NativeGeometry();void SetGeometry(){rebuilds++;NativeGeometry();}};
#define SetGeometry NativeGeometry
__GEOMETRY_METHOD__
#undef SetGeometry
__METHODS__
void tie(SAILONE&s,CVECTOR*pos,int ropenum){int i,iMax,posNum;float posDelta,curDelta;entid_t tmpEI;NODE*nod=s.hostNode;int gNum=1,sailQuantity=1;SAILONE*slist[1]{&s};auto*gdata=s.pp->gdata;
__TIE_LOOP__
}
std::array<CVECTOR*,10>vectors(SAILGEOMETRY&g){return {&g.cv.dVv,&g.cv.ddVv,&g.cv.dddVv,&g.cv.dVh,&g.cv.ddVh,&g.cv.normL,&g.cv.dnormL,&g.cv.normR,&g.cv.dnormR,&g.cv.dopV};}
void vec(CVECTOR v){std::cout<<"["<<bits(v.x)<<","<<bits(v.y)<<","<<bits(v.z)<<"]";}
void geo(SAILGEOMETRY&g){std::cout<<"[";bool first=true;for(auto*v:vectors(g)){if(!first)std::cout<<",";first=false;vec(*v);}std::cout<<"]";}
void snapshot(SAILONE&s){bool finite=true;for(auto v:s.ss.hardPoints)finite=finite&&std::isfinite(v.x)&&std::isfinite(v.y)&&std::isfinite(v.z);
std::cout<<"{\"finite\":"<<int(finite)<<",\"geometry\":";geo(s.sgeo);std::cout<<",\"hard_points\":[";for(int i=0;i<4;i++){if(i)std::cout<<",";vec(s.ss.hardPoints[i]);}
std::cout<<"],\"rolling\":"<<int(s.bRolling)<<",\"angle\":"<<bits(s.oldWindAngl)<<",\"limits\":["<<bits(s.m_fMinAngle)<<","<<bits(s.m_fMaxAngle)<<"],\"transition\":";if(s.sroll){std::cout<<"{\"geometry\":";geo(s.sroll->oldgeo);std::cout<<",\"delta\":"<<bits(s.sroll->delta)<<",\"up\":"<<int(s.sroll->rollup)<<"}";}else std::cout<<"null";
std::cout<<",\"sphere\":[";vec(s.ss.boundSphere.c);std::cout<<","<<bits(s.ss.boundSphere.r)<<"],\"position\":";vec(s.hostNode->loc_mtx.p);
std::cout<<",\"globals\":"<<globals<<",\"rebuilds\":"<<rebuilds<<",\"ties\":[";for(int i=0;i<4;i++){if(i)std::cout<<",";if(s.sailtrope.pnttie[i])vec(*s.sailtrope.pPos[i]);else std::cout<<"null";}
std::cout<<"],\"records\":[";for(int i=0;i<2;i++){if(i)std::cout<<",";auto*r=s.sailtrope.rrs[i];if(!r){std::cout<<"null";continue;}std::cout<<"{\"number\":"<<r->ropenum<<",\"tie\":"<<r->tiePoint<<",\"r1\":"<<bits(r->r1)<<",\"r2\":"<<bits(r->r2)<<",\"b\":";if(s.ss.eSailType==SAIL_TREANGLE)std::cout<<"null";else vec(r->b);std::cout<<"}";}
std::cout<<"],\"queries\":[";for(size_t i=0;i<queries.size();i++){if(i)std::cout<<",";std::cout<<queries[i];}std::cout<<"],\"calls\":[";for(size_t i=0;i<calls.size();i++){if(i)std::cout<<",";std::cout<<"[";for(size_t j=0;j<calls[i].size();j++){if(j)std::cout<<",";std::cout<<calls[i][j];}std::cout<<"]";}std::cout<<"]}";calls.clear();queries.clear();}
void matrices(CMatrix*m){std::cout<<"[";for(int i=0;i<6;i++){if(i)std::cout<<",";std::cout<<"[";for(int j=0;j<16;j++){if(j)std::cout<<",";std::cout<<bits(m[i].matrix[j]);}std::cout<<"]";}std::cout<<"]";}
int main(){std::cout<<"{\"cases\":[";for(int id=0;id<78;id++){
if(id)std::cout<<",";Parent p;NODE parent,node;GeoPort geometry;parent.geo=&geometry;node.parent=&parent;node.loc_mtx.p=CVECTOR(3.f+id,-2.f,.25f);
SAILONE s;s.pp=&p;s.hostNode=&node;s.ss.eSailType=SAIL_TYPE(id%6);s.bRolling=id<72&&id%4==0;s.ss.turningSail=id%5!=0;core.present=id%7!=0;
float x=2.f+float(id%3)*.25f;s.ss.hardPoints[0]=CVECTOR(id%2?-x:x,2.f,.3f);s.ss.hardPoints[1]=CVECTOR(-1.f,2.25f,.125f);s.ss.hardPoints[2]=CVECTOR(.25f,-1.f,.5f);s.ss.hardPoints[3]=CVECTOR(2.f,-1.125f,-.125f);
s.oldWindAngl=(id%3-1)*.125f;s.m_fMinAngle=.6f+float(id%4)*.125f;s.m_fMaxAngle=.6f-float(id%3)*.2f;
geometry.width=std::array<float,4>{-.25f,0.f,.125f,.5f}[id%4];p.TsailWindDepend=std::array<float,3>{0.f,.05f,.5f}[id%3];p.MAXTURNANGL=std::array<float,4>{.6f,.8f,0.f,-.1f}[(id/6)%4];
int j=0;for(auto*v:vectors(s.sgeo)){*v=CVECTOR(float(j)*.125f,-float(j)*.25f,.5f);j++;}
if(id<72&&(id/6)%2){s.sroll=new SAILROLLING;s.sroll->oldgeo=s.sgeo;s.sroll->delta=.75f;s.sroll->rollup=id%2;}
globals=0;rebuilds=0;calls.clear();queries.clear();s.SetGeometry();
CMatrix m[6];RopeData data[3];registry.gdata[0].modelEI=77;registry.gdata[1].modelEI=88;
for(int k=0;k<3;k++){data[k].ropeNum=k==1?2:1;data[k].HostGroup=k==2?1:0;data[k].pBeg=CVECTOR(float(k)*.25f,-.5f,.125f);data[k].pEnd=CVECTOR(3.f+float(k),.25f,1.f-float(k)*.125f);data[k].bMatWorld=&m[k*2];data[k].eMatWorld=&m[k*2+1];registry.rlist[k]=&data[k];
for(int end=0;end<2;end++){auto&a=m[k*2+end].matrix;int mode=(id/6)%4;if(mode==1){a[0]=0;a[2]=-1;a[8]=1;a[10]=0;}if(mode==2){a[0]=1.25f;a[1]=.125f;a[4]=-.25f;a[5]=.75f;}if(mode==3){a[0]=-1;}a[12]=float(end)*.5f;a[13]=float(k)*.125f;a[14]=float(end)*-.25f;}}
std::cout<<"{\"id\":"<<id<<",\"kind\":"<<id%6<<",\"turning\":"<<int(s.ss.turningSail)<<",\"service\":"<<int(core.present)<<",\"limit_parameters\":["<<bits(geometry.width)<<","<<bits(p.TsailWindDepend)<<","<<bits(p.MAXTURNANGL)<<"],\"matrices\":";matrices(m);
std::cout<<",\"endpoint_checks\":[";for(int k=0;k<7;k++){if(k)std::cout<<",";int n=std::array<int,7>{1,-1,2,-2,1,999,1}[k];entid_t model=k==4?88:(k==6?999:77);CVECTOR out(1.f,-2.f,3.f);registry.NativePoint(&out,n,model);vec(out);}
std::cout<<"],\"snapshots\":[";snapshot(s);
CVECTOR tiepoints[4]{s.ss.hardPoints[0],s.ss.hardPoints[2],s.ss.hardPoints[s.ss.eSailType==SAIL_TREANGLE?1:3],s.ss.hardPoints[2]};
for(int k=0;k<4;k++){std::cout<<",";tie(s,&tiepoints[k],k==2?-2:1);snapshot(s);}
std::cout<<",";s.SetTurnLimits();snapshot(s);
if(id>=72){core.present=true;s.bRolling=false;for(int k=0;k<2;k++)if(s.sailtrope.rrs[k]){delete s.sailtrope.rrs[k];s.sailtrope.rrs[k]=nullptr;}s.sailtrope.pnttie[2]=true;s.sailtrope.pPos[2]=&tiepoints[1];s.sailtrope.rrs[0]=new ROTATEROPEDSAIL{};s.sailtrope.rrs[0]->tiePoint=2;s.sailtrope.rrs[0]->ropenum=1;}
std::cout<<"],\"steps\":[";for(int k=0;k<(id<72?6:1);k++){if(k)std::cout<<",";m[1].matrix[12]+=.0625f;float wind=std::array<float,6>{0.f,1.f,-1.f,.5f,0.f,-.5f}[k];float step=std::array<float,3>{0.f,.034f,.1f}[(k+id)%3];if(id>=72){wind=-1.f;step=.1f;}
std::cout<<"{\"wind\":"<<bits(wind)<<",\"step\":"<<bits(step)<<",\"matrices\":";matrices(m);s.sailWind.x=wind;s.TurnSail(step);std::cout<<",\"state\":";snapshot(s);std::cout<<"}";}
std::cout<<"]}";delete s.sroll;for(auto*r:s.sailtrope.rrs)delete r;
}std::cout<<"]}";}
'''
source=source.replace('__GEOMETRY__',parts['geometry']).replace('__MATRIX__',parts['matrix_point']+'\n'+parts['MulToInv']).replace('__ENDPOINT__',parts['GetEndPoint']).replace('__GEOMETRY_METHOD__',parts['SetGeometry()']).replace('__METHODS__','\n'.join(parts[n] for n in ['SetTurnLimits()','TurnSail(float','DoRollingStep(uint32_t'])).replace('__TIE_LOOP__',parts['tie_loop'])
with tempfile.TemporaryDirectory(prefix='storm-sail-rope-') as folder:
 p=Path(folder);(p/'main.cpp').write_text(source)
 subprocess.run(['clang++','-std=c++20','-Wno-parentheses','-Wno-dangling-else','-ffp-contract=off','-fsanitize=undefined','-fno-sanitize-recover=all','-I',str(repo/'src/libs/math/include'),str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
 result=json.loads(subprocess.check_output([str(p/'oracle')],text=True))
assert len(result['cases'])==78
result.update(scope=__doc__,retained_sha256={k:hashlib.sha256(v.encode()).hexdigest() for k,v in parts.items()})
(here/'cases.json').write_text(json.dumps(result,separators=(',',':'))+'\n')
print('78 profiles / 546 endpoint checks / '+str(sum(len(c['snapshots'])+len(c['steps']) for c in result['cases']))+' complete represented snapshots under UBSan')
