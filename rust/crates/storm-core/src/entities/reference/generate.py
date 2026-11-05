"""Run unedited retained EntityManager, cache, Entity and VMA under UBSan.

The driver supplies diagnostic class factories/Init bodies, a minimal attribute
payload and a vector-backed plf::stack adapter. This is a native lifecycle
oracle, not execution of original commercial classes or the script compiler.
"""
from pathlib import Path
import hashlib
import json
import random
import shutil
import subprocess
import tempfile

here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/core/src/entity_manager.cpp').is_file())
core=repo/'src/libs/core'
files=['src/entity_manager.cpp','src/entity_manager.h','src/entity_container_cache.cpp',
       'src/entity_container_cache.h','include/entity.h','include/vma.hpp']
driver=r'''
#include "entity_manager.h"
#include "vma.hpp"
#include <iostream>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>
struct InitRecord {entid_t id;int mode,root;bool valid,listed;};
EntityManager manager;
ATTRIBUTES roots[2]{{0,0},{1,0}};
std::vector<entid_t> history,dropped;
std::vector<InitRecord> initialized;
struct Probe:Entity {
 int mode;
 explicit Probe(int m):mode(m){}
 bool Init()override{
  auto id=GetId();int root=AttributesPointer?AttributesPointer->tag:-1;
  auto& ids=manager.GetEntityIds(mode==4?"parent":mode==3?"self":mode==1?"reject":mode==2?"error":mode==6?"ba":mode==5?"aq":"ok");
  bool listed=std::find(ids.begin(),ids.end(),id)!=ids.end();
  initialized.push_back({id,mode,root,manager.IsEntityValid(id),listed});history.push_back(id);
  if(AttributesPointer)AttributesPointer->writes++;
  manager.AddToLayer(2,id,7);
  if(mode==1)return false;
  if(mode==2)throw std::runtime_error("diagnostic Init error");
  if(mode==3)manager.EraseEntity(id);
  if(mode==4){manager.CreateEntity("ok",AttributesPointer);return false;}
  return true;
 }
 void ProcessStage(Stage,uint32_t)override{}
 ~Probe()override{dropped.push_back(GetId());}
};
struct Factory:VMA {
 std::string name;int mode;
 Factory(std::string n,int m):name(n),mode(m){SetHash(static_cast<int32_t>(MakeHashValue(name.c_str())));}
 const char*GetName()override{return name.c_str();}
 void*CreateClass()override{if(mode==7)return nullptr;return new Probe(mode);}
};
int main(){
 // Duplicate registration deliberately follows the first one. aq/ba collide.
 Factory ok("ok",0),duplicate("OK",1),reject("reject",1),error("error",2),self("self",3),parent("parent",4),aq("aq",5),ba("ba",6),null("null",7);
 manager.SetLayerType(2,layer_type_t::execute);
 char op;std::string name;int index;
 while(std::cin>>op){
  bool failed=false;entid_t result=0;
  try{
   if(op=='c'){std::cin>>name>>index;result=manager.CreateEntity(name.c_str(),index<0?nullptr:&roots[index]);}
   if(op=='d'){std::cin>>index;manager.EraseEntity(index<0?0:history.at(index));}
   if(op=='f')manager.NewLifecycle();
  }catch(const std::exception&){failed=true;}
  std::cout<<"{\"error\":"<<(failed?"true":"false")<<",\"result\":"<<result<<",\"history\":[";
  bool first=true;for(auto id:history){if(!first)std::cout<<',';first=false;auto*e=manager.GetEntityPointer(id);std::cout<<'['<<id<<','<<(manager.IsEntityValid(id)?"true":"false")<<','<<(e?"true":"false")<<','<<(e&&e->AttributesPointer?e->AttributesPointer->tag:-1)<<']';}
  std::cout<<"],\"initialized\":[";first=true;
  for(auto r:initialized){if(!first)std::cout<<',';first=false;std::cout<<'['<<r.id<<','<<r.mode<<','<<r.root<<','<<(r.valid?"true":"false")<<','<<(r.listed?"true":"false")<<']';}
  std::cout<<"],\"dropped\":[";first=true;for(auto id:dropped){if(!first)std::cout<<',';first=false;std::cout<<id;}
  std::cout<<"],\"classes\":[";first=true;
  for(auto n:{"ok","reject","error","self","parent","aq","ba"}){if(!first)std::cout<<',';first=false;std::cout<<'[';bool f=true;for(auto id:manager.GetEntityIds(n)){if(!f)std::cout<<',';f=false;std::cout<<id;}std::cout<<']';}
  std::cout<<"],\"layer\":[";first=true;for(auto id:manager.GetEntityIds(layer_type_t::execute)){if(!first)std::cout<<',';first=false;std::cout<<id;}
  std::cout<<"],\"writes\":["<<roots[0].writes<<','<<roots[1].writes<<"]}\n";
 }
 manager.EraseAll();
}
'''
# Original throwing Init leaves its instance valid. Rust deliberately queues
# Result::Err instances for safe cleanup; test that divergence separately.
sequences=[
 [['c','OK',0],['c','reject',1],['c','ok',-1],['f'],['c','ok',1],['d',0],['f'],['c','ok',0]],
 [['c','parent',0],['c','self',1],['f'],['c','ok',-1],['f']],
 [['c','aq',0],['c','BA',1],['d',0],['f'],['c','aQ',1],['c','missing',-1],['c','null',-1]],
]
rng=random.Random(4410193)
for _ in range(12):
    steps=[];created=0
    for _ in range(32):
        choice=rng.randrange(5)
        if choice<3:
            name=rng.choice(['ok','OK','reject','self','parent','aq','ba','missing','null'])
            steps.append(['c',name,rng.choice([-1,0,1])])
            if name not in ['missing','null']:created+=2 if name=='parent' else 1
        elif choice==3 and created:steps.append(['d',rng.randrange(created)])
        else:steps.append(['f'])
    sequences.append(steps)
