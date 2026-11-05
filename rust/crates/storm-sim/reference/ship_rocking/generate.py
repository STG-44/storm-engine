"""Unedited ShipRocking, RotateAroundY, Min and CVECTOR under UBSan.
Matrix construction, SHIP construction/state binding and SEA_BASE are adapters.
WaveXZ uses a prescribed analytic/alternating surface, not original SEA.
Native math observations separate equation parity from portable scalar kernels.
"""
from pathlib import Path
import hashlib, json, subprocess, tempfile

here = Path(__file__).resolve().parent
repo = next(p for p in here.parents if (p / 'src/libs/ship/src/ship.cpp').is_file())
ship = (repo / 'src/libs/ship/src/ship.cpp').read_text()
math = (repo / 'src/libs/math/include/math_inlines.h').read_text()
def method(text, signature):
    start = text.index(signature)
    return text[start:text.index('\n}', start) + 2]
retained = {'rocking': method(ship, 'CVECTOR SHIP::ShipRocking('),
            'rotate': method(math, 'inline void RotateAroundY('),
            'min': method(math, 'template <class T> T Min(')}
source = r'''
#include <array>
#include <bit>
#include <cstdint>
#include <iostream>
#include <vector>
#include "c_vector.h"
std::vector<std::array<uint32_t,3>> trig, queries;
uint32_t bits(float f){return std::bit_cast<uint32_t>(f);}
float observed_cos(float x){float v=::cosf(x);trig.push_back({1,bits(x),bits(v)});return v;}
float observed_sin(float x){float v=::sinf(x);trig.push_back({0,bits(x),bits(v)});return v;}
float observed_atan(float x){float v=atan(x);trig.push_back({2,bits(x),bits(v)});return v;}
#define cosf observed_cos
#define sinf observed_sin
#define atan observed_atan
struct CMatrix{float m[4][4]{};};
struct Sea{int kind=0;float base=0;float WaveXZ(float x,float z){float v=base+0.03125f*x-0.0625f*z+0.0005f*x*z;if(kind==1)v=base;if(kind==2)v+=queries.size()%2?-0.375f:0.375f;queries.push_back({bits(x),bits(z),bits(v)});return v;}};
struct SHIP{struct{CVECTOR vPos,vAng,vBoxSize;}State;struct point{float fY=42.0f;}ShipPoints[16][16];CVECTOR vAng;float fXOffset=0,fZOffset=0,fRockingY=1,fRockingAZ=1;Sea*pSea=nullptr;CMatrix matrix;CMatrix*GetMatrix(){return &matrix;}CVECTOR ShipRocking(float);};
__BODIES__
void vec(CVECTOR v){std::cout<<"["<<bits(v.x)<<","<<bits(v.y)<<","<<bits(v.z)<<"]";}
void rows(const std::vector<std::array<uint32_t,3>>&rows){std::cout<<"[";bool first=true;for(auto&r:rows){if(!first)std::cout<<",";first=false;std::cout<<"["<<r[0]<<","<<r[1]<<","<<r[2]<<"]";}std::cout<<"]";}
struct Case{float yaw,width,length,dt,height,angular,offsetx,offsetz;bool sea;int kind;};
int main(){std::vector<Case>cases={__CASES__};for(auto&c:cases){SHIP s;Sea sea;sea.kind=c.kind;s.pSea=c.sea?&sea:nullptr;s.State.vPos=CVECTOR(3.0f,2.0f,-5.0f);s.State.vAng=CVECTOR(0.125f,c.yaw,-0.25f);s.vAng=CVECTOR(-0.125f,c.yaw+0.0625f,0.375f);s.State.vBoxSize=CVECTOR(c.width,8.0f,c.length);s.fXOffset=c.offsetx;s.fZOffset=c.offsetz;s.fRockingY=c.height;s.fRockingAZ=c.angular;
 for(int step=0;step<5;step++){sea.base=0.25f*step;queries.clear();trig.clear();auto a=s.ShipRocking(c.dt);std::cout<<"{\"height\":"<<bits(s.State.vPos.y)<<",\"angles\":";vec(a);std::cout<<",\"heights\":[";for(int x=0;x<6;x++){if(x)std::cout<<",";std::cout<<"[";for(int z=0;z<6;z++){if(z)std::cout<<",";std::cout<<bits(s.ShipPoints[x][z].fY);}std::cout<<"]";}std::cout<<"],\"queries\":";rows(queries);std::cout<<",\"math\":";rows(trig);std::cout<<"}\n";
 for(int x=0;x<16;x++)for(int z=0;z<16;z++)if((x>=6||z>=6)&&s.ShipPoints[x][z].fY!=42.0f)return 2;
 s.State.vAng=a;s.vAng=a;s.State.vPos.x+=0.375f;s.State.vPos.z-=0.25f;
 }}}
'''
cases = []
for i in range(96):
    cases.append(dict(id=i, yaw=[0.0, 0.35, -1.2, 3.0][i % 4],
                      width=[1.0, 6.0, 18.5][i % 3], length=[3.0, 16.0, 73.25][(i//3) % 3],
                      dt=[0.0, 0.016, 0.025, 0.05, 0.1, 0.4][(i//4) % 6],
                      height=[0.0, 0.25, 1.0, 2.0][(i//6) % 4],
                      angular=[1.0, 0.5, 0.0, -0.125][(i//8) % 4],
                      offsetx=[0.0, 1000.0, -37.0][i % 3], offsetz=[0.0, -2000.0, 25.0][i % 3],
                      sea=i % 13 != 0, kind=i % 3))
source = source.replace('__BODIES__', '\n'.join(retained[k] for k in ['min','rotate','rocking']))
fields = ['yaw','width','length','dt','height','angular','offsetx','offsetz']
source = source.replace('__CASES__', ','.join('{' + ','.join([str(c[k])+'f' for k in fields] + [str(c['sea']).lower(), str(c['kind'])]) + '}' for c in cases))
with tempfile.TemporaryDirectory(prefix='storm-ship-rocking-') as folder:
    p = Path(folder)
    (p / 'main.cpp').write_text(source)
    subprocess.run(['clang++','-std=c++20','-ffp-contract=off','-fsanitize=undefined','-fno-sanitize-recover=all',
                    '-I',str(repo/'src/libs/math/include'),str(p/'main.cpp'),'-o',str(p/'oracle')], check=True)
    rows = [json.loads(s) for s in subprocess.check_output([str(p/'oracle')], text=True).splitlines()]
assert len(rows) == len(cases)*5
for i,c in enumerate(cases):
    c['snapshots'] = rows[i*5:(i+1)*5]
metadata = {'scope': __doc__, 'retained_sha256': {k:hashlib.sha256(v.encode()).hexdigest() for k,v in retained.items()},
            'vector_header_sha256': hashlib.sha256((repo/'src/libs/math/include/c_vector.h').read_bytes()).hexdigest()}
parts = [json.dumps(metadata, indent=2)[:-2] + ',\n  "cases": [']
for i,c in enumerate(cases):
    parts.append('    '+json.dumps({k:v for k,v in c.items() if k!='snapshots'},separators=(',',':'))[:-1]+',"snapshots":[')
    parts.extend('      '+json.dumps(s,separators=(',',':'))+(',' if n<4 else '') for n,s in enumerate(c['snapshots']))
    parts.append('    ]}'+(',' if i+1<len(cases) else ''))
parts.append('  ]\n}')
(here/'cases.json').write_text('\n'.join(parts)+'\n')
print(f'{len(cases)} native rocking sequences / {len(rows)} complete snapshots under UBSan')
