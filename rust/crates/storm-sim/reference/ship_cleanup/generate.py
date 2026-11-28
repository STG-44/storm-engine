"""Unedited SHIP destructor, logical group deletion, ShipTracks::DelShip and
original EntityManager/cache/Entity/VMA under UBSan. Models, service ownership,
messages and track nodes are probes; rendering and buffer compaction are absent.
"""
from pathlib import Path
import hashlib,json,shutil,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/ship/src/ship.cpp').exists())
def method(text,start):
 a=text.index(start);return text[a:text.index('\n}',a)+2]
def case(text,start,end):
 a=text.index(start);return text[a:text.index(end,a)]
ship=(repo/'src/libs/ship/src/ship.cpp').read_text();rig=repo/'src/libs/rigging/src'
parts={'ship_destructor':method(ship,'SHIP::~SHIP()'),
 'sail':case((rig/'sail.cpp').read_text(),'    case MSG_SAIL_DEL_GROUP:','        // release a group'),
 'rope':case((rig/'rope.cpp').read_text(),'    case MSG_ROPE_DEL_GROUP:','    }\n\n    return 0;'),
 'vant':case((rig/'vant.cpp').read_text(),'    case MSG_VANT_DEL_GROUP:','    case MSG_VANT_DEL_MAST:'),
 'flag':method((rig/'flag.cpp').read_text(),'void FLAG::GroupSTORM_DELETE('),
 'track':method((repo/'src/libs/ship/src/track.cpp').read_text(),'void ShipTracks::DelShip(')}
