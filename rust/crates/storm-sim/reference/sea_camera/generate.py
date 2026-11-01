"""Unedited BuildViewMatrix, SetInversePosition and rigid Transposition helpers.
No renderer or native camera entity is constructed; inputs are explicit.
"""
from pathlib import Path
import hashlib,json,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/math/include/matrix.h').exists())
h=(repo/'src/libs/math/include/matrix.h').read_text()
def method(signature):
 a=h.index(signature);return h[a:h.index('\n}',a)+2]
parts={n:method(s) for n,s in [('identity','inline void CMatrix::SetIdentity()'),('view','inline bool CMatrix::BuildViewMatrix('),('position','inline void CMatrix::SetInversePosition('),('transpose','inline void CMatrix::Transposition()'),('transpose3','inline void CMatrix::Transposition3X3()'),('x','inline CVECTOR &CMatrix::Vx()'),('y','inline CVECTOR &CMatrix::Vy()'),('z','inline CVECTOR &CMatrix::Vz()'),('p','inline CVECTOR &CMatrix::Pos()')]}
source=r'''
#include <bit>
#include <cmath>
#include <iostream>
#include "c_vector.h"
struct CMatrix {union{float matrix[16];float m[4][4];};void SetIdentity();void SetInversePosition(float,float,float);bool BuildViewMatrix(CVECTOR,CVECTOR,CVECTOR);void Transposition();void Transposition3X3();CVECTOR&Vx()const;CVECTOR&Vy()const;CVECTOR&Vz()const;CVECTOR&Pos()const;};
__METHODS__
uint32_t bits(float f){return std::bit_cast<uint32_t>(f);}void vec(CVECTOR v){std::cout<<"["<<bits(v.x)<<","<<bits(v.y)<<","<<bits(v.z)<<"]";}void mat(CMatrix&m){std::cout<<"[";for(int i=0;i<16;i++){if(i)std::cout<<",";std::cout<<bits(m.matrix[i]);}std::cout<<"]";}
int main(){std::cout<<"[";for(int id=0;id<64;id++){
 if(id)std::cout<<",";CVECTOR from(float(id%9)*3.125f,-2.f+float(id%7),-float(id%11)*2.75f);CVECTOR to(float(id%5)*-.5f,1.25f,float(id%3)*7.f);CVECTOR up(0.f,1.f,0.f);
 if(id%8==0)to=from;if(id%8==1){to=from+CVECTOR(0.f,1.f,0.f);up=CVECTOR(0.f,0.f,0.f);}if(id%8==2)up=CVECTOR(.25f,1.f,-.125f);if(id%8==3)up=to-from;
 CMatrix m;bool valid=m.BuildViewMatrix(from,to,up);std::cout<<"{\"from\":";vec(from);std::cout<<",\"to\":";vec(to);std::cout<<",\"up\":";vec(up);std::cout<<",\"valid\":"<<(valid?"true":"false")<<",\"view\":";mat(m);
 if(id%4==0){m.matrix[0]*=1.25f;m.matrix[7]=.75f;m.matrix[11]=-.5f;m.matrix[15]=2.f;}
 std::cout<<",\"input\":";mat(m);m.Transposition();std::cout<<",\"transposed\":";mat(m);std::cout<<"}";
 }std::cout<<"]";}
'''.replace('__METHODS__','\n'.join(parts.values()))
def main():
 with tempfile.TemporaryDirectory(prefix='storm-sea-camera-') as folder:
  p=Path(folder);(p/'main.cpp').write_text(source)
  subprocess.run(['clang++','-std=c++20','-ffp-contract=off','-fsanitize=undefined','-fno-sanitize-recover=all','-I'+str(repo/'src/libs/math/include'),str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
  cases=json.loads(subprocess.check_output([str(p/'oracle')],text=True))
 (here/'cases.json').write_text(json.dumps({'scope':__doc__,'retained_sha256':{k:hashlib.sha256(v.encode()).hexdigest() for k,v in parts.items()},'cases':cases},separators=(',',':'))+'\n')
 print('64 complete native view/transpose profiles under UBSan')

if __name__ == "__main__":main()
