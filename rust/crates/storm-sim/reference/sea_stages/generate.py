"""Native Core Execute/Realize loops and SEA ProcessStage around retained numeric
Realize segments. Script, layer membership, renderer and query actors are explicit
adapters; this does not execute full SHIP or original water rendering.
"""
from pathlib import Path
import importlib.util,hashlib,json,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/sea/src/sea.cpp').exists())
spec=importlib.util.spec_from_file_location('sea_camera_reference',here.parent/'sea_camera/generate.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
cpp=(repo/'src/libs/sea/src/sea.cpp').read_text();core=(repo/'src/libs/core/src/core_impl.cpp').read_text();header=(repo/'src/libs/sea/src/sea.h').read_text()
def method(text,start):
 a=text.index(start);return text[a:text.index('\n}',a)+2]
parts={'execute':method(core,'void CoreImpl::ProcessExecute()'),'realize_loop':method(core,'void CoreImpl::ProcessRealize()')}
a=header.index('    void ProcessStage(Stage stage, uint32_t delta) override');parts['sea_dispatch']=header[a:header.index('\n    }',a)+6]
a=cpp.index('    float fDeltaTime = static_cast<float>(dwDeltaTime) * 0.001f;');parts['animation']=cpp[a:cpp.index('    // pSeaParameters',a)]
a=cpp.index('    CMatrix mView, mIView;',cpp.index('void SEA::Realize('));parts['camera']=cpp[a:cpp.index('    bUnderSea = false;',a)]
source=module.source.split('int main(){',1)[0]+r'''
#include <vector>
#define FRAMES 64
#define D3DTS_VIEW 0
#define SECTION_EXECUTE 0
#define SECTION_REALIZE 1
struct Entity {enum class Stage {execute,realize,lost_render,restore_render};virtual void ProcessStage(Stage,uint32_t)=0;virtual~Entity()=default;};
enum class layer_type_t {execute,realize};
struct CoreService {std::vector<int>exec{0,1},draw{2};Entity*entities[3];bool first=true;const auto&GetEntityIds(layer_type_t t){return t==layer_type_t::execute?exec:draw;}Entity*GetEntityPointerSafe(int id){return id==0&&!first?nullptr:entities[id];}}core;
struct CoreImpl {struct{uint32_t delta=0;uint32_t GetDeltaTime(){return delta;}}Timer;void ProcessRunStart(int){}void ProcessRunEnd(int){}void ProcessExecute();void ProcessRealize();};
__CORE__
struct Renderer {CMatrix view;void GetTransform(int,CMatrix&out){out=view;}};
struct SEA:Entity {
 Renderer*rs;bool bStop=false;float fFrame1=0,fFrame2=10,fAnimSpeed1=6,fAnimSpeed2=3;
 CVECTOR vMove1=CVECTOR(0.f),vMove2=CVECTOR(0.f),vMoveSpeed1=CVECTOR(5.f,0.f,0.f),vMoveSpeed2=CVECTOR(0.f,0.f,2.f),vCamPos=CVECTOR(0.f),vWorldOffset=CVECTOR(0.f);
 void Realize(uint32_t dwDeltaTime){if(bStop)return;__ANIMATION__ __CAMERA__}
 void LostRender(){}void RestoreRender(){}
 __DISPATCH__
};
struct Query:Entity {int id;SEA*sea;std::vector<std::string>*rows;void ProcessStage(Stage s,uint32_t)override{if(s!=Stage::execute)return;std::ostringstream out;out<<"["<<id<<","<<bits(sea->fFrame1)<<","<<bits(sea->fFrame2)<<","<<bits(sea->vCamPos.x)<<","<<bits(sea->vCamPos.y)<<","<<bits(sea->vCamPos.z)<<"]";rows->push_back(out.str());}};
int main(){std::cout<<"[";for(int id=0;id<32;id++){
 if(id)std::cout<<",";Renderer renderer;renderer.view.SetIdentity();SEA sea;sea.rs=&renderer;sea.Realize(0);CoreImpl engine;std::vector<std::string>queries;Query a,b;a.id=0;b.id=1;a.sea=b.sea=&sea;a.rows=b.rows=&queries;core.entities[0]=&a;core.entities[1]=&b;core.entities[2]=&sea;core.first=true;
 std::cout<<"{\"id\":"<<id<<",\"frames\":[";
 for(int step=0;step<16;step++){
 if(step)std::cout<<",";CVECTOR eye(float(id%3)*17.5f+float(step)*.125f,10.f+float(id%5),-20.f+float(step)*.25f);renderer.view.BuildViewMatrix(eye,CVECTOR(0.f),CVECTOR(0.f,1.f,0.f));
 if(step==4)sea.bStop=true;if(step==7)sea.bStop=false;if(step==5||step==7)sea.Realize(0);if(step==10)core.first=false;
 engine.Timer.delta=step<3?std::vector<uint32_t>{0,16,17}[step]:1000;queries.clear();engine.ProcessExecute();engine.ProcessRealize();
 std::cout<<"{\"step\":"<<step<<",\"delta\":"<<engine.Timer.delta<<",\"stop\":"<<(sea.bStop?"true":"false")<<",\"view\":";mat(renderer.view);std::cout<<",\"queries\":[";for(size_t i=0;i<queries.size();i++){if(i)std::cout<<",";std::cout<<queries[i];}
 std::cout<<"],\"frame\":["<<bits(sea.fFrame1)<<","<<bits(sea.fFrame2)<<"],\"movement\":[";vec(sea.vMove1);std::cout<<",";vec(sea.vMove2);std::cout<<"],\"eye\":";vec(sea.vCamPos);std::cout<<",\"offset\":";vec(sea.vWorldOffset);std::cout<<"}";
 }std::cout<<"]}";
 }std::cout<<"]";}
'''
source='#include <sstream>\n'+source.replace('__CORE__',parts['execute']+'\n'+parts['realize_loop']).replace('__ANIMATION__',parts['animation']).replace('__CAMERA__',parts['camera']).replace('__DISPATCH__',parts['sea_dispatch'])
with tempfile.TemporaryDirectory(prefix='storm-sea-stages-') as folder:
 p=Path(folder);(p/'main.cpp').write_text(source)
 subprocess.run(['clang++','-std=c++20','-ffp-contract=off','-fsanitize=undefined','-fno-sanitize-recover=all','-I'+str(repo/'src/libs/math/include'),str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
 cases=json.loads(subprocess.check_output([str(p/'oracle')],text=True))
(here/'cases.json').write_text(json.dumps({'scope':__doc__,'retained_sha256':{k:hashlib.sha256(v.encode()).hexdigest() for k,v in parts.items()},'cases':cases},separators=(',',':'))+'\n')
print('32 native stage sequences / 512 query-time and realized camera snapshots under UBSan')
