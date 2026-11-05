"""Compile unedited FirePlace lifecycle methods and SHIP fire-message cases under UBSan.

Core particle/sound dispatch, vector/matrix position, MESSAGE storage, object
layout and assertion-to-error handling are adapters. Native service calls and
defined state are recorded; the constructor's uninitialized run time is never
read. Original particle simulation, audio playback and save/load are excluded.
"""
from pathlib import Path
import hashlib,json,re,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/ship/src/fire_place.cpp').is_file())
native=(repo/'src/libs/ship/src/fire_place.cpp').read_text()
ship=(repo/'src/libs/ship/src/ship.cpp').read_text()
headers=(repo/'src/libs/shared_headers/include/shared/messages.h').read_text()+'\n'+(repo/'src/libs/shared_headers/include/shared/sound.h').read_text()
def method(source,signature):
 start=source.index(signature);return source[start:source.index('\n}',start)+2]
signatures=['FirePlace::FirePlace()','FirePlace::~FirePlace()', 'bool FirePlace::CreateParticle(', 'void FirePlace::DeleteParticle()', 'void FirePlace::Run(', 'void FirePlace::StopSound(', 'void FirePlace::Stop()', 'void FirePlace::Execute(']
bodies={signature:method(native,signature) for signature in signatures}
for label,end in [('MSG_SHIP_ACTIVATE_FIRE_PLACE','MSG_SHIP_GET_CHARACTER_INDEX'),('MSG_SHIP_SAFE_DELETE','MSG_SHIP_FLAG_REFRESH')]:
 start=ship.index('    case '+label+':',ship.index('uint64_t SHIP::ProcessMessage'))
 bodies[label]=ship[start:ship.index('    case '+end+':',start)]
