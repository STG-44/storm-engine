"""Unedited BaseEmitter birth/time and ParticleSystem pause dispatch under UBSan.

Graph sampling, matrix/position math, processor allocation, object construction
and external particle release are adapters. No full renderer, C RNG or physics.
"""
from pathlib import Path
import json,hashlib,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/particles').is_dir())
base=(repo/'src/libs/particles/src/system/emitters/base.cpp').read_text()
system=(repo/'src/libs/particles/src/system/particle_system/particle_system.cpp').read_text()
def method(source,sig):
 start=source.index(sig);return source[start:source.index('\n}',start)+2]
bodies={sig:method(base,sig) for sig in ['void BaseEmitter::BornParticles(', 'void BaseEmitter::Execute(', 'void BaseEmitter::IncreaseTime(']}
bodies['uint32_t ParticleSystem::Execute(']=method(system,'uint32_t ParticleSystem::Execute(')
cases=[{'looping':loop,'maximum':cap,'slope':slope,'fail_every':fail,'enabled':True,'attached':False,'stopped':False,'type_enabled':True,'elapsed':0.0,'lifetime':1.0,'base':32.0,'remainder':0.0} for loop in [False,True] for cap in [0,2,7] for slope in [0.0,2.0] for fail in [0,2]]
for values in [{'enabled':False},{'attached':True},{'stopped':True},{'type_enabled':False},{'elapsed':2.5},{'lifetime':0.25},{'base':0.0},{'base':1.0},{'base':65.0},{'remainder':0.75},{'remainder':-0.75}]:cases.append(dict(cases[9],**values))
deltas=[0.0,0.125,0.375,0.5,1.25,-0.125]
source=r'''
#include <bit>
#include <cstdint>
#include <vector>
#include <iostream>
#include <stdexcept>
#define INTERPOLATION_STEPS 32.0f
constexpr int BILLBOARD_PARTICLE=1,MODEL_PARTICLE=2;
struct Vector{};Vector operator*(Vector,const struct Matrix&){return {};}
struct Matrix{struct {Vector pos,vy;}v;};
float blend=0;int allocations=0,fail_every=0;
std::vector<std::vector<uint32_t>> rates,births;
uint32_t bits(float x){return std::bit_cast<uint32_t>(x);}
struct DataGraph{uint32_t index;float base,slope;float GetRandomValue(float time,float life){float r=base+slope*time;rates.push_back({index,bits(time),bits(life),bits(r)});return r;}};
struct Processor{uint32_t kind;void AddParticle(void*,Vector,Vector,Matrix,float time,float life,void*,uint32_t* active,uint32_t guid){bool ok=!(fail_every&&++allocations%fail_every==0);if(!fail_every)++allocations;births.push_back({guid,bits(time),bits(life),bits(blend),uint32_t(ok),kind});if(!ok)--*active;}};
struct Manager{Processor bb{1},mdl{2};Processor* GetBBProcessor(){return &bb;}Processor* GetMDLProcessor(){return &mdl;}}manager;
struct BaseEmitter{
 struct Type{bool Visible=true;int Type=1;float Remain=0;uint32_t ActiveCount=0,MaxParticlesCount=0;DataGraph* EmissionRate;void* pFields=nullptr;};
 bool Visible=true,Stoped=false,IsAttachedFlag=false,Looped=false;float LifeTime=1,ElapsedTime=0;uint32_t Unique_GUID=0;Vector Position;void* pMaster=nullptr;Matrix matWorldTransform,matWorldTransformOld,matWorldTransformNew;std::vector<Type> ParticleTypes;
 void BlendMatrix(Matrix&,const Matrix&,const Matrix&,float value){blend=value;}
 Vector GetNewParticlePosition(float){return {};}
 void GetEmissionDirection(Matrix&){}
 Manager* GetManager(){return &manager;}
 void BornParticles(float);void Execute(float);void IncreaseTime(float);
};
struct ParticleSystem{bool AutoDeleted=false,EmissionPause=false;struct Slot{BaseEmitter* pEmitter;};std::vector<Slot> Emitters;bool DeleteIfNeed(){return false;}uint32_t Execute(float);};
__METHODS__
void list(const std::vector<std::vector<uint32_t>>& rows){std::cout<<"[";bool first=true;for(auto& row:rows){if(!first)std::cout<<",";first=false;std::cout<<"[";bool inner=true;for(auto n:row){if(!inner)std::cout<<",";inner=false;std::cout<<n;}std::cout<<"]";}std::cout<<"]";}
void snapshot(BaseEmitter& e,uint32_t count){std::cout<<"{\"elapsed\":"<<bits(e.ElapsedTime)<<",\"stopped\":"<<(e.Stoped?"true":"false")<<",\"count\":"<<count<<",\"types\":[";bool first=true;for(auto& p:e.ParticleTypes){if(!first)std::cout<<",";first=false;std::cout<<"["<<bits(p.Remain)<<","<<p.ActiveCount<<"]";}std::cout<<"],\"rates\":";list(rates);std::cout<<",\"births\":";list(births);std::cout<<"}";rates.clear();births.clear();}
void run(bool loop,uint32_t cap,float slope,int fail,bool enabled,bool attached,bool stopped,bool type_enabled,float elapsed,float life,float rate,float remain){
 allocations=0;fail_every=fail;rates.clear();births.clear();BaseEmitter e;e.Looped=loop;e.Visible=enabled;e.IsAttachedFlag=attached;e.Stoped=stopped;e.ElapsedTime=elapsed;e.LifeTime=life;
 DataGraph a{0,rate,slope},b{1,3.0f,0.0f};e.ParticleTypes={{type_enabled,1,remain,0,cap,&a},{true,2,0,0,cap,&b}};
 ParticleSystem system;system.Emitters.push_back({&e});std::cout<<"[";snapshot(e,1);int index=0;
 for(float delta:{0.0f,0.125f,0.375f,0.5f,1.25f,-0.125f}){system.EmissionPause=index==2;auto count=system.Execute(delta);if(index%2==0)for(auto& p:e.ParticleTypes)if(p.ActiveCount)--p.ActiveCount;std::cout<<",";snapshot(e,count);index++;}
 std::cout<<"]\n";
}
int main(){__RUNS__}
'''
def cpp(v):
 if isinstance(v,bool):return str(v).lower()
 if isinstance(v,float):return str(v)+'f'
 return str(v)
keys=['looping','maximum','slope','fail_every','enabled','attached','stopped','type_enabled','elapsed','lifetime','base','remainder']
source=source.replace('__METHODS__','\n'.join(bodies.values())).replace('__RUNS__','\n'.join('run('+','.join(cpp(c[k]) for k in keys)+');' for c in cases))
with tempfile.TemporaryDirectory(prefix='storm-emitter-reference-') as folder:
 p=Path(folder);(p/'main.cpp').write_text(source)
 subprocess.run(['clang++','-std=c++20','-ffp-contract=off','-fsanitize=undefined','-fno-sanitize-recover=all',str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
 rows=subprocess.check_output([str(p/'oracle')],text=True).splitlines()
assert len(rows)==len(cases)
for c,row in zip(cases,rows):c['expected']=json.loads(row)
(here/'cases.json').write_text(json.dumps({'scope':__doc__,'retained_sha256':{s:hashlib.sha256(body.encode()).hexdigest() for s,body in bodies.items()},'deltas':deltas,'cases':cases},indent=2)+'\n')
print(f'{len(cases)} sequences / {len(cases)*(len(deltas)+1)} complete state/rate/birth snapshots under UBSan')