files=['src/entity_manager.cpp','src/entity_manager.h','src/entity_container_cache.cpp','src/entity_container_cache.h','include/entity.h','include/vma.hpp']
driver=r'''
#include "entity_manager.h"
#include "vma.hpp"
#include "shared/sail_msg.h"
#include <iostream>
#include <sstream>
#include <string>
#include <array>
#define STORM_DELETE(p) do{delete p;p=nullptr;}while(0)
EntityManager manager;
std::vector<std::string> events;
std::vector<entid_t> history;
entid_t activeShip=0,activeModel=0,ships[2]{},models[2]{},blots[2]{},bonus=0,services[6]{},tracksID=0;
std::string number(entid_t id){return std::to_string(id);}
std::string truth(bool v){return v?"true":"false";}
bool lookup(entid_t id){return manager.GetEntityPointer(id)!=nullptr;}
void drop(entid_t id,int kind){events.push_back("[\"drop\","+number(id)+","+std::to_string(kind)+"]");}
struct G{entid_t shipEI=0,modelEI=0,model_id=0;bool bDeleted=false;};
struct Data{G groups[2];bool pending=false;}data[6];
class MESSAGE{public:int32_t code;entid_t owner;int32_t Long(){return code;}entid_t EntityID(){return owner;}};
struct FLAG {int groupQuantity=2;G*gdata;bool&bYesDeleted;explicit FLAG(Data&d):gdata(d.groups),bYesDeleted(d.pending){}void GroupSTORM_DELETE(entid_t);};
__FLAG__
struct Service:Entity,FLAG{int kind;bool&bDeleteState;explicit Service(int k):FLAG(data[k]),kind(k),bDeleteState(data[k].pending){}
 bool Init()override{return true;}void ProcessStage(Stage,uint32_t)override{}
 uint64_t ProcessMessage(MESSAGE&message)override{int codes[]={MSG_SAIL_DEL_GROUP,MSG_ROPE_DEL_GROUP,MSG_FLAG_DEL_GROUP,MSG_VANT_DEL_GROUP,MSG_VANT_DEL_GROUP,MSG_VANT_DEL_GROUP};if(message.code!=codes[kind])return 0;entid_t tmpEI=0,tmp_id=0;int i;switch(message.Long()){
 __SAIL__
 __ROPE__
 __VANT__
 case MSG_FLAG_DEL_GROUP:GroupSTORM_DELETE(message.EntityID());break;
 }return 0;}
};
struct SHIP;using SHIP_BASE=SHIP;
struct Track {SHIP*pShip;entid_t id;~Track(){events.push_back("[\"track_drop\","+number(id)+"]");}};
struct ShipTracks:Entity {std::vector<Track*>aShips;bool Init()override{return true;}void ProcessStage(Stage,uint32_t)override{}void DelShip(SHIP_BASE*);~ShipTracks()override{for(auto*p:aShips)delete p;}};
__TRACK__
struct Core {
 void EraseEntity(entid_t id){for(int i=0;i<2;i++)if(models[i]&&id==models[i]){activeShip=ships[i];activeModel=models[i];}manager.EraseEntity(id);events.push_back("[\"erase\","+number(id)+","+number(activeShip)+","+truth(lookup(activeShip))+","+truth(manager.IsEntityValid(activeShip))+","+truth(lookup(activeModel))+","+truth(manager.IsEntityValid(activeModel))+"]");}
 uint64_t Send_Message(entid_t target,const char*,int32_t code,entid_t owner){bool valid=manager.IsEntityValid(target);events.push_back("[\"send\","+number(target)+","+std::to_string(code)+","+number(owner)+","+truth(valid)+","+truth(lookup(activeModel))+","+truth(manager.IsEntityValid(activeModel))+"]");if(valid){MESSAGE m;m.code=code;m.owner=owner;return manager.GetEntityPointer(target)->ProcessMessage(m);}return 0;}
 entid_t GetEntityId(const char*n){return manager.GetEntityId(n);}
 Entity*GetEntityPointer(entid_t id){events.push_back("[\"track_lookup\","+number(id)+"]");return manager.GetEntityPointer(id);}
}core;
int nextShip=0,nextKind=0;entid_t nextChild=0;
struct Tail{entid_t id=0;~Tail(){drop(id,0);}};
struct SHIP:Entity {entid_t model=0,blots_id=0,sail_id=0,rope_id=0,flag_id=0,vant_id=0,vantl_id=0,vantz_id=0;Tail tail;
 bool Init()override{int i=nextShip;model=models[i];blots_id=blots[i];sail_id=services[0];rope_id=services[1];flag_id=services[2];vant_id=services[3];vantl_id=services[4];vantz_id=services[5];tail.id=GetId();manager.AddToLayer(1,GetId(),2);manager.AddToLayer(0,GetId(),31);return true;}
 entid_t GetModelEID(){return model;}void ProcessStage(Stage,uint32_t)override{}~SHIP()override;
};
__SHIP__
struct Probe:Entity {int kind;entid_t child;Probe():kind(nextKind),child(nextChild){}bool Init()override{return true;}void ProcessStage(Stage,uint32_t)override{}~Probe()override{if(child)core.EraseEntity(child);drop(GetId(),kind);}};
struct Factory:VMA{std::string name;int kind;Factory(std::string n,int k):name(n),kind(k){SetHash(static_cast<int32_t>(MakeHashValue(name.c_str())));}const char*GetName()override{return name.c_str();}void*CreateClass()override{if(kind<6)return new ::Service(kind);if(kind==6)return new ShipTracks;if(kind==7)return new SHIP;return new Probe;}};
entid_t create(const char*name,int kind=3,entid_t child=0){nextKind=kind;nextChild=child;auto id=manager.CreateEntity(name,nullptr);history.push_back(id);return id;}
template<class T>void ids(const T&v){std::cout<<"[";bool first=true;for(auto id:v){if(!first)std::cout<<",";first=false;std::cout<<id;}std::cout<<"]";}
void snapshot(int step){std::cout<<"{\"step\":"<<step<<",\"entities\":[";bool first=true;for(auto id:history){if(!first)std::cout<<",";first=false;std::cout<<"["<<id<<","<<truth(manager.IsEntityValid(id))<<","<<truth(lookup(id))<<"]";}std::cout<<"],\"groups\":[";for(int k=0;k<6;k++){if(k)std::cout<<",";std::cout<<"[";for(int i=0;i<2;i++){if(i)std::cout<<",";std::cout<<"["<<data[k].groups[i].shipEI<<","<<truth(data[k].groups[i].bDeleted)<<"]";}std::cout<<"]";}
 std::cout<<"],\"pending\":[";for(int k=0;k<6;k++){if(k)std::cout<<",";std::cout<<truth(data[k].pending);}std::cout<<"],\"tracks\":[";if(tracksID){auto*p=static_cast<ShipTracks*>(manager.GetEntityPointer(tracksID));if(p){for(size_t i=0;i<p->aShips.size();i++){if(i)std::cout<<",";std::cout<<p->aShips[i]->id;}}}std::cout<<"],\"execute\":";ids(manager.GetEntityIds(layer_type_t::execute));std::cout<<",\"realize\":";ids(manager.GetEntityIds(layer_type_t::realize));std::cout<<",\"events\":[";for(size_t i=0;i<events.size();i++){if(i)std::cout<<",";std::cout<<events[i];}std::cout<<"]}";
}
int main(int argc,char**argv){int profile=std::stoi(argv[1]);Factory a("sail",0),b("rope",1),c("flag",2),d("vant",3),e("vantl",4),f("vantz",5),g("ShipTracks",6),h("ship",7),j("model",8),k("blots",8),l("probe",8);const char*names[]={"sail","rope","flag","vant","vantl","vantz"};manager.SetLayerType(1,layer_type_t::execute);manager.SetLayerType(0,layer_type_t::realize);
 for(int i=0;i<6;i++)if(profile&(1<<i))services[i]=create(names[i]);if(profile%3)tracksID=create("ShipTracks");bonus=create("probe",3);
 for(int i=0;i<2;i++){models[i]=create("model",1,i==0&&profile%2==0?bonus:0);blots[i]=create("blots",2);nextShip=i;ships[i]=create("ship",0);for(int s=0;s<6;s++){auto owner=s==0?ships[i]:models[i];data[s].groups[i]={owner,owner,owner,false};}if(tracksID)static_cast<ShipTracks*>(manager.GetEntityPointer(tracksID))->aShips.push_back(new Track{static_cast<SHIP*>(manager.GetEntityPointer(ships[i])),ships[i]});}
 std::cout<<"[";for(int step=0;step<8;step++){if(step)std::cout<<",";if(step==1){core.Send_Message(services[0],"li",MSG_SAIL_DEL_GROUP,models[0]);core.Send_Message(services[1],"li",MSG_ROPE_DEL_GROUP,ships[0]);}if(step==2)manager.EraseEntity(ships[0]);if(step==3||step==4||step==7)manager.NewLifecycle();if(step==4)manager.EraseEntity(ships[0]);if(step==5)create("probe");if(step==6){if(profile&32)manager.EraseEntity(services[2]);manager.EraseEntity(ships[1]);}snapshot(step);}std::cout<<"]";manager.EraseAll();}
'''
driver=driver.replace('__FLAG__',parts['flag']).replace('__SAIL__',parts['sail']).replace('__ROPE__',parts['rope']).replace('__VANT__',parts['vant']).replace('__TRACK__',parts['track']).replace('__SHIP__',parts['ship_destructor'])
with tempfile.TemporaryDirectory(prefix='storm-ship-cleanup-') as directory:
 p=Path(directory)
 for f in files:shutil.copyfile(repo/'src/libs/core'/f,p/Path(f).name)
 (p/'attributes.h').write_text('#pragma once\nstruct ATTRIBUTES{};\n')
 (p/'plf_stack.h').write_text('#pragma once\n#include <vector>\nnamespace plf{template<class T>struct stack{std::vector<T>data;bool empty()const{return data.empty();}void clear(){data.clear();}void push(T v){data.push_back(v);}void pop(){data.pop_back();}T top()const{return data.back();}};}\n')
 (p/'main.cpp').write_text(driver)
 subprocess.run(['clang++','-std=c++20','-fsanitize=undefined','-fno-sanitize-recover=all','-I'+str(p),'-I'+str(repo/'src/libs/util/include'),'-I'+str(repo/'src/libs/shared_headers/include'),str(p/'main.cpp'),str(p/'entity_manager.cpp'),str(p/'entity_container_cache.cpp'),'-o',str(p/'oracle')],check=True)
 cases=[{'profile':i,'snapshots':json.loads(subprocess.check_output([str(p/'oracle'),str(i)],text=True))} for i in range(64)]
(here/'cases.json').write_text(json.dumps({'scope':__doc__,'retained_sha256':{k:hashlib.sha256(v.encode()).hexdigest() for k,v in parts.items()},'core_files_sha256':{f:hashlib.sha256((repo/'src/libs/core'/f).read_bytes()).hexdigest() for f in files},'cases':cases},separators=(',',':'))+'\n')
print('64 native cleanup profiles / 512 complete ownership, registry and teardown snapshots under UBSan')
