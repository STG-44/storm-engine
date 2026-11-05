"""Unedited rolled GoWave/DoSRollSail/DoTRollSail and FillIndex under UBSan.
Uses the prior oracle's C++ scaffolding as data, never executes that generator.
GetGrid selection slices accept projected distance; matrices/perspective, RNG,
weather, timing instrumentation and the free processors are explicit ports.
"""
from pathlib import Path
import ast,hashlib,json,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/rigging/src/sailone.cpp').is_file())
cpp=(repo/'src/libs/rigging/src/sailone.cpp').read_text();h=(repo/'src/libs/rigging/src/sailone.h').read_text()
tree=ast.parse((here.parent/'sail_wave/generate.py').read_text())
source=next(ast.literal_eval(n.value) for n in tree.body if isinstance(n,ast.Assign) and any(isinstance(t,ast.Name) and t.id=='source' for t in n.targets) and isinstance(n.value,ast.Constant))
def method(name):
 a=cpp.index('void SAILONE::'+name);return cpp[a:cpp.index('\n}',a)+2]
names=['SetGeometry()','FillVertex(SAILVERTEX','SetTexGrid(SAILVERTEX','GoVWave(SAILVERTEX','GoTWave(SAILVERTEX','DoSRollSail(SAILVERTEX','DoTRollSail(SAILVERTEX','goWave(SAILVERTEX','FillIndex(uint16_t']
parts={n:method(n) for n in names}
a=h.index('struct SAILGEOMETRY');parts['geometry']=h[a:h.index('struct ROTATEROPEDSAIL',a)]
a=h.index('#define SAIL_COL_MAX');parts['vertex']=h[a:h.index('// parameters for sail triangulation',a)]
a=h.index('#define SAIL_FAR_IDX');parts['farGrid']=h[a:h.index('struct WIND',a)]
a=cpp.index('bool SAILONE::GetGrid(');get_grid=cpp[a:cpp.index('\n}',a)+2]
selection=get_grid[get_grid.index('    if (bRolling)'):get_grid.index('        pos = *pMatWorld')]+get_grid[get_grid.index('        for (auto i = 0;'):-1]
parts['GetGrid selection']=selection
source=source.replace('#include <cstdlib>','#include <cstdlib>\n#include <algorithm>\n#include <cmath>\n#define RDTSC_B(v) v=0\n#define RDTSC_E(v) v=0')
source=source.replace('struct Parent {','struct Parent {\nfloat ROLL_Z_VAL=.01f,ROLL_Y_VAL=.04f,ROLL_Z_DELTA=.001f,TR_FORM_MUL=2.f;\nfloat SSailRollForm[13]={.2f,.8f,1.f,.8f,.4f,1.f,1.3f,1.f,.4f,.8f,1.f,.8f,.2f};\nfloat TSailRollForm[5]={.1f,.6f,.3f,.8f,.2f};')
source=source.replace('bool bRolling=false','struct {uint64_t idx;}tm;\nbool bRolling=true')
source=source.replace('bool bYesLimitPoint;','bool bYesLimitPoint,turningSail,hole[12];uint32_t sIndx,nIndx,sholeIndx,nholeIndx,nnormIndx;')
source=source.replace('void DoSRollSail(SAILVERTEX*){std::abort();}void DoTRollSail(SAILVERTEX*){std::abort();}','void DoSRollSail(SAILVERTEX*);void DoTRollSail(SAILVERTEX*);void FillIndex(uint16_t*);')
a=source.index('uint32_t bits(float')
source=source[:a]+parts['farGrid']+'\nstruct Selector{bool bRolling=false,bFreeSail=false;uint32_t m_dwRow=0,m_dwCol=0;bool select(float distance){uint32_t dwRow,dwCol;\n'+selection+'\n}};\n'+source[a:]
a=source.index('void snapshot(')
source=source[:a]+r'''
void topology(SAILONE&s){
uint16_t indices[1152];std::fill(std::begin(indices),std::end(indices),uint16_t(65535));s.FillIndex(indices);
std::cout<<"{\"normal\":"<<s.ss.nnormIndx*3<<",\"hole_start\":"<<s.ss.sholeIndx<<",\"holes\":"<<s.ss.nholeIndx*3<<",\"indices\":[";
for(uint32_t i=0;i<s.ss.nIndx;i++){if(i)std::cout<<",";std::cout<<indices[i];}std::cout<<"]}";
}
'''+source[a:]
source=source.replace('std::cout<<"]}";\n}\nint main()', 'std::cout<<"],\\\"topology\\\":";topology(s);std::cout<<"}";\n}\nint main()')
# Keep the well-audited initializer/output fields and specialize the rolled inputs.
source=source.replace('std::cout<<"[";for(int id=0;id<60;id++)','std::cout<<"{\\\"rolls\\\":[";for(int id=0;id<60;id++)')
source=source.replace('s.m_dwRow=std::array<uint32_t,5>{17,13,9,5,3}[(id/6)%5];s.m_dwCol=std::array<uint32_t,4>{13,10,7,4}[(id/6)%4];',
'''s.m_dwRow=5;s.m_dwCol=13;s.ss.nIndx=s.ss.eSailType==SAIL_TREANGLE?768:1152;s.ss.sIndx=0;
s.ss.turningSail=(id/6)%2;for(int i=0;i<12;i++)s.ss.hole[i]=((id*73>>i)&1)!=0;
if(id>=30){p.ROLL_Z_VAL=.02f;p.ROLL_Y_VAL=.06f;p.ROLL_Z_DELTA=.125f;p.TR_FORM_MUL=.75f;p.WINDVECTOR_TINCR=31;p.WINDVECTOR_SINCR=61;for(auto&f:p.SSailRollForm)f*=.5f;for(auto&f:p.TSailRollForm)f*=1.5f;}
''')
source=source.replace('if((id/6)%2){s.sroll=new SAILROLLING;s.sroll->oldgeo=s.sgeo;s.sroll->delta=.25f+float(id%3)*.25f;s.sroll->rollup=id%2;}','')
source=source.replace('std::cout<<",\\\"initial\\\":";snapshot(s,v);',r'''std::cout<<",\"turning\":"<<int(s.ss.turningSail)<<",\"mask\":"<<(id*73&4095)<<",\"increment\":"<<(id%6==0?p.WINDVECTOR_TINCR:p.WINDVECTOR_SINCR)<<",\"roll_parameters\":["<<bits(p.ROLL_Z_VAL)<<","<<bits(p.ROLL_Y_VAL)<<","<<bits(p.ROLL_Z_DELTA)<<","<<bits(p.TR_FORM_MUL)<<"],\"square_form\":[";for(int i=0;i<13;i++){if(i)std::cout<<",";std::cout<<bits(p.SSailRollForm[i]);}std::cout<<"],\"triangle_form\":[";for(int i=0;i<5;i++){if(i)std::cout<<",";std::cout<<bits(p.TSailRollForm[i]);}std::cout<<"],\"initial\":";snapshot(s,v);''')
ending=r'''
}std::cout<<"],\"topologies\":[";bool first=true;
for(int kind=0;kind<6;kind++)for(uint32_t rows:{17,13,9,5,3})for(uint32_t columns:{13,10,7,4})for(uint32_t mask:{0,1,341,2730,4095}){
if(kind!=0&&rows==3&&columns!=4)continue;
if(!first)std::cout<<",";first=false;SAILONE s;s.ss.eSailType=SAIL_TYPE(kind);s.bRolling=false;s.m_dwRow=rows;s.m_dwCol=columns;s.ss.nIndx=kind==0?768:1152;s.ss.sIndx=0;
for(int i=0;i<12;i++)s.ss.hole[i]=((mask>>i)&1)!=0;
std::cout<<"{\"kind\":"<<kind<<",\"grid\":["<<rows<<","<<columns<<"],\"mask\":"<<mask<<",\"topology\":";topology(s);std::cout<<"}";
}
std::cout<<"],\"selection\":[";first=true;
for(int flags=0;flags<4;flags++){Selector g;g.bRolling=flags&1;g.bFreeSail=flags&2;
for(float threshold:{0.f,10.f,20.f,30.f,50.f,200.f,250.f,600.f,10000.f})for(int d=-1;d<=1;d++){
float distance=d==0?threshold:std::nextafterf(threshold,d<0?0.f:INFINITY);
if(!first)std::cout<<",";first=false;bool changed=g.select(distance);
std::cout<<"{\"flags\":"<<flags<<",\"distance\":"<<bits(distance)<<",\"changed\":"<<int(changed)<<",\"grid\":["<<g.m_dwRow<<","<<g.m_dwCol<<"]}";
}}
std::cout<<"]}";}
'''
assert source.endswith('}std::cout<<"]";}\n');source=source[:-len('}std::cout<<"]";}\n')]+ending
source=source.replace('__DECLARATIONS__',parts['vertex']+'\n'+parts['geometry']).replace('__METHODS__','\n'.join(parts[n] for n in names))
with tempfile.TemporaryDirectory(prefix='storm-sail-roll-') as folder:
 p=Path(folder);(p/'main.cpp').write_text(source)
 subprocess.run(['clang++','-std=c++20','-ffp-contract=off','-fsanitize=undefined','-fno-sanitize-recover=all','-I',str(repo/'src/libs/math/include'),str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
 data=json.loads(subprocess.check_output([str(p/'oracle')],text=True))
assert len(data['rolls'])==60 and len(data['topologies'])==525 and len(data['selection'])==108
(here/'cases.json').write_text(json.dumps(dict(scope=__doc__,retained_sha256={k:hashlib.sha256(v.encode()).hexdigest() for k,v in parts.items()},**data),separators=(',',':'))+'\n')
print('60 rolled profiles / 180 complete frames / 37740 complete vertices / 525 index profiles / 108 grid selections under UBSan')
