"""Unedited complete GetGrid, farGrid and CMatrix point transform under UBSan.
Matrices/camera inputs are supplied explicitly; native matrix/camera entities
and their scheduling are outside this numeric reference.
"""
from pathlib import Path
import hashlib,json,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/rigging/src/sailone.cpp').is_file())
cpp=(repo/'src/libs/rigging/src/sailone.cpp').read_text();h=(repo/'src/libs/rigging/src/sailone.h').read_text();mh=(repo/'src/libs/math/include/matrix.h').read_text()
def method(text,name):
 a=text.index(name);return text[a:text.index('\n}',a)+2]
parts={'GetGrid':method(cpp,'bool SAILONE::GetGrid('),'point':method(mh,'inline CVECTOR CMatrix::operator*(const CVECTOR')}
a=h.index('#define SAIL_FAR_IDX');parts['farGrid']=h[a:h.index('struct WIND',a)]
source=r'''
#include <bit>
#include <cmath>
#include <array>
#include <cstdint>
#include <iostream>
#include "c_vector.h"
__TABLE__
struct CMatrix{float matrix[16]{1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1};CVECTOR operator*(const CVECTOR&);};
__POINT__
struct SAILONE{bool bRolling=false,bFreeSail=false;uint32_t m_dwRow=0,m_dwCol=0;CMatrix*pMatWorld;struct{struct{CVECTOR c,rc;}boundSphere;}ss;bool GetGrid(CVECTOR&,float);};
__METHOD__
uint32_t bits(float v){return std::bit_cast<uint32_t>(v);}
void vec(CVECTOR p){std::cout<<"["<<bits(p.x)<<","<<bits(p.y)<<","<<bits(p.z)<<"]";}
int main(){std::cout<<"[";for(int id=0;id<96;id++){
if(id)std::cout<<",";CMatrix m;int mode=(id/4)%6;auto&a=m.matrix;
if(mode==1){a[0]=0;a[2]=-1;a[8]=1;a[10]=0;}if(mode==2){a[0]=-1;}if(mode==3){a[0]=1.25f;a[1]=.125f;a[4]=-.25f;a[5]=.75f;}if(mode==4){a[0]=a[5]=a[10]=0;}if(mode==5){a[3]=.5f;a[7]=-.25f;a[11]=2.f;}
a[12]=float(id%7)*.25f;a[13]=-float(id%5)*.125f;a[14]=float(id%3)*.5f;
SAILONE s;s.pMatWorld=&m;s.ss.boundSphere.c=CVECTOR(float(id%4)-1.f,2.f,-.25f);s.ss.boundSphere.rc=CVECTOR(7.f,-8.f,9.f);
std::cout<<"{\"id\":"<<id<<",\"center\":";vec(s.ss.boundSphere.c);std::cout<<",\"matrix\":[";for(int i=0;i<16;i++){if(i)std::cout<<",";std::cout<<bits(a[i]);}std::cout<<"],\"frames\":[";
for(int k=0;k<16;k++){if(k)std::cout<<",";int flags=(id+k/4)%4;s.bRolling=flags&1;s.bFreeSail=flags&2;
float fov=std::array<float,6>{.7853981852531433f,.2f,1.5f,0.f,-.5f,2.8f}[(id/16+k)%6];
float range=std::array<float,8>{0.f,10.f,20.f,50.f,200.f,250.f,600.f,2000.f}[(id+k)%8];
CVECTOR world=m*s.ss.boundSphere.c;CVECTOR eye=world+CVECTOR(range,float(k%3)*.125f,0.f);bool changed=s.GetGrid(eye,fov);
CVECTOR delta=world-eye;float distance=sqrtf(delta.x*delta.x+delta.y*delta.y+delta.z*delta.z)*tanf(fov*.5f);
std::cout<<"{\"flags\":"<<flags<<",\"eye\":";vec(eye);std::cout<<",\"fov\":"<<bits(fov)<<",\"changed\":"<<int(changed)<<",\"grid\":["<<s.m_dwRow<<","<<s.m_dwCol<<"],\"world_center\":";vec(s.ss.boundSphere.rc);std::cout<<",\"distance\":";if(flags)std::cout<<"null";else std::cout<<bits(distance);std::cout<<"}";
}std::cout<<"]}";}std::cout<<"]";}
'''
source=source.replace('__TABLE__',parts['farGrid']).replace('__POINT__',parts['point']).replace('__METHOD__',parts['GetGrid'])
with tempfile.TemporaryDirectory(prefix='storm-sail-lod-') as folder:
 p=Path(folder);(p/'main.cpp').write_text(source)
 subprocess.run(['clang++','-std=c++20','-ffp-contract=off','-fsanitize=undefined','-fno-sanitize-recover=all','-I',str(repo/'src/libs/math/include'),str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
 cases=json.loads(subprocess.check_output([str(p/'oracle')],text=True))
assert len(cases)==96 and sum(len(c['frames']) for c in cases)==1536
(here/'cases.json').write_text(json.dumps(dict(scope=__doc__,retained_sha256={k:hashlib.sha256(v.encode()).hexdigest() for k,v in parts.items()},cases=cases),separators=(',',':'))+'\n')
print('96 complete GetGrid profiles / 1536 snapshots under UBSan')
