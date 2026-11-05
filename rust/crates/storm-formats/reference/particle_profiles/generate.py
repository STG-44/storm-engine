"""Retained CreateSystem/cache lookup bodies; manager, resources and allocation are adapters.

Only safe relative ASCII names are compared. Windows separators are tested
separately in Rust. No commercial files are used.
"""
from pathlib import Path
import hashlib, json, subprocess, tempfile
here = Path(__file__).resolve().parent
repo = next(p for p in here.parents if (p / 'src/libs/particles').is_dir())
bodies = {}
for path, signature in [
    ('k2_wrapper/particles.cpp', 'PARTICLE_SYSTEM *PARTICLES::CreateSystem('),
    ('data_cache/data_cache.cpp', 'DataSource *DataCache::GetParticleSystemDataSource('),
]:
    text = (repo / 'src/libs/particles/src' / path).read_text()
    start = text.index(signature)
    bodies[signature] = text[start:text.index('\n}', start) + 2]
queries = ['Ship_Fire', 'SHIP_FIRE.XPS', 'ship_fire.xps', 'Ship_Smoke', 'SHIP_SMOKE.XpS',
           'smoke.ini', 'smoke.ini.XPS', 'smoke.', '.xps', 'effects/Ship.Fire',
           'effects/SHIP_FIRE.XPS', 'missing', 'ShipFire', 'ship_fire.xps.xps']
source = r'''
#include <algorithm>
#include <cctype>
#include <cstdint>
#include <filesystem>
#include <iostream>
#include <string>
#include <vector>
namespace storm {bool iEquals(std::string a,std::string b){std::transform(a.begin(),a.end(),a.begin(),tolower);std::transform(b.begin(),b.end(),b.begin(),tolower);return a==b;}}
struct DataSource {int id;};
struct CacheEntry {std::string FileName;DataSource *pData;};
struct DataCache {std::vector<CacheEntry> Cache;DataSource *GetParticleSystemDataSource(const char*);};
struct IParticleSystem {DataSource *data;void AutoDelete(bool) {}};
struct Manager {DataCache cache;IParticleSystem *CreateParticleSystemEx(const char *name,const char*,int){auto *data=cache.GetParticleSystemDataSource(name);return data?new IParticleSystem{data}:nullptr;}};
struct PARTICLES;
struct PARTICLE_SYSTEM {IParticleSystem *p;explicit PARTICLE_SYSTEM(IParticleSystem *v):p(v){}void SetManager(PARTICLES*){}~PARTICLE_SYSTEM(){delete p;}};
struct SystemInfo {PARTICLE_SYSTEM *pSystem;uint32_t LifeTime;std::string FileName;};
struct PARTICLES {Manager *pManager;bool CreationCapture=true;std::vector<uintptr_t> CaptureBuffer;std::vector<SystemInfo> CreatedSystems;PARTICLE_SYSTEM *CreateSystem(const char*,uint32_t);};
__METHODS__
int main(){Manager m;DataSource data[7]={{1},{2},{3},{4},{5},{6},{7}};
 const char *cached[]={"ship_fire.xps","ship_smoke.xps","smoke.ini.xps","smoke..xps",".xps.xps","effects/ship.fire.xps","effects/ship_fire.xps"};
 for(int i=0;i<7;i++){auto path=std::filesystem::path()/"resource"/"particles"/cached[i];m.cache.Cache.push_back({path.string(),&data[i]});}
 PARTICLES service{&m};const char *queries[]={__QUERIES__};
 for(auto *name:queries){auto *p=service.CreateSystem(name,17);std::cout<<"{\"id\":"<<(p?p->p->data->id:0)<<",\"systems\":"<<service.CreatedSystems.size()<<",\"captured\":"<<service.CaptureBuffer.size()<<"}\n";}
 for(auto info:service.CreatedSystems)delete info.pSystem;
}
'''.replace('__METHODS__', '\n'.join(bodies.values())).replace('__QUERIES__', ','.join(json.dumps(q) for q in queries))
with tempfile.TemporaryDirectory(prefix='storm-profile-cache-') as folder:
    p = Path(folder)
    (p/'main.cpp').write_text(source)
    subprocess.run(['clang++','-std=c++20','-fsanitize=undefined','-fno-sanitize-recover=all',str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
    rows = [json.loads(line) for line in subprocess.check_output([str(p/'oracle')],text=True).splitlines()]
(here/'cases.json').write_text(json.dumps({'scope':__doc__, 'retained_sha256':{k:hashlib.sha256(v.encode()).hexdigest() for k,v in bodies.items()}, 'cached':list(zip(['ship_fire.xps','ship_smoke.xps','smoke.ini.xps','smoke..xps','.xps.xps','effects/ship.fire.xps','effects/ship_fire.xps'], range(1,8))), 'cases':[dict(name=name,**row) for name,row in zip(queries,rows)]},indent=2)+'\n')
print(f'{len(rows)} native profile/cache lookup and system/capture snapshots under UBSan')
