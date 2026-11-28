"""Unedited SHIP dispatch, Init priority block and Mount layer registrations,
with the original Core stage loops and EntityManager/cache/Entity/VMA. Script
variables, service access and Execute/Realize bodies are probes, not full SHIP.
"""
from pathlib import Path
import hashlib,json,shutil,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/ship/src/ship.cpp').exists())
ship=(repo/'src/libs/ship/src/ship.cpp').read_text();header=(repo/'src/libs/ship/src/ship.h').read_text();core=(repo/'src/libs/core/src/core_impl.cpp').read_text()
def method(text,start):
 a=text.index(start);return text[a:text.index('\n}',a)+2]
a=header.index('    void ProcessStage(Stage stage, uint32_t delta) override');dispatch=header[a:header.index('\n    }',a)+6]
a=ship.index('    const auto priorityExecutePtr');priorities=ship[a:ship.index('    return true;',a)]
a=ship.index('    core.AddToLayer(RealizeLayer, GetId(), iShipPriorityRealize);');mount=ship[a:ship.index('\n\n',a)]
parts={'dispatch':dispatch,'priorities':priorities,'mount_layers':mount,'core_execute':method(core,'void CoreImpl::ProcessExecute()'),'core_realize':method(core,'void CoreImpl::ProcessRealize()')}
files=['src/entity_manager.cpp','src/entity_manager.h','src/entity_container_cache.cpp','src/entity_container_cache.h','include/entity.h','include/vma.hpp']
driver=r'''
#include "entity_manager.h"
#include "vma.hpp"
#include <iostream>
#include <string>
#include <optional>
#include <array>
#define SECTION_EXECUTE 0
#define SECTION_REALIZE 1
struct VDATA{int32_t v;int32_t GetInt(){return v;}};
EntityManager manager;
std::optional<VDATA> ex,rz;
struct Core {
 const auto&GetEntityIds(layer_type_t type){return manager.GetEntityIds(type);}
 Entity*GetEntityPointerSafe(entid_t id){return manager.IsEntityValid(id)?manager.GetEntityPointer(id):nullptr;}
 void*GetScriptVariable(const char*name){auto&v=std::string(name)=="iShipPriorityExecute"?ex:rz;return v?&*v:nullptr;}
 void AddToLayer(layer_index_t l,entid_t id,priority_t p){manager.AddToLayer(l,id,p);}
}core;
struct CoreImpl{struct{uint32_t delta;uint32_t GetDeltaTime(){return delta;}}Timer;void ProcessRunStart(int){}void ProcessRunEnd(int){}void ProcessExecute();void ProcessRealize();};
__CORE__
std::vector<std::array<uint64_t,3>> calls;
std::vector<entid_t> dropped;
struct SHIP:Entity {
 inline static layer_index_t ExecuteLayer=1,RealizeLayer=0;
 int32_t iShipPriorityExecute,iShipPriorityRealize;
 bool Init()override{__PRIORITIES__ return true;}
 void MountLayers(){__MOUNT__}
 void Execute(uint32_t d){calls.push_back({GetId(),0,d});}
 void Realize(uint32_t d){calls.push_back({GetId(),1,d});}
 __DISPATCH__
 ~SHIP()override{dropped.push_back(GetId());}
};
struct Factory:VMA{Factory(){SetHash(static_cast<int32_t>(MakeHashValue("ship")));}const char*GetName()override{return "ship";}void*CreateClass()override{return new SHIP;}};
void ids(const auto&values){bool first=true;std::cout<<"[";for(auto v:values){if(!first)std::cout<<",";first=false;std::cout<<v;}std::cout<<"]";}
int main(int argc,char**argv){int profile=std::stoi(argv[1]);Factory factory;manager.SetLayerType(1,layer_type_t::execute);manager.SetLayerType(0,layer_type_t::realize);CoreImpl engine;std::vector<entid_t> history;std::vector<std::array<uint32_t,2>> ps;
 auto create=[&](int n){int32_t vals[]={2,31,-1,0,2147483647,-2147483647};ex=profile%4==0?std::nullopt:std::optional<VDATA>{{vals[(profile+n)%6]}};rz=profile%3==0?std::nullopt:std::optional<VDATA>{{vals[(profile+2*n+1)%6]}};auto id=manager.CreateEntity("ship",nullptr);auto*s=static_cast<SHIP*>(manager.GetEntityPointer(id));s->MountLayers();history.push_back(id);ps.push_back({uint32_t(s->iShipPriorityExecute),uint32_t(s->iShipPriorityRealize)});};
 create(0);create(1);std::cout<<"[";
 for(int step=0;step<16;step++){if(step)std::cout<<",";if(step==4)manager.SetLayerFrozen(1,true);if(step==6)manager.SetLayerFrozen(1,false);if(step==8)manager.EraseEntity(history[0]);if(step==12)create(2);if(step==14)manager.RemoveFromLayer(0,history[1]);calls.clear();engine.Timer.delta=step%4==0?0:step%4==1?16:step%4==2?17:1000;engine.ProcessExecute();engine.ProcessRealize();
 std::cout<<"{\"step\":"<<step<<",\"delta\":"<<engine.Timer.delta<<",\"priorities\":[";for(size_t i=0;i<ps.size();i++){if(i)std::cout<<",";ids(ps[i]);}std::cout<<"],\"calls\":[";for(size_t i=0;i<calls.size();i++){if(i)std::cout<<",";ids(calls[i]);}std::cout<<"],\"before_lifecycle\":[";for(size_t i=0;i<history.size();i++){if(i)std::cout<<",";std::cout<<"["<<history[i]<<","<<(manager.IsEntityValid(history[i])?"true":"false")<<","<<(manager.GetEntityPointer(history[i])?"true":"false")<<"]";}
 manager.NewLifecycle();std::cout<<"],\"after_lifecycle\":[";for(size_t i=0;i<history.size();i++){if(i)std::cout<<",";std::cout<<"["<<history[i]<<","<<(manager.IsEntityValid(history[i])?"true":"false")<<","<<(manager.GetEntityPointer(history[i])?"true":"false")<<"]";}
 std::cout<<"],\"execute_ids\":";ids(manager.GetEntityIds(layer_type_t::execute));std::cout<<",\"realize_ids\":";ids(manager.GetEntityIds(layer_type_t::realize));std::cout<<",\"dropped\":";ids(dropped);std::cout<<"}";
 }std::cout<<"]";manager.EraseAll();}
'''
driver=driver.replace('__CORE__',parts['core_execute']+'\n'+parts['core_realize']).replace('__PRIORITIES__',priorities).replace('__MOUNT__',mount).replace('__DISPATCH__',dispatch)
with tempfile.TemporaryDirectory(prefix='storm-ship-stages-') as folder:
 p=Path(folder)
 for f in files:shutil.copyfile(repo/'src/libs/core'/f,p/Path(f).name)
 (p/'attributes.h').write_text('#pragma once\nstruct ATTRIBUTES{};\n')
 (p/'plf_stack.h').write_text('#pragma once\n#include <vector>\nnamespace plf{template<class T>struct stack{std::vector<T>data;bool empty()const{return data.empty();}void clear(){data.clear();}void push(T v){data.push_back(v);}void pop(){data.pop_back();}T top()const{return data.back();}};}\n')
 (p/'main.cpp').write_text(driver)
 subprocess.run(['clang++','-std=c++20','-ffp-contract=off','-fsanitize=undefined','-fno-sanitize-recover=all','-I'+str(p),'-I'+str(repo/'src/libs/util/include'),str(p/'main.cpp'),str(p/'entity_manager.cpp'),str(p/'entity_container_cache.cpp'),'-o',str(p/'oracle')],check=True)
 cases=[{'profile':i,'frames':json.loads(subprocess.check_output([str(p/'oracle'),str(i)],text=True))} for i in range(32)]
(here/'cases.json').write_text(json.dumps({'scope':__doc__,'retained_sha256':{k:hashlib.sha256(v.encode()).hexdigest() for k,v in parts.items()},'core_files_sha256':{f:hashlib.sha256((repo/'src/libs/core'/f).read_bytes()).hexdigest() for f in files},'cases':cases},separators=(',',':'))+'\n')
print('32 native ship dispatch profiles / 512 complete registry and callback snapshots under UBSan')