names=['PS_CREATE_RIC','PS_VALIDATE_PARTICLE','MSG_SOUND_PLAY','MSG_SOUND_STOP','MSG_SOUND_SET_3D_PARAM','SOUND_WAV_3D','VOLUME_FX','SOUND_PARAM_POSITION','MSG_SHIP_ACTIVATE_FIRE_PLACE','MSG_SHIP_SAFE_DELETE']
constants={name:int(re.search(r'#define\s+'+name+r'\s+(\d+)',headers).group(1)) for name in names}
source=r'''
#include <bit>
#include <cstdarg>
#include <cstdint>
#include <iomanip>
#include <iostream>
#include <memory>
#include <sstream>
#include <stdexcept>
#include <string>
#include <variant>
#include <vector>
using entid_t=uint64_t;
struct CVECTOR {float x,y,z;};
CVECTOR position{1,2,3};
std::vector<std::string> events;
std::string bits(CVECTOR p){return std::to_string(std::bit_cast<uint32_t>(p.x))+","+std::to_string(std::bit_cast<uint32_t>(p.y))+","+std::to_string(std::bit_cast<uint32_t>(p.z));}
#define Assert(value) do {if(!(value))throw std::runtime_error("assert");} while(false)
__CONSTANTS__
constexpr uint32_t INVALID_ARRAY_INDEX=0xffffffff;
struct VPARTICLE_SYSTEM {
 int id;
 void Pause(bool b){Assert(b);events.push_back("pause:"+std::to_string(id));}
 void SetEmitter(CVECTOR p,CVECTOR d){Assert(d.x==0&&d.y==1&&d.z==0);events.push_back("move:"+std::to_string(id)+":"+bits(p));}
};
struct Core {
 bool available=true;int create_mask=3,valid_mask=3,sound=7,next=0;
 std::vector<std::unique_ptr<VPARTICLE_SYSTEM>> systems;
 entid_t GetEntityId(const char*){return available?1:0;}
 uint64_t Send_Message(entid_t,const char*,int code,...){
  va_list args;va_start(args,code);uint64_t result=0;
  if(code==PS_CREATE_RIC){
   auto name=va_arg(args,const char*);CVECTOR p{float(va_arg(args,double)),float(va_arg(args,double)),float(va_arg(args,double))};
   auto x=va_arg(args,double),y=va_arg(args,double),z=va_arg(args,double);Assert(x==0&&y==1&&z==0);Assert(va_arg(args,int)==0);
   ++next;int bit=(next-1)%2;events.push_back("create:"+std::to_string(next)+":"+name+":"+bits(p));
   if(create_mask&(1<<bit)){auto system=std::make_unique<VPARTICLE_SYSTEM>();system->id=next;result=reinterpret_cast<uintptr_t>(system.get());systems.push_back(std::move(system));}
  }else if(code==PS_VALIDATE_PARTICLE){
   auto p=va_arg(args,VPARTICLE_SYSTEM*);result=(valid_mask&(1<<((p->id-1)%2)))?1:0;events.push_back("validate:"+std::to_string(p->id)+":"+std::to_string(result));
  }else if(code==MSG_SOUND_PLAY){
   auto name=va_arg(args,const char*);Assert(va_arg(args,int)==SOUND_WAV_3D);Assert(va_arg(args,int)==VOLUME_FX);
   Assert(va_arg(args,int)==0);Assert(va_arg(args,int)==1);Assert(va_arg(args,int)==0);Assert(va_arg(args,int)==0);
   CVECTOR p{float(va_arg(args,double)),float(va_arg(args,double)),float(va_arg(args,double))};
   events.push_back("play:"+std::string(name)+":"+bits(p));result=sound;
  }else if(code==MSG_SOUND_STOP){auto id=va_arg(args,int);Assert(va_arg(args,int)==0);events.push_back("stop:"+std::to_string(id));
  }else if(code==MSG_SOUND_SET_3D_PARAM){auto id=va_arg(args,int);Assert(va_arg(args,int)==SOUND_PARAM_POSITION);
   CVECTOR p{float(va_arg(args,double)),float(va_arg(args,double)),float(va_arg(args,double))};events.push_back("sound_move:"+std::to_string(id)+":"+bits(p));
  }else{throw std::runtime_error("unexpected service message");}
  va_end(args);return result;
 }
} core;
struct FirePlace {
 static entid_t eidSound;
 VPARTICLE_SYSTEM *pParticleSmoke,*pParticleFire;void* pShip;int32_t iSoundID,iBallCharacterIndex;
 bool bActive;float fRunTime;std::string sParticleSmokeName,sParticleFireName,sSoundName;
 FirePlace();~FirePlace();bool CreateParticle(const char*,const char*);void DeleteParticle();
 void Run(const char*,const char*,int32_t,const char*,float);void StopSound(int32_t);void Stop();void Execute(float);
 void SetShip(void* p){pShip=p;}void SetActive(bool b){bActive=b;}bool isActive(){return bActive;}CVECTOR GetPos(){return position;}
};
entid_t FirePlace::eidSound=2;
__METHODS__
void event_json(){std::cout<<"[";bool first=true;for(auto& e:events){if(!first)std::cout<<",";first=false;std::cout<<std::quoted(e);}std::cout<<"]";events.clear();}
void snapshot(FirePlace* p,bool known){
 std::cout<<"{\"state\":";
 if(!p)std::cout<<"null";else{
  std::cout<<"{\"active\":"<<(p->bActive?"true":"false")<<",\"remaining_bits\":";
  if(known)std::cout<<std::bit_cast<uint32_t>(p->fRunTime);else std::cout<<"null";
  std::cout<<",\"owner\":"<<p->iBallCharacterIndex<<",\"sound\":"<<p->iSoundID
   <<",\"smoke\":"<<(p->pParticleSmoke?p->pParticleSmoke->id:0)<<",\"fire\":"<<(p->pParticleFire?p->pParticleFire->id:0)
   <<",\"names\":["<<std::quoted(p->sParticleSmokeName)<<","<<std::quoted(p->sParticleFireName)<<","<<std::quoted(p->sSoundName)<<"]}";
 }
 std::cout<<",\"events\":";event_json();std::cout<<"}";
}
void lifecycle(bool available,int create,int valid,int sound,uint32_t duration,uint32_t delta,bool lost){
 core=Core{};core.available=available;core.create_mask=create;core.valid_mask=valid;core.sound=sound;events.clear();position={1,2,3};
 auto p=std::make_unique<FirePlace>();bool known=false;std::cout<<"[";snapshot(p.get(),known);
 p->Run("smoke","fire",7,"sound",std::bit_cast<float>(duration));known|=p->isActive();std::cout<<",";snapshot(p.get(),known);
 p->Run("ignored","ignored",19,"ignored",99);known|=p->isActive();std::cout<<",";snapshot(p.get(),known);
 position={4,5,6};p->Execute(std::bit_cast<float>(delta));std::cout<<",";snapshot(p.get(),known);
 if(lost)core.available=false;p->Stop();std::cout<<",";snapshot(p.get(),known);
 p->Run("restart-smoke","restart-fire",23,"restart-sound",2);known|=p->isActive();std::cout<<",";snapshot(p.get(),known);
 position={7,8,9};p->Execute(0.5f);std::cout<<",";snapshot(p.get(),known);
 p.reset();std::cout<<",";snapshot(nullptr,false);std::cout<<"]\n";
}
struct MESSAGE {
 std::vector<std::variant<int32_t,std::string,float>> values;size_t cursor=0;
 int32_t Long(){return std::get<int32_t>(values.at(cursor++));}
 const std::string& String(){return std::get<std::string>(values.at(cursor++));}
 float Float(){return std::get<float>(values.at(cursor++));}
};
struct SHIP {std::vector<FirePlace> aFirePlaces;uint64_t ProcessMessage(MESSAGE& message){auto code=message.Long();switch(code){__SHIP__}return 0;}};
void messages(int count,int index){
 core=Core{};events.clear();position={1,2,3};SHIP ship;ship.aFirePlaces.resize(count);
 MESSAGE activate{{int32_t(MSG_SHIP_ACTIVATE_FIRE_PLACE),int32_t(index),std::string("smoke"),std::string("fire"),std::string("sound"),1.0f,int32_t(7)}};
 bool error=false;try{ship.ProcessMessage(activate);if(count>1&&index==2){activate.values[1]=int32_t(0);activate.cursor=0;ship.ProcessMessage(activate);}}catch(const std::exception&){error=true;}
 std::cout<<"{\"error\":"<<(error?"true":"false")<<",\"active\":";
 int active=0;for(auto& p:ship.aFirePlaces)active+=p.isActive();std::cout<<active<<",\"events\":";event_json();
 MESSAGE clear{{int32_t(MSG_SHIP_SAFE_DELETE)}};auto result=ship.ProcessMessage(clear);
 std::cout<<",\"clear_result\":"<<result<<",\"remaining_places\":"<<ship.aFirePlaces.size()<<",\"clear_events\":";event_json();std::cout<<"}\n";
}
int main(){__RUNS__}
'''
source=source.replace('__CONSTANTS__','\n'.join(f'constexpr int32_t {k}={v};' for k,v in constants.items()))
source=source.replace('__METHODS__','\n'.join(bodies[k] for k in signatures)).replace('__SHIP__','\n'.join(bodies[k] for k in ['MSG_SHIP_ACTIVATE_FIRE_PLACE','MSG_SHIP_SAFE_DELETE']))
pairs=[(0,0),(0x80000000,0),(0x3f800000,0x3f800000),(0x3f800000,0x3f900000),(0xbf800000,0),(0x7f800000,0x3f000000),(0xff800000,0),(0x7fc00000,0x3e000000),(0x3f800000,0xbf000000),(0x3f800000,0x7f800000),(0x3f800000,0x7fc00000),(1,0)]
cases=[{'available':True,'create_mask':c,'valid_mask':v,'sound':s,'duration_bits':0x3f800000,'delta_bits':0x3f900000,'lost':False} for c in range(4) for v in range(4) for s in [0,7]]
cases += [{'available':a,'create_mask':3,'valid_mask':3,'sound':7,'duration_bits':d,'delta_bits':dt,'lost':False} for a in [False,True] for d,dt in pairs]
cases += [{'available':True,'create_mask':3,'valid_mask':v,'sound':7,'duration_bits':0x3f800000,'delta_bits':0x3f000000,'lost':True} for v in range(4)]
messages=[{'count':n,'index':i} for n in [0,1,3] for i in [-1,0,2,3,-2147483648]]
runs=[f"lifecycle({str(c['available']).lower()},{c['create_mask']},{c['valid_mask']},{c['sound']},{c['duration_bits']}u,{c['delta_bits']}u,{str(c['lost']).lower()});" for c in cases]
runs += [f"messages({c['count']},{c['index']});" for c in messages]
source=source.replace('__RUNS__','\n'.join(runs))
with tempfile.TemporaryDirectory(prefix='storm-fire-reference-') as folder:
 path=Path(folder);(path/'main.cpp').write_text(source)
 subprocess.run(['clang++','-std=c++20','-fsanitize=undefined','-fno-sanitize-recover=all',str(path/'main.cpp'),'-o',str(path/'oracle')],check=True)
 rows=subprocess.run([str(path/'oracle')],check=True,capture_output=True,text=True,timeout=30).stdout.splitlines()
assert len(rows)==len(cases)+len(messages)
for c,row in zip(cases,rows):c['expected']=json.loads(row)
for c,row in zip(messages,rows[len(cases):]):c['expected']=json.loads(row)
(here/'cases.json').write_text(json.dumps({'scope':__doc__,'constants':constants,'retained_sha256':{k:hashlib.sha256(v.encode()).hexdigest() for k,v in bodies.items()},'lifecycle':cases,'messages':messages},indent=2)+'\n')
print(f'{len(cases)} lifecycle sequences / {len(cases)*8} snapshots; {len(messages)} native SHIP message sequences under UBSan')
