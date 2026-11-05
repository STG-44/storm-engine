"""Whole unedited SEA::AttributeChanged and animation with real ATTRIBUTES parsing.
Surface numeric fields only; color/render/texture callbacks are stubs. Codec
interning is an ASCII adapter. Checked Rust rejection cases are separate.
"""
from pathlib import Path
import hashlib,json,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/sea/src/sea.cpp').exists())
cpp=(repo/'src/libs/sea/src/sea.cpp').read_text()
a=cpp.index('uint32_t SEA::AttributeChanged(');handler=cpp[a:cpp.index('\n}',a)+2]
a=cpp.index('    float fDeltaTime = static_cast<float>(dwDeltaTime) * 0.001f;');animation=cpp[a:cpp.index('    // pSeaParameters',a)]
cases=[]
for i in range(32):
    ops=[['notify','SeA2.Amp1',f' {2+i*.25} waves'],['notify','sea2.amp2',str(1+i*.125)],
      ['notify','other.MaxSeaHeight','999'],['set','MaxSeaHeight',str([0,1.5,4,50][i%4])],['notify','MaxSeaHeight',None],
      ['notify','SEA2.Amp1',str(4+i*.5)],['notify','isDone','ignored'],['notify','isDone.refresh','1'],
      ['notify','Sea2.MoveSpeed1',[' 1.25, -2.5, +.5 trailing','-0, +0, -.0','2.75, -1e-2, 3e1tail','4 , 6, 8','5, bad, 7','nonsense',', 1, 2','1,2'][i%8]],['notify','Sea2.MoveSpeed1','2.5'],
      ['notify','Sea2.MoveSpeed2',' -.25, +.5, 1e1 rest'],['notify','Sea2.MoveSpeed2','4 , 6, 8'],
      ['notify','MaxSeaDistance',str([-20,0,1600,12.5][i%4])],['notify','Sea2.Scale1',str([-.5,0,.25,2][i%4])],
      ['notify','Sea2.AnimSpeed1',str([0,6.25,63.5,100][i%4])],['notify','Sea2.AnimSpeed2','3.125'],
      ['notify','Stop','-1'],['advance',1000],['notify','Sea2.Amp2','3.5'],['notify','MaxSeaHeight',str([0,2,10,50][i%4])],
      ['notify','isDone.refresh','2'],['set','Stop','0'],['notify','other.Stop','7'],['advance',17],
      ['notify','fog.Stop','1'],['notify','Sea2.MaxSeaHeight','77'],['notify','isDone.MaxSeaHeight','555'],
      ['notify','Sea2.MoveSpeed1','nonsense'],['notify','Sea2.MoveSpeed1','1, bad, 3'],
      ['notify','Sea2.Amp1',''],['notify','MaxSeaHeight',None],['advance',1000],['notify','isDone.refresh','3']]
    cases.append({'id':i,'operations':ops})
