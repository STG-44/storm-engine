"""Generate golden cases by compiling the retained original C++ methods.
Run from any directory with Python 3 and a C++17 compiler (CXX defaults to c++).
No game data is read. The Rust implementation is never called by this script.
"""
from pathlib import Path
import hashlib,json,os,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/rigging/src/sailone.cpp').is_file())
source=(repo/'src/libs/rigging/src/sailone.cpp').read_text()
names=['FillIndex','SetTexGrid','Trace','SSailTrace','TSailTrace','CheckSailSquar']
methods=[]
for name in names:
 start=source.index(('void' if name in ['FillIndex','SetTexGrid'] else 'float')+' SAILONE::'+name+'(')
 a=source.index('{',start);depth=1;b=a+1
 while depth:
  if source[b]=='{':depth+=1
  if source[b]=='}':depth-=1
  b+=1
 methods.append(source[start:b])
speed_source=(repo/'src/libs/rigging/src/sail.cpp').read_text()
start=speed_source.index('float GetSailSpeed(');a=speed_source.index('{',start);b=speed_source.index('\n}',a)+2
methods.append(speed_source[start:b])
prefix=r'''
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <iostream>
#include <iomanip>
#include <vector>
#define SAIL_ROW_MAX 17
#define SAIL_COL_MAX 13
#define SAIL_TREANGLE 1
#define RDTSC_B(x) x=0
#define RDTSC_E(x) x=0
#include "c_vector.h"
struct Matrix {void MulToInv(const CVECTOR&a,CVECTOR&b){b=a;}};
struct Attribute{int GetAttributeAsDword(const char*){return 0;}};
struct VAI_OBJBASE{Attribute*GetACharacter(){return nullptr;}};
struct Core {void* GetEntityPointer(int){return nullptr;} template<class...T>void Event(T...){}}core;
int g_iBallOwnerIdx=0;
struct Node {const char*GetName(){return "test";}};
struct Parent {struct G{bool bDeleted=false,bYesShip=false;int shipEI=0;float speed_m=1;}gdata[1];};
struct SAILVERTEX {CVECTOR pos,norm;float tu1,tv1,tu2,tv2,tu3,tv3;};
struct SAILONE {
 struct {unsigned sIndx=0,nIndx=0,sholeIndx=0,nholeIndx=0,nnormIndx=0;int eSailType=0,holeCount=0;bool hole[12]{};CVECTOR boxCenter{},boxSize{};}ss;
 unsigned m_dwCol=13,m_dwRow=17;bool bRolling=false;float m_fHorzGeraldScale=1,m_fVertGeraldScale=1;
 struct {uint64_t idx=0;}tm;
 Matrix matrix,*pMatWorld=&matrix;Parent parent,*pp=&parent;int HostNum=0,groupNum=0;float maxSpeed=1;Node node,*hostNode=&node;CVECTOR SailPnt[20]{};
 void SetGeometry(){} void CalculateMirrorSailIndex(){} uint32_t GetHoleDword(){uint32_t n=0;for(int i=0;i<12;i++)if(ss.hole[i])n|=1u<<i;return n;}
 void FillIndex(uint16_t*);void SetTexGrid(SAILVERTEX*)const;float Trace(const CVECTOR&,const CVECTOR&,bool);float SSailTrace(CVECTOR&,CVECTOR&,bool);float TSailTrace(CVECTOR&,CVECTOR&,bool);float CheckSailSquar(int,CVECTOR&,CVECTOR&,CVECTOR&,CVECTOR&,CVECTOR&,bool);
};
'''
suffix=r'''
uint64_t hash(const std::vector<uint16_t>&v){uint64_t h=14695981039346656037ull;for(auto x:v)for(int shift:{0,8}){h^=(x>>shift)&255;h*=1099511628211ull;}return h;}
void vec(CVECTOR v){std::cout<<'['<<v.x<<','<<v.y<<','<<v.z<<']';}
void setup(SAILONE&s,int tri){
 s.ss.eSailType=tri;s.m_dwCol=tri?17:13;s.ss.nIndx=tri?768:1152;
 int i=0;
 if(tri){for(int r=0;r<5;r++)for(int c=0;c<=r;c++){float u=(r-c)*.25f,v=c*.25f;s.SailPnt[i++]={4*u,6*v,.25f*u*v};}}
 else{for(int x=0;x<4;x++)for(int y=0;y<5;y++){float u=x/3.f,v=y*.25f;s.SailPnt[i++]={4*u,6*v,.25f*u*v};}}
 CVECTOR min=s.SailPnt[0],max=min;
 for(int j=1;j<i;j++){auto p=s.SailPnt[j];min={fminf(min.x,p.x),fminf(min.y,p.y),fminf(min.z,p.z)};max={fmaxf(max.x,p.x),fmaxf(max.y,p.y),fmaxf(max.z,p.z)};}
 s.ss.boxCenter=(min+max)*.5f;s.ss.boxSize=max-s.ss.boxCenter;
}
int main(){std::cout<<std::setprecision(9)<<"[";bool comma=false;
 for(int tri=0;tri<2;tri++){
  SAILONE s;setup(s,tri);unsigned limit=1u<<(tri?10:12);
  for(unsigned mask=0;mask<limit;mask++){
   for(int i=0;i<12;i++)s.ss.hole[i]=(mask>>i)&1;
   std::vector<uint16_t>indices(s.ss.nIndx);s.FillIndex(indices.data());
   if(comma)std::cout<<',';comma=true;
   std::cout<<"{\"type\":\"indices\",\"triangle\":"<<tri<<",\"mask\":"<<mask<<",\"normal_count\":"<<s.ss.nnormIndx*3<<",\"hash\":\""<<std::hex<<hash(indices)<<std::dec<<"\"}";
  }
  for(int y=-1;y<=13;y++)for(int x=-1;x<=13;x++){
   CVECTOR a={4*x/12.f,6*y/12.f,-2},b={a.x,a.y,2};
   for(int mode=0;mode<6;mode++){
    unsigned mask=mode==2?limit-1:0;for(int i=0;i<12;i++)s.ss.hole[i]=(mask>>i)&1;s.ss.holeCount=mode==2?(tri?10:12):0;
    auto from=a,to=b;if(mode==1){from.z=.25f*(from.x/4)*(from.y/6);}
    if(mode==3){from.x-=1.25f;to.x+=1.25f;from.y+=.75f;to.y-=.75f;}
    if(mode==4){from={a.x,8.f,a.y*.04f};to={a.x,-2.f,a.y*.04f};}
    if(mode==5){from={a.x,a.y,-.5f};to={a.x+2.f,a.y+.5f,1.5f};}
    float t=s.Trace(from,to,true);
    std::cout<<",{\"type\":\"trace\",\"triangle\":"<<tri<<",\"mask\":"<<mask<<",\"from\":";vec(from);std::cout<<",\"to\":";vec(to);std::cout<<",\"fraction\":"<<t<<",\"result_mask\":"<<s.GetHoleDword()<<"}";
   }
  }
  int count=tri?153:221;std::vector<SAILVERTEX> v(count);s.SetTexGrid(v.data());
  std::cout<<",{\"type\":\"uv\",\"triangle\":"<<tri<<",\"vertices\":[";for(int i=0;i<count;i++){if(i)std::cout<<',';std::cout<<'['<<v[i].tu1<<','<<v[i].tv1<<','<<v[i].tu3<<','<<v[i].tv3<<']';}std::cout<<"]}";
 }
 for(int maximum:{-1,0,10,12})for(int holes:{-1,0,1,3,10,12,15})for(float dependence:{0.f,.5f,1.f,1.5f}) {
  std::cout<<",{\"type\":\"speed\",\"triangle\":0,\"maximum\":"<<maximum<<",\"holes\":"<<holes<<",\"dependence\":"<<dependence<<",\"result\":"<<GetSailSpeed(holes,maximum,37.625f,dependence)<<"}";
 }
 std::cout<<"]";
}
'''
with tempfile.TemporaryDirectory() as d:
 d=Path(d);cpp=d/'reference.cpp';exe=d/'reference';cpp.write_text(prefix+'\n'.join(methods)+suffix)
 subprocess.run([os.environ.get('CXX','c++'),'-std=c++17','-O0','-ffp-contract=off','-I'+str(repo/'src/libs/math/include'),str(cpp),'-o',str(exe)],check=True)
 data=json.loads(subprocess.check_output([str(exe)],text=True))
output={'source':'src/libs/rigging/src/sailone.cpp','source_sha256':hashlib.sha256(source.encode()).hexdigest(),'methods':names+['GetSailSpeed'],'speed_source_sha256':hashlib.sha256(speed_source.encode()).hexdigest(),'cases':data}
(here/'cases.json').write_text(json.dumps(output,separators=(',',':'))+'\n');print('Original C++ cases:',len(data))
