"""Native factory and pause bodies; allocation ordinals, resource source and output are adapters.

This checks instance identity/retention and pause isolation, not global particle
physics, random state, automatic deletion or the complete manager.
"""
from pathlib import Path
import hashlib,json,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/particles').is_dir())
bodies={}
for path,signature in [
 ('manager/particle_manager.cpp','ParticleSystem *ParticleManager::CreateParticleSystemFromDataSource('),
 ('k2_wrapper/p_system.cpp','void PARTICLE_SYSTEM::Pause('),
 ('system/particle_system/particle_system.cpp','void ParticleSystem::PauseEmission('),
]:
 text=(repo/'src/libs/particles/src'/path).read_text();start=text.index(signature)
 bodies[signature]=text[start:text.index('\n}',start)+2]
actions=[['create',0],['create',0],['pause',1],['create',0],['create',0],['pause',2],['pause',3],['pause',4]]
source=r'''
#include <cstdint>
#include <iostream>
#include <vector>
struct DataSource{};
struct ParticleManager;
struct ParticleSystem {static int next;int id;bool EmissionPause=false;explicit ParticleSystem(ParticleManager*):id(++next){}void CreateFromDataSource(DataSource*){}void PauseEmission(bool);};
int ParticleSystem::next=0;
struct SystemDesc {ParticleSystem *pSystem;};
struct ParticleManager {std::vector<SystemDesc> Systems;ParticleSystem *CreateParticleSystemFromDataSource(DataSource*);};
struct PARTICLE_SYSTEM {ParticleSystem *pSystem;void Pause(bool);};
__BODIES__
int main(){ParticleManager manager;DataSource data;int actions[][2]={__ACTIONS__};
 for(auto &action:actions){
  if(action[0]==0)manager.CreateParticleSystemFromDataSource(&data);
  else {PARTICLE_SYSTEM wrapper{manager.Systems[action[1]-1].pSystem};wrapper.Pause(true);}
  std::cout<<"[";bool first=true;for(auto &entry:manager.Systems){if(!first)std::cout<<",";first=false;std::cout<<"{\"instance\":"<<entry.pSystem->id<<",\"paused\":"<<(entry.pSystem->EmissionPause?"true":"false")<<"}";}std::cout<<"]\n";
 }
 for(auto &entry:manager.Systems)delete entry.pSystem;
}
'''.replace('__BODIES__','\n'.join(bodies.values())).replace('__ACTIONS__',','.join('{'+str(int(op=='pause'))+','+str(arg)+'}' for op,arg in actions))
with tempfile.TemporaryDirectory(prefix='storm-particle-instances-') as folder:
 p=Path(folder);(p/'main.cpp').write_text(source)
 subprocess.run(['clang++','-std=c++20','-fsanitize=undefined','-fno-sanitize-recover=all',str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
 snapshots=[json.loads(row) for row in subprocess.check_output([str(p/'oracle')],text=True).splitlines()]
(here/'cases.json').write_text(json.dumps({'scope':__doc__,'retained_sha256':{k:hashlib.sha256(v.encode()).hexdigest() for k,v in bodies.items()},'actions':actions,'snapshots':snapshots},indent=2)+'\n')
print(f'{len(snapshots)} complete native factory/pause identity snapshots under UBSan')
