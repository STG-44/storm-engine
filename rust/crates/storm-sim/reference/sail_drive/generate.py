"""Unedited CalculateSailWind, GetSailSpeed, MulToInvNorm, capacity and speed
message case under UBSan. Matrix storage, group construction, transport and
SetRolling are adapters; rolling calls are observed, not executed. The native
group normalization assignments are executed without the surrounding renderer.
"""
from pathlib import Path
import hashlib,json,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/rigging/src/sail.cpp').is_file())
sail=(repo/'src/libs/rigging/src/sail.cpp').read_text();one=(repo/'src/libs/rigging/src/sailone.cpp').read_text()
def method(text,name):
 a=text.index(name);return text[a:text.index('\n}',a)+2]
wind=method(one,'void SAILONE::CalculateSailWind()')
speed=method(sail,'float GetSailSpeed(')
matrix=method((repo/'src/libs/math/include/matrix.h').read_text(),'inline void CMatrix::MulToInvNorm(')
a=one.index('    // calculation of sail width and height');capacity=one[a:one.index('    if (sailWidth <',a)]
a=sail.index('    case MSG_SAIL_GET_SPEED:');query=sail[a:sail.index('    case MSG_SAIL_GET_PARAMS:',a)]
a=sail.index('            if (gdata[i].speed_m < 0.001f)');norm=sail[a:sail.index('                if (gdata[i].maxHole',a)]+'            }'
h=(repo/'src/libs/rigging/src/sail.h').read_text();a=h.index('    struct GROUPDATA');group=h[a:h.index('    GROUPDATA *gdata',a)]
source=r'''
#include <array>
#include <bit>
#include <iostream>
#include <vector>
#include <cstdint>
#include <cmath>
#include "c_vector.h"
#include "shared/sail_msg.h"
using entid_t=uint64_t;
enum SAIL_TYPE{SAIL_TREANGLE,SAIL_SQUARE,SAIL_TRAPECIDAL,SAIL_GERBED,SAIL_ROLLING,SAIL_SPECIAL};
struct CMatrix{float matrix[16]{};void MulToInvNorm(const CVECTOR&,CVECTOR&);};
__MATRIX__
float dependence=1;
__SPEED__
float GetSailSpeed(int q,int m,float v){return GetSailSpeed(q,m,v,dependence);}
struct Message{uint64_t ship;float*output;float request;uint64_t EntityID(){return ship;}void*Pointer(){return output;}float Float(){return request;}};
struct SAILONE;
struct SAIL{
__GROUP__
GROUPDATA data{},*gdata=&data;int groupQuantity=1;SAILONE**slist=nullptr;
struct{CVECTOR ang;float base=0.4f;}globalWind;
float ts_min=.2f,ts_xdep=.6f,ts_zdep=.8f,fs_min=.4f,fs_xdep=.5f,fs_zdep=.6f,ss_min=.4f,ss_xdep=.1f,ss_zdep=.6f,m_fMinSpeedVal=0;
void Query(Message&message);void Normalize(){int i=0;__NORM__}
};
std::vector<std::array<int,2>>calls;
struct SAILONE{
SAIL*pp;int HostNum=0,rollType=1,index=0,holeMax=12;float maxSpeed=0,curSpeed=0,sailWidth=0,sailHeight=0;
struct{int holeCount=0;SAIL_TYPE eSailType=SAIL_SQUARE;CVECTOR hardPoints[4];}ss;
CMatrix matrix,*pMatWorld=&matrix;CVECTOR sailWind;bool bRolling=false;struct Roll{float delta;};Roll roll{},*sroll=nullptr;
int GetMaxHoleCount(){return holeMax;}void SetRolling(bool up){calls.push_back({index,int(up)});}
void Capacity(){__CAPACITY__}void CalculateSailWind();
};
__WIND__
void SAIL::Query(Message&message){int i=0;uint64_t tmpEI=0;switch(MSG_SAIL_GET_SPEED){__QUERY__}}
uint32_t bits(float f){return std::bit_cast<uint32_t>(f);}void vec(CVECTOR v){std::cout<<"["<<bits(v.x)<<","<<bits(v.y)<<","<<bits(v.z)<<"]";}
void snap(SAIL&p,float output){auto&g=p.data;std::cout<<"{\"group\":["<<bits(g.shipSpeed)<<","<<bits(g.maxSpeed)<<","<<bits(g.speed_m)<<","<<bits(g.fSpeedMul)<<","<<g.curSailSet<<"],\"output\":"<<bits(output)<<",\"calls\":[";bool first=true;for(auto c:calls){if(!first)std::cout<<",";first=false;std::cout<<"["<<c[0]<<","<<c[1]<<"]";}std::cout<<"]}\n";calls.clear();}
int main(){for(int id=0;id<96;id++){
SAIL p;std::array<SAILONE,3>ss;std::array<SAILONE*,3>ptrs;int indices[3]={0,1,2};p.slist=ptrs.data();auto&g=p.data;g.shipEI=100;g.sailQuantity=3;g.sailIdx=indices;g.fSpeedMul=.75f;g.curSailSet=id%3;g.bDeleted=id%11==0;
p.globalWind.ang=CVECTOR(float(id%5)*.5f-1.f,float(id%3)*.25f-.25f,float(id%7)*.5f-1.5f);dependence=float(id%4)*.5f;p.m_fMinSpeedVal=float(id%3)*.125f;
for(int j=0;j<3;j++){auto&s=ss[j];s.pp=&p;s.index=j;ptrs[j]=&s;s.ss.eSailType=SAIL_TYPE((id+j)%6);s.ss.hardPoints[0]=CVECTOR(0.f,0.f,0.f);s.ss.hardPoints[1]=CVECTOR(2.f+float(id)*.125f,0.f,.25f);s.ss.hardPoints[2]=CVECTOR(2.5f,3.f+float(j),.5f);s.ss.hardPoints[3]=CVECTOR(0.f,4.f,0.f);s.Capacity();s.ss.holeCount=(id+j)%16;s.holeMax=s.ss.eSailType==SAIL_TREANGLE?10:12;s.bRolling=(id+j)%7==0;if((id+j)%4==0){s.roll.delta=float(id%5)*.25f;s.sroll=&s.roll;}s.rollType=1+j%2;
float axes[9]={1.f,float(j)*.125f,0.f,0.f,1.f,.25f,float(id%3)*.125f,0.f,1.f};for(int k=0;k<3;k++)for(int n=0;n<3;n++)s.matrix.matrix[k*4+n]=axes[k*3+n];
g.speed_m+=s.maxSpeed;s.CalculateSailWind();g.speed_c+=s.curSpeed;std::cout<<"{\"capacity\":"<<bits(s.maxSpeed)<<",\"wind\":";vec(s.sailWind);std::cout<<",\"current\":"<<bits(s.curSpeed)<<",\"maximum\":"<<bits(GetSailSpeed(s.ss.holeCount,s.holeMax,s.maxSpeed))<<"}\n";}
if(id%13==0)g.speed_m=.0005f;
p.Normalize();snap(p,7.5f);
float requests[]={0.f,.3f,std::nextafter(.3f,1.f),.7f,std::nextafter(.7f,1.f),1.f,-.5f,1.5f};
for(int step=0;step<3;step++){float output=7.5f;Message m{uint64_t(id%9==0?999:100),id%10==0?nullptr:&output,step==0?requests[id%8]:float(step-1)};p.Query(m);snap(p,output);}
}}
'''
parts=dict(MATRIX=matrix,SPEED=speed,GROUP=group,NORM=norm,CAPACITY=capacity,WIND=wind,QUERY=query)
for k,v in parts.items():source=source.replace('__'+k+'__',v)
with tempfile.TemporaryDirectory(prefix='storm-sail-drive-') as folder:
 p=Path(folder);(p/'main.cpp').write_text(source)
 subprocess.run(['clang++','-std=c++20','-ffp-contract=off','-fsanitize=undefined','-fno-sanitize-recover=all','-I',str(repo/'src/libs/math/include'),'-I',str(repo/'src/libs/shared_headers/include'),str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
 rows=[json.loads(x) for x in subprocess.check_output([str(p/'oracle')],text=True).splitlines()]
assert len(rows)==96*7
cases=[dict(id=i,sails=rows[i*7:i*7+3],snapshots=rows[i*7+3:i*7+7]) for i in range(96)]
(here/'cases.json').write_text(json.dumps(dict(scope=__doc__,retained_sha256={k:hashlib.sha256(v.encode()).hexdigest() for k,v in parts.items()},cases=cases),indent=2)+'\n')
print('96 native sail profiles: 288 complete sail outputs and 384 group/query snapshots under UBSan')
