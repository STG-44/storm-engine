"""Unedited free sail methods, goWave, CVECTOR and MulToInvNorm under UBSan.
Random draws/RAND_MAX and matrix construction are explicit ports. Finite frames
compare every represented field; native non-finite outcomes are characterized,
not propagated into the checked renderer. No native RNG or mast service claim.
"""
from pathlib import Path
import ast,hashlib,json,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/rigging/src/sailone.cpp').is_file())
cpp=(repo/'src/libs/rigging/src/sailone.cpp').read_text();h=(repo/'src/libs/rigging/src/sailone.h').read_text();mh=(repo/'src/libs/math/include/matrix.h').read_text()
tree=ast.parse((here.parent/'sail_wave/generate.py').read_text())
source=next(ast.literal_eval(n.value) for n in tree.body if isinstance(n,ast.Assign) and any(isinstance(t,ast.Name) and t.id=='source' for t in n.targets) and isinstance(n.value,ast.Constant))
def method(name):
 a=cpp.index('void SAILONE::'+name);return cpp[a:cpp.index('\n}',a)+2]
names=['SetGeometry()','FillVertex(SAILVERTEX','SetTexGrid(SAILVERTEX','GoVWave(SAILVERTEX','GoTWave(SAILVERTEX','DoSFreeSail(SAILVERTEX','DoTFreeSail(SAILVERTEX','goWave(SAILVERTEX']
parts={n:method(n) for n in names}
a=h.index('struct SAILGEOMETRY');parts['geometry']=h[a:h.index('struct ROTATEROPEDSAIL',a)]
a=h.index('#define SAIL_COL_MAX');parts['vertex']=h[a:h.index('// parameters for sail triangulation',a)]
a=mh.index('inline void CMatrix::MulToInvNorm(');parts['MulToInvNorm']=mh[a:mh.index('\n}',a)+2]
source=source.replace('#include <cstdlib>','#include <cstdlib>\n#include <cmath>')
source=source.replace('struct Parent {','struct Parent {float FALL_SSAIL_ADD_MIN=.2f,FALL_SSAIL_ADD_RAND=.2f,FALL_TSAIL_ADD_MIN=.2f,FALL_TSAIL_ADD_RAND=.2f;')
source=source.replace('struct SAILONE {', 'struct CMatrix {float matrix[16]{};void MulToInvNorm(const CVECTOR&,CVECTOR&);};\n'+parts['MulToInvNorm']+'\nstruct SAILONE {CMatrix*pMatWorld;')
source=source.replace('bFreeSail=false','bFreeSail=true')
source=source.replace('void DoSFreeSail(SAILVERTEX*){std::abort();}void DoTFreeSail(SAILVERTEX*){std::abort();}','void DoSFreeSail(SAILVERTEX*);void DoTFreeSail(SAILVERTEX*);')
source=source.replace('__METHODS__',r'''
int supplied_max=32767;int supplied_values[16];int used=0;
int next_rand(){if(used>=16)std::abort();return supplied_values[used++];}
#undef RAND_MAX
#define RAND_MAX supplied_max
#define rand next_rand
__METHODS__
''')
source=source.replace('std::cout<<"{\\\"phase\\\":["', 'std::cout<<"{\\\"finite\\\":";bool finite=true;int nv=s.ss.eSailType==SAIL_TREANGLE?153:221;for(int i=0;i<nv;i++)finite=finite&&std::isfinite(v[i].pos.x)&&std::isfinite(v[i].pos.y)&&std::isfinite(v[i].pos.z);std::cout<<int(finite)<<",\\\"draws_used\\\":"<<used<<",\\\"phase\\\":["')
a=source.index('int main()')
source=source[:a]+r'''
int main(){std::cout<<"[";for(int id=0;id<72;id++){
if(id)std::cout<<",";Parent p;SAILONE s;CMatrix matrix;s.pMatWorld=&matrix;s.pp=&p;s.ss.eSailType=SAIL_TYPE(id%6);
float x=id*.125f,w=2.f+float(id%5),ht=1.f+float(id%4);
s.ss.hardPoints[0]=CVECTOR(x,2.f,-.25f);s.ss.hardPoints[1]=CVECTOR(x+w,2.25f,.125f);s.ss.hardPoints[2]=CVECTOR(x+.25f,2.f-ht,.5f);s.ss.hardPoints[3]=CVECTOR(x+w-.25f,2.f-ht-.125f,-.125f);
s.ss.fDeepZ=.125f;s.ss.fDeepH=.03f;s.ss.fDeepVh=.01f;s.ss.fDeepVz=.02f;s.ss.holeCount=id%13;s.ss.bYesLimitPoint=false;
s.sailHeight=ht;p.globalWind.base=std::array<float,5>{-.5f,0.f,.4f,1.f,1.5f}[id%5];s.sailWind=CVECTOR((id%3-1)*.75f,(id%2)*.125f,(id%5-2)*.5f);
if(id>=48&&id<60){s.sailWind=CVECTOR(0.f,1.f,0.f);p.globalWind.base=1.f;}
std::array<std::array<float,9>,6> matrices={{{1,0,0,0,1,0,0,0,1},{0,1,0,-1,0,0,0,0,1},{-1,0,0,0,1,0,0,0,1},{1.25f,.125f,-.25f,-.5f,.75f,.0625f,.25f,-.125f,1.5f},{0,0,0,0,0,0,0,0,0},{.75f,.5f,.25f,-.125f,1.5f,.25f,.125f,-.25f,.5f}}};
for(int i=0;i<3;i++)for(int j=0;j<3;j++)matrix.matrix[i*4+j]=matrices[(id/6)%6][i*3+j];
if(id>=30){p.FALL_SSAIL_ADD_MIN=p.FALL_TSAIL_ADD_MIN=std::array<float,3>{-.05f,.1f,.2f}[id%3];p.FALL_SSAIL_ADD_RAND=p.FALL_TSAIL_ADD_RAND=std::array<float,3>{0.f,.2f,.35f}[(id/6)%3];}
s.m_dwRow=5;s.m_dwCol=4;s.VertIdx=id%13;s.HorzIdx=(id+7)%13;s.WindUp=id%2;s.SumWind=.01f*ht;s.MaxSumWind=.02f*ht;
CVECTOR ropes[4];for(int i=0;i<4;i++){ropes[i]=CVECTOR(float(i)+.25f,-1.f,.5f);s.sailtrope.pPos[i]=&ropes[i];s.sailtrope.pnttie[i]=((id>>i)&1)!=0;}
for(int i=0;i<20;i++)s.SailPnt[i]=CVECTOR(float(i)*.125f,-.25f,.5f);
s.SetGeometry();SAILVERTEX v[221]{};s.FillVertex(v);s.SetTexGrid(v);
if(id>=60){CVECTOR direction;matrix.MulToInvNorm(!(CVECTOR(0.f,-1.f,0.f)+s.sailWind*p.globalWind.base),direction);CVECTOR dv=direction*(ht*.25f);int g=0;
for(int ix=0;ix<(id%6==0?5:4);ix++){CVECTOR point=s.ss.hardPoints[0]+(s.ss.hardPoints[1]-s.ss.hardPoints[0])*(id%6==0?float(ix)*.25f:float(ix)/3.f);
for(int iy=0;iy<(id%6==0?ix+1:5);iy++){s.SailPnt[g++]=point;point+=dv;}}
if(id>=66){p.FALL_SSAIL_ADD_MIN=p.FALL_TSAIL_ADD_MIN=0.f;p.FALL_SSAIL_ADD_RAND=p.FALL_TSAIL_ADD_RAND=0.f;}}
supplied_max=id%2?2147483647:32767;int count=id%6==0?10:16;used=0;
std::cout<<"{\"id\":"<<id<<",\"kind\":"<<id%6<<",\"hard_points\":[";for(int i=0;i<4;i++){if(i)std::cout<<",";vec(s.ss.hardPoints[i]);}
std::cout<<"],\"parameters\":["<<bits(ht)<<","<<bits(p.globalWind.base)<<","<<bits(p.FALL_SSAIL_ADD_MIN)<<","<<bits(p.FALL_SSAIL_ADD_RAND)<<"],\"wind\":";vec(s.sailWind);
std::cout<<",\"axes\":[";for(int i=0;i<3;i++){if(i)std::cout<<",";vec(CVECTOR(matrix.matrix[i*4],matrix.matrix[i*4+1],matrix.matrix[i*4+2]));}
std::cout<<"],\"rand_max\":"<<supplied_max<<",\"initial\":";snapshot(s,v);std::cout<<",\"draws\":[";
int draws[3][16];for(int frame=0;frame<3;frame++){if(frame)std::cout<<",";std::cout<<"[";for(int i=0;i<count;i++){if(i)std::cout<<",";uint32_t samples[5]={0u,uint32_t(supplied_max),uint32_t(supplied_max/2),uint32_t(supplied_max-1),uint32_t((uint64_t(id+11)*1234567)%supplied_max)};draws[frame][i]=int(samples[(frame+i+id)%5]);std::cout<<draws[frame][i];}std::cout<<"]";}
std::cout<<"],\"frames\":[";for(int frame=0;frame<3;frame++){if(frame)std::cout<<",";for(int i=0;i<count;i++)supplied_values[i]=draws[frame][i];used=0;s.goWave(v,std::array<uint32_t,3>{0,17,1001}[frame]);snapshot(s,v);if(used!=count)std::abort();}
std::cout<<"]}";
}std::cout<<"]";}
'''
source=source.replace('__DECLARATIONS__',parts['vertex']+'\n'+parts['geometry']).replace('__METHODS__','\n'.join(parts[n] for n in names))
with tempfile.TemporaryDirectory(prefix='storm-sail-free-') as folder:
 p=Path(folder);(p/'main.cpp').write_text(source)
 subprocess.run(['clang++','-std=c++20','-ffp-contract=off','-fsanitize=undefined','-fno-sanitize-recover=all','-I',str(repo/'src/libs/math/include'),str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
 cases=json.loads(subprocess.check_output([str(p/'oracle')],text=True))
assert len(cases)==72
finite=sum(s['finite'] for c in cases for s in c['frames']);nonfinite=216-finite
(here/'cases.json').write_text(json.dumps(dict(scope=__doc__,retained_sha256={k:hashlib.sha256(v.encode()).hexdigest() for k,v in parts.items()},cases=cases),separators=(',',':'))+'\n')
print(f'72 free profiles / {finite} finite frames / {nonfinite} non-finite native outcomes / '+str(sum(len(s['vertices']) for c in cases for s in c['frames'] if s['finite']))+' complete finite vertices under UBSan')
