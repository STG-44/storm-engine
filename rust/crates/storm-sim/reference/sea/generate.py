"""Unedited scalar WaveXZ, map interpolation, normal construction and animation.
Synthetic byte frames only. No game assets, graphics services or weather scripts.
Native math uses its default SSE rounding mode. Run with clang++ and UBSan.
"""
from pathlib import Path
import hashlib, json, subprocess, tempfile
here = Path(__file__).resolve().parent
repo = next(p for p in here.parents if (p/'src/libs/sea/src/sea.cpp').is_file())
cpp = (repo/'src/libs/sea/src/sea.cpp').read_text()
def method(name):
    a = cpp.index(name)
    return cpp[a:cpp.index('\n}', a)+2]
parts = {name: method(prefix) for name, prefix in [
    ('sample', 'float SEA::WaveXZ('), ('height', 'void SEA::CalculateHeightMap('),
    ('normal', 'void SEA::CalculateNormalMap(')]}
a = cpp.index('#define GET_MASSIVE')
parts['build_normal'] = cpp[a:cpp.index('aVectors[i][x + y * XWIDTH] = vRes1;', a)]
a = cpp.index('    float fDeltaTime = static_cast<float>(dwDeltaTime) * 0.001f;')
parts['animation'] = cpp[a:cpp.index('    // pSeaParameters', a)]
source = r'''
#include <bit>
#include <cmath>
#include <cstdint>
#include <iostream>
#include <vector>
#include "c_vector.h"
#include "math3d.h"
#define FRAMES 64
#define XWIDTH 128
#define YWIDTH 128
#define Sqr(x) ((x)*(x))
#define MAKELONG(a,b) (uint32_t(uint16_t(a)) | (uint32_t(uint16_t(b)) << 16))
struct SEA {
 CVECTOR vCamPos, vMove1, vMove2, vMoveSpeed1, vMoveSpeed2;
 float fMaxSeaDistance=1600, fAmp1=8, fAmp2=1, fScale1=.5f, fScale2=2;
 float fFrame1=0, fFrame2=10, fAnimSpeed1=6, fAnimSpeed2=3;
 float *pSeaFrame1, *pSeaFrame2, *pSeaNormalsFrame1, *pSeaNormalsFrame2;
 std::vector<uint8_t*> aBumps; std::vector<uint32_t*> aNormals;
 bool bSimpleSea=false;
 float WaveXZ(float,float,CVECTOR*);
 void CalculateHeightMap(float,float,float*,std::vector<uint8_t*>&);
 void CalculateNormalMap(float,float,float*,std::vector<uint32_t*>&);
 void animate(uint32_t dwDeltaTime) { __ANIMATION__ }
 void build() {
  for(uint32_t i=0;i<FRAMES;i++) for(uint32_t y=0;y<YWIDTH;y++) for(uint32_t x=0;x<XWIDTH;x++) {
  __BUILD_NORMAL__
  }
 }
};
__METHODS__
uint32_t bits(float f){return std::bit_cast<uint32_t>(f);}
template<class T> uint64_t hash(const T *v,int n){uint64_t h=14695981039346656037ull;for(int i=0;i<n;i++){uint32_t w;if constexpr(sizeof(T)==1)w=v[i];else if constexpr(std::is_same_v<T,float>)w=bits(v[i]);else w=v[i];for(int b=0;b<sizeof(T);b++){h^=(w>>(8*b))&255;h*=1099511628211ull;}}return h;}
void vec(CVECTOR v){std::cout<<"["<<bits(v.x)<<","<<bits(v.y)<<","<<bits(v.z)<<"]";}
int main(){
 SEA s;std::vector<std::vector<uint8_t>> heights(FRAMES,std::vector<uint8_t>(16384));std::vector<std::vector<uint32_t>> normals(FRAMES,std::vector<uint32_t>(16384));
 for(int f=0;f<FRAMES;f++){for(int i=0;i<16384;i++){int x=i%128,z=i/128;heights[f][i]=(x*3+z*5+f*7+(x*z+f)%19)%256;}s.aBumps.push_back(heights[f].data());s.aNormals.push_back(normals[f].data());}s.build();
 std::vector<float> maps(6*16384);s.pSeaFrame1=maps.data();s.pSeaNormalsFrame1=maps.data()+16384;s.pSeaFrame2=maps.data()+3*16384;s.pSeaNormalsFrame2=maps.data()+4*16384;
 std::cout<<"{\"normal_hashes\":[";for(int f=0;f<64;f++){if(f)std::cout<<",";std::cout<<"\""<<hash(normals[f].data(),16384)<<"\"";}std::cout<<"],\"cases\":[";
 for(int id=0;id<4;id++){
 if(id)std::cout<<",";s.fAmp1=id==1?0.f:8.f;s.fAmp2=id==1?0.f:(id==2?-1.25f:1.f);s.fScale1=id==2?-.25f:.5f;s.fScale2=id==3?0.f:2.f;s.fFrame1=0;s.fFrame2=10;s.fAnimSpeed1=id==3?63.5f:6.f;s.fAnimSpeed2=id==3?53.5f:3.f;s.vMove1=s.vMove2=CVECTOR(0.f);s.vMoveSpeed1=CVECTOR(5.f,0,0);s.vMoveSpeed2=CVECTOR(0,0,2.f);s.vCamPos=CVECTOR(7.f,123.f,-9.f);
 std::cout<<"{\"amplitude\":["<<bits(s.fAmp1)<<","<<bits(s.fAmp2)<<"],\"scale\":["<<bits(s.fScale1)<<","<<bits(s.fScale2)<<"],\"animation_speed\":["<<bits(s.fAnimSpeed1)<<","<<bits(s.fAnimSpeed2)<<"],\"frames\":[";
 for(int k=0;k<16;k++){
 if(k)std::cout<<",";uint32_t delta=k==0?0:(k==1?1:(k==2?16:(k==3?17:1000)));s.animate(delta);
 s.CalculateHeightMap(s.fFrame1,1.f/255.f,s.pSeaFrame1,s.aBumps);s.CalculateNormalMap(s.fFrame1,1.f/255.f,s.pSeaNormalsFrame1,s.aNormals);
 s.CalculateHeightMap(s.fFrame2,1.f/255.f,s.pSeaFrame2,s.aBumps);s.CalculateNormalMap(s.fFrame2,1.f/255.f,s.pSeaNormalsFrame2,s.aNormals);
 std::cout<<"{\"delta_ms\":"<<delta<<",\"animation\":["<<bits(s.fFrame1)<<","<<bits(s.fFrame2)<<","<<bits(s.vMove1.x)<<","<<bits(s.vMove1.z)<<","<<bits(s.vMove2.x)<<","<<bits(s.vMove2.z)<<"],\"maps_hash\":\""<<hash(maps.data(),6*16384)<<"\",\"queries\":[";
 for(int q=0;q<96;q++){
 if(q)std::cout<<",";float x=float(q-48)*3.125f,z=-float(q-48)*1.375f;
 if(q<8){x=float(q)-4.f;z=float(q)*.5f;}if(q>=88){x=s.vCamPos.x+std::vector<float>{1599.9999f,1600.f,1600.0001f,-1600.f,-1600.0001f,0.f,0.f,0.f}[q-88];z=s.vCamPos.z+(q==95?1600.0001f:0.f);}
 CVECTOR n;float h=s.WaveXZ(x,z,&n),h0=s.WaveXZ(x,z,nullptr);std::cout<<"["<<bits(x)<<","<<bits(z)<<","<<bits(h)<<","<<bits(h0)<<",";vec(n);std::cout<<"]";
 }std::cout<<"]}";
 }std::cout<<"]}";
 }std::cout<<"]}";
}
'''
source = source.replace('__ANIMATION__', parts['animation']).replace('__BUILD_NORMAL__', parts['build_normal']).replace('__METHODS__', '\n'.join(parts[k] for k in ['sample','height','normal']))
with tempfile.TemporaryDirectory(prefix='storm-sea-') as folder:
    p = Path(folder)
    (p/'main.cpp').write_text(source)
    subprocess.run(['clang++','-std=c++20','-ffp-contract=off','-fsanitize=undefined','-fno-sanitize-recover=all','-I',str(repo/'src/libs/math/include'),str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
    result = json.loads(subprocess.check_output([str(p/'oracle')],text=True))
result['scope'] = __doc__
result['retained_sha256'] = {k: hashlib.sha256(v.encode()).hexdigest() for k,v in parts.items()}
(here/'cases.json').write_text(json.dumps(result,separators=(',',':'))+'\n')
print('64 normal maps / 64 prepared surfaces / 6144 height+normal queries under UBSan')
