"""Unedited native manager registration/removal/validation/clear and IsAlive.
Construction, data-source contents, release storage and output are adapters.
Release callbacks retain allocations until oracle exit to keep deterministic IDs;
this verifies native vector/callback order, not allocator reuse or full processing.
"""
from pathlib import Path
import hashlib,json,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/particles').is_dir())
bodies={}
for path,sigs in [
 ('manager/particle_manager.cpp',['void ParticleManager::RemoveResource(','bool ParticleManager::ValidateSystem(','void ParticleManager::DeleteAllSystems(','ParticleSystem *ParticleManager::CreateParticleSystemFromDataSource(']),
 ('system/particle_system/particle_system.cpp',['bool ParticleSystem::IsAlive(']),
]:
 text=(repo/'src/libs/particles/src'/path).read_text()
 for sig in sigs:
  start=text.index(sig);bodies[sig]=text[start:text.index('\n}',start)+2]
actions=[['create',0],['create',0],['create',0],['create',0],['release',2],['release',1],['release',99],['create',0],['clear',0],['clear',0],['create',0],['release',6]]
alive=[[],[[True,0]],[[True,1]],[[False,0]],[[True,0],[True,0]],[[True,1],[True,0]],[[True,0],[False,0]],[[False,0],[True,0]],[[True,0],[True,2]]]
source=r'''
#include <cstdint>
#include <iostream>
#include <vector>
struct DataSource{};
struct Emitter {bool stopped;uint32_t count;bool IsStoped(){return stopped;}uint32_t GetParticleCount(){return count;}};
struct ParticleManager;
struct ParticleSystem {struct EmitterDesc{Emitter *pEmitter;};std::vector<EmitterDesc> Emitters;ParticleManager *master;int id;static std::vector<ParticleSystem*> allocated;explicit ParticleSystem(ParticleManager* m):master(m),id(allocated.size()+1){allocated.push_back(this);}void CreateFromDataSource(DataSource*){}bool Release();bool IsAlive();};
std::vector<ParticleSystem*> ParticleSystem::allocated;
struct IParticleSystem{};
struct ParticleManager {struct SystemDesc{ParticleSystem*pSystem;};std::vector<SystemDesc> Systems;bool GlobalDelete=false;std::vector<int> released;void RemoveResource(IParticleSystem*);bool ValidateSystem(IParticleSystem*);void DeleteAllSystems();ParticleSystem*CreateParticleSystemFromDataSource(DataSource*);};
// The pointer comparisons below use the native signatures; hierarchy is storage-only.
__BODIES__
bool ParticleSystem::Release(){master->released.push_back(id);master->RemoveResource(reinterpret_cast<IParticleSystem*>(this));return false;}
int main(){ParticleManager manager;DataSource data;
 int actions[][2]={__ACTIONS__};
 for(auto &a:actions){manager.released.clear();
  if(a[0]==0)manager.CreateParticleSystemFromDataSource(&data);
  if(a[0]==1){for(auto*s:ParticleSystem::allocated)if(s->id==a[1]&&manager.ValidateSystem(reinterpret_cast<IParticleSystem*>(s))){s->Release();break;}}
  if(a[0]==2)manager.DeleteAllSystems();
  std::cout<<"{\"ids\":[";bool first=true;for(auto&s:manager.Systems){if(!first)std::cout<<",";first=false;std::cout<<s.pSystem->id;}std::cout<<"],\"valid\":[";first=true;for(auto*s:ParticleSystem::allocated){if(!first)std::cout<<",";first=false;std::cout<<(manager.ValidateSystem(reinterpret_cast<IParticleSystem*>(s))?"true":"false");}std::cout<<"],\"released\":[";first=true;for(int id:manager.released){if(!first)std::cout<<",";first=false;std::cout<<id;}std::cout<<"]}\n";
 }
 ParticleSystem probe(&manager);std::vector<std::vector<Emitter>> cases={__ALIVE__};
 std::cout<<"[";bool first=true;for(auto&c:cases){probe.Emitters.clear();for(auto&e:c)probe.Emitters.push_back({&e});if(!first)std::cout<<",";first=false;std::cout<<(probe.IsAlive()?"true":"false");}std::cout<<"]\n";
 for(auto*p:ParticleSystem::allocated)if(p!=&probe)delete p;
}
'''
# Native ParticleSystem derives IParticleSystem; retain that storage relation for pointer comparison.
source=source.replace('struct ParticleManager;\nstruct ParticleSystem','struct ParticleManager;struct IParticleSystem{};\nstruct ParticleSystem : IParticleSystem').replace('struct IParticleSystem{};\nstruct ParticleManager {','struct ParticleManager {')
source=source.replace('__BODIES__','\n'.join(bodies.values())).replace('__ACTIONS__',','.join('{'+str({'create':0,'release':1,'clear':2}[op])+','+str(arg)+'}' for op,arg in actions)).replace('__ALIVE__',','.join('{'+','.join('{'+str(stopped).lower()+','+str(count)+'}' for stopped,count in case)+'}' for case in alive))
with tempfile.TemporaryDirectory(prefix='storm-manager-') as folder:
 p=Path(folder);(p/'main.cpp').write_text(source)
 subprocess.run(['clang++','-std=c++20','-fsanitize=undefined','-fno-sanitize-recover=all',str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
 rows=[json.loads(s) for s in subprocess.check_output([str(p/'oracle')],text=True).splitlines()]
(here/'cases.json').write_text(json.dumps({'scope':__doc__,'retained_sha256':{k:hashlib.sha256(v.encode()).hexdigest() for k,v in bodies.items()},'actions':actions,'snapshots':rows[:-1],'alive_inputs':alive,'alive':rows[-1]},indent=2)+'\n')
print(str(len(rows)-1)+' complete native manager snapshots and '+str(len(alive))+' IsAlive cases under UBSan')