source=r'''
#include <memory>
#include <bit>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <iostream>
#include <string>
#include <vector>
#include "attributes.h"
#include "c_vector.h"
#define FRAMES 64
class Codec:public VSTRING_CODEC {std::vector<std::string>names;public:
 uint32_t GetNum()override{return names.size();}void VariableChanged()override{}
 uint32_t Convert(const char*s)override{return Convert(s,strlen(s));}
 uint32_t Convert(const char*s,int32_t n)override{std::string v(s,n);for(char&c:v)if(c>='A'&&c<='Z')c+=32;for(uint32_t i=0;i<names.size();i++)if(names[i]==v)return i;names.push_back(v);return names.size()-1;}
 const char*Convert(uint32_t c)override{return names.at(c).c_str();}};
struct V4{float x{},y{},z{},w{};};V4 COLOR2VECTOR4(uint32_t){return {};}CVECTOR COLOR2VECTOR(uint32_t){return CVECTOR(0.f);}
struct SEA {
 ATTRIBUTES*AttributesPointer;
 float _fAmp1=8,_fAmp2=1,fAmp1=8,fAmp2=1,fScale1=.5f,fScale2=2,fAnimSpeed1=6,fAnimSpeed2=3,fFrame1=0,fFrame2=10,fMaxSeaHeight=20,fMaxSeaDistance=1600;
 CVECTOR vMove1=CVECTOR(0.f),vMove2=CVECTOR(0.f),vMoveSpeed1=CVECTOR(5.f,0.f,0.f),vMoveSpeed2=CVECTOR(0.f,0.f,2.f);
 bool bStop=false,bUnderSeaEnable=false,bFoamEnable=false,bSimpleSea=false,bFogEnable=false;
 V4 v4SeaColor{},v4SkyColor{},v4SeaParameters{};CVECTOR vFogColor=CVECTOR(0.f);
 float fPosShift=0,fBumpScale=0,fBumpSpeed=0,fLodScale=0,fGridStep=0,fFrenel=0,fFoamV=0,fFoamK=0,fFoamUV=0,fFoamTextureDisturb=0,fFogStartDistance=0,fFogSeaDensity=0;
 uint32_t dwMaxDim=0,dwMinDim=0;int zero_requests=0,zero_applied=0;
 uint32_t AttributeChanged(ATTRIBUTES*);
 void BuildVolumeTexture(){}
 void Realize(uint32_t dwDeltaTime){if(dwDeltaTime==0)zero_requests++;if(bStop)return;if(dwDeltaTime==0)zero_applied++;__ANIMATION__}
};
__HANDLER__
uint32_t bits(float x){return std::bit_cast<uint32_t>(x);}void vec(CVECTOR v){std::cout<<"["<<bits(v.x)<<","<<bits(v.y)<<","<<bits(v.z)<<"]";}
void snap(SEA&s){std::cout<<"{\"raw\":["<<bits(s._fAmp1)<<","<<bits(s._fAmp2)<<"],\"amplitude\":["<<bits(s.fAmp1)<<","<<bits(s.fAmp2)<<"],\"scale\":["<<bits(s.fScale1)<<","<<bits(s.fScale2)<<"],\"speed\":["<<bits(s.fAnimSpeed1)<<","<<bits(s.fAnimSpeed2)<<"],\"frame\":["<<bits(s.fFrame1)<<","<<bits(s.fFrame2)<<"],\"move_speed\":[";vec(s.vMoveSpeed1);std::cout<<",";vec(s.vMoveSpeed2);std::cout<<"],\"movement\":[";vec(s.vMove1);std::cout<<",";vec(s.vMove2);std::cout<<"],\"max_height\":"<<bits(s.fMaxSeaHeight)<<",\"max_distance\":"<<bits(s.fMaxSeaDistance)<<",\"stopped\":"<<(s.bStop?"true":"false")<<",\"zero_requests\":"<<s.zero_requests<<",\"zero_applied\":"<<s.zero_applied<<"}\n";}
int main(){__CASES__}
'''
blocks=[]
for case in cases:
    lines=['{Codec codec;ATTRIBUTES root(codec);SEA s;s.AttributesPointer=&root;']
    for op in case['operations']:
        if op[0]=='advance': lines.append(f's.Realize({op[1]});')
        else:
            path=op[1]
            lines.append('{auto*p=root.CreateSubAClass(&root,'+json.dumps(path)+');')
            if op[2] is not None: lines.append('p->SetValue('+json.dumps(op[2])+');')
            if op[0]=='notify':lines.append('s.AttributeChanged(p);')
            lines.append('}')
        lines.append('snap(s);')
    lines.append('}')
    blocks.append('\n'.join(lines))
source=source.replace('__ANIMATION__',animation).replace('__HANDLER__',handler).replace('__CASES__','\n'.join(blocks))
with tempfile.TemporaryDirectory(prefix='storm-sea-weather-') as folder:
    p=Path(folder);(p/'platform').mkdir()
    (p/'storm_assert.h').write_text('#pragma once\n#include <cassert>\n#define Assert(x) assert(x)\n')
    (p/'platform/platform.hpp').write_text('#pragma once\n#include <cstdarg>\n#include <cstdio>\n#include <cstring>\n#include <cassert>\ntemplate<size_t N>int sprintf_s(char(&o)[N],const char*f,...){va_list v;va_start(v,f);int r=vsnprintf(o,N,f,v);va_end(v);return r;}\ntemplate<size_t N>int strncpy_s(char(&o)[N],const char*s,int n){assert(n>=0&&size_t(n)<N);memcpy(o,s,n);o[n]=0;return 0;}\n')
    (p/'main.cpp').write_text(source)
    subprocess.run(['clang++','-std=c++20','-w','-ffp-contract=off','-fsanitize=undefined','-fno-sanitize-recover=all','-include','memory','-I'+str(p),'-I'+str(repo/'src/libs/math/include'),'-I'+str(repo/'src/libs/core/include'),'-I'+str(repo/'src/libs/util/include'),str(p/'main.cpp'),str(repo/'src/libs/core/src/attributes.cpp'),'-o',str(p/'oracle')],check=True)
    rows=[json.loads(s) for s in subprocess.check_output([str(p/'oracle')],text=True).splitlines()]
offset=0
for case in cases:
    n=len(case['operations']);case['snapshots']=rows[offset:offset+n];offset+=n
assert offset==len(rows)==1056
(here/'cases.json').write_text(json.dumps({'scope':__doc__,'retained_sha256':{'handler':hashlib.sha256(handler.encode()).hexdigest(),'animation':hashlib.sha256(animation.encode()).hexdigest(),'attributes':hashlib.sha256((repo/'src/libs/core/src/attributes.cpp').read_bytes()).hexdigest()},'cases':cases},separators=(',',':'))+'\n')
print('32 weather sequences / 1056 complete native snapshots under UBSan')