with tempfile.TemporaryDirectory(prefix='storm-entity-creation-') as directory:
    root=Path(directory)
    for name in files:shutil.copyfile(core/name,root/Path(name).name)
    (root/'attributes.h').write_text('#pragma once\n#include <vector>\nstruct ATTRIBUTES {int tag;int writes;};\n')
    (root/'plf_stack.h').write_text('#pragma once\n#include <vector>\nnamespace plf {template<class T>struct stack{std::vector<T> data;bool empty()const{return data.empty();}void clear(){data.clear();}void push(T v){data.push_back(v);}void pop(){data.pop_back();}T top()const{return data.back();}};}\n')
    (root/'oracle.cpp').write_text(driver)
    subprocess.run(['clang++','-std=c++20','-O0','-fsanitize=undefined','-fno-sanitize-recover=all','-I',str(root),'-I',str(repo/'src/libs/util/include'),str(root/'oracle.cpp'),str(root/'entity_manager.cpp'),str(root/'entity_container_cache.cpp'),'-o',str(root/'oracle')],check=True)
    cases=[]
    for steps in sequences:
        result=subprocess.run([str(root/'oracle')],input='\n'.join(' '.join(map(str,s)) for s in steps)+'\n',text=True,capture_output=True,check=True,timeout=5)
        snapshots=[json.loads(line) for line in result.stdout.splitlines()];assert len(steps)==len(snapshots)
        cases.append({'operations':steps,'snapshots':snapshots})
fixture={'scope':__doc__,'files_sha256':{n:hashlib.sha256((core/n).read_bytes()).hexdigest() for n in files},'cases':cases}
(here/'cases.json').write_text(json.dumps(fixture,separators=(',',':'))+'\n')
print('Generated',len(cases),'native creation sequences,',sum(len(c['snapshots']) for c in cases),'snapshots under UBSan')
