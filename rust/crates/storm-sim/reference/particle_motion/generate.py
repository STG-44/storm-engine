"""Unedited billboard Process, physics helpers and vector math under UBSan.

Graph curves, particle construction/storage/freeing and unattached emitter
interfaces are adapters. This does not execute native AddParticle, global RNG,
attached births, model processing or renderer/manager dispatch.
"""
from pathlib import Path
import hashlib,json,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/particles').is_dir())
processor=repo/'src/libs/particles/src/system/particle_processor'
def method(source,sig):
 start=source.index(sig);return source[start:source.index('\n}',start)+2]
bb=(processor/'bb_processor.cpp').read_text();physics=(processor/'physic.h').read_text()
matrix=(repo/'src/libs/math/include/math3d/matrix.h').read_text()
bodies={'void BillBoardProcessor::Process(':method(bb,'void BillBoardProcessor::Process('),
 'inline void SolvePhysic(':method(physics,'inline void SolvePhysic('),
 'inline void AddGravityForce(':method(physics,'inline void AddGravityForce('),
 'inline Vector Matrix::MulVertex(':method(matrix,'inline Vector Matrix::MulVertex(')}
cases=[{'mass':m,'drag':d,'gravity':g,'blend':b} for m in [0.0,0.3,2.0,-2.0] for d in [-25.0,0.0,100.0,175.0] for g in [-50.0,30.0,150.0] for b in [0.0,12.0]]
deltas=[0.0,0.125,0.25,-0.125,0.5,1.25]
source=r'''
#include <bit>
#include <cstdint>
#include <iostream>
#include <vector>
#include "math3d/vector.h"
struct Matrix {float m[4][4]{}; Matrix(){for(int i=0;i<4;i++)m[i][i]=1;} Matrix(float,Vector):Matrix(){} Vector MulVertex(const Vector&)const;};
Vector operator*(const Vector& v,const Matrix& m){return m.MulVertex(v);}
__MATRIX__
__PHYSICS__
uint32_t bits(float f){return std::bit_cast<uint32_t>(f);}
std::vector<std::vector<uint32_t>> calls;
std::vector<uint32_t> freed;
struct DataGraph{uint32_t id,field;float base,slope;float GetValue(float time,float life,float k){float value=(base+slope*time)+k*0.25f;calls.push_back({id,field,bits(time),bits(life),bits(k),bits(value)});return value;}};
struct IEmitter{void Teleport(Matrix){}void SetTransform(Matrix){}void BornParticles(float){}};
struct BB_ParticleData {
 uint32_t id,*ActiveCount; float ElapsedTime,LifeTime,Mass,UMass,Spin,Angle=0,RenderAngle=0,OldRenderAngle=0;
 Vector PhysPos,Velocity,ExternalForce,RenderPos,OldRenderPos;Matrix matWorld;
 DataGraph *Graph_Drag,*graph_GravK,*Graph_SpinDrag,*Graph_TrackX,*Graph_TrackY,*Graph_TrackZ,*Graph_PhysBlend;
 float DragK,GravKK,SpinDragK,KTrackX,KTrackY,KTrackZ,KPhysBlend;
 IEmitter* AttachedEmitter=nullptr;
};
struct BillBoardProcessor{std::vector<BB_ParticleData*> Particles;void FreeParticle(BB_ParticleData* p){freed.push_back(p->id);}void Process(float);};
__PROCESS__
void list(const std::vector<uint32_t>& row){std::cout<<"[";bool first=true;for(auto v:row){if(!first)std::cout<<",";first=false;std::cout<<v;}std::cout<<"]";}
void snapshot(const BillBoardProcessor& p,uint32_t active){std::cout<<"{\"active\":"<<active<<",\"bodies\":[";bool first=true;for(auto* a:p.Particles){if(!first)std::cout<<",";first=false;
 std::vector<uint32_t> row{a->id,bits(a->ElapsedTime),bits(a->LifeTime),bits(a->Mass),bits(a->Spin),bits(a->Angle),bits(a->RenderAngle)};
 for(auto v:{a->PhysPos,a->Velocity,a->ExternalForce,a->RenderPos,a->OldRenderPos})for(float f:{v.x,v.y,v.z})row.push_back(bits(f));list(row);}
 std::cout<<"],\"calls\":[";first=true;for(auto& row:calls){if(!first)std::cout<<",";first=false;list(row);}std::cout<<"],\"freed\":";list(freed);std::cout<<"}";calls.clear();freed.clear();}
void run(float mass,float drag,float gravity,float blend){
 BillBoardProcessor p;BB_ParticleData bodies[3];DataGraph graphs[3][7];uint32_t active=3;
 for(uint32_t i=0;i<3;i++){
  auto& a=bodies[i];a.id=i;a.ActiveCount=&active;a.ElapsedTime=0;a.LifeTime=i==0?0.125f:(i==1?0.75f:5.0f);a.Mass=mass;a.UMass=fabsf(mass);a.Spin=1.25f*(i+1);
  a.PhysPos=Vector(0.5f,-2.0f,4.0f);a.RenderPos=a.OldRenderPos=a.PhysPos;a.Velocity=Vector(1.25f,-0.5f,2.0f);a.ExternalForce=Vector(0.0f);
  a.matWorld.m[3][0]=float(i);a.matWorld.m[3][1]=-float(i)*0.5f;a.matWorld.m[3][2]=0.25f;
  if(i==2){a.matWorld.m[0][0]=0;a.matWorld.m[0][2]=-2;a.matWorld.m[2][0]=0.5f;a.matWorld.m[2][2]=0;}
  float bases[]={drag,gravity,float(i)*60.0f,-0.5f,1.5f,2.0f,blend};float slopes[]={10.0f,-2.5f,4.0f,0.25f,-0.5f,1.0f,0.125f};
  for(uint32_t j=0;j<7;j++)graphs[i][j]={i,j,bases[j],slopes[j]};
  a.Graph_Drag=&graphs[i][0];a.graph_GravK=&graphs[i][1];a.Graph_SpinDrag=&graphs[i][2];a.Graph_TrackX=&graphs[i][3];a.Graph_TrackY=&graphs[i][4];a.Graph_TrackZ=&graphs[i][5];a.Graph_PhysBlend=&graphs[i][6];
  a.DragK=a.GravKK=a.SpinDragK=a.KTrackX=a.KTrackY=a.KTrackZ=a.KPhysBlend=0.25f+0.125f*float(i);p.Particles.push_back(&a);
 }
 calls.clear();freed.clear();std::cout<<"[";snapshot(p,active);uint32_t frame=0;
 for(float delta:{0.0f,0.125f,0.25f,-0.125f,0.5f,1.25f}){for(auto* a:p.Particles)a->ExternalForce=Vector(float(frame)*0.125f,-0.75f,0.25f);p.Process(delta);std::cout<<",";snapshot(p,active);frame++;}std::cout<<"]\n";
}
int main(){__RUNS__}
'''
source=source.replace('__MATRIX__',bodies['inline Vector Matrix::MulVertex(']).replace('__PHYSICS__','\n'.join(bodies[k] for k in ['inline void SolvePhysic(','inline void AddGravityForce('])).replace('__PROCESS__',bodies['void BillBoardProcessor::Process(']).replace('__RUNS__','\n'.join('run('+','.join(str(c[k])+'f' for k in ['mass','drag','gravity','blend'])+');' for c in cases))
with tempfile.TemporaryDirectory(prefix='storm-motion-reference-') as folder:
 p=Path(folder);(p/'main.cpp').write_text(source)
 subprocess.run(['clang++','-std=c++20','-ffp-contract=off','-fsanitize=undefined','-fno-sanitize-recover=all','-I',str(repo/'src/libs/math/include'),str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
 rows=subprocess.check_output([str(p/'oracle')],text=True).splitlines()
assert len(rows)==len(cases)
for c,row in zip(cases,rows):c['expected']=json.loads(row)
hashes={k:hashlib.sha256(v.encode()).hexdigest() for k,v in bodies.items()}
hashes['math3d/vector.h']=hashlib.sha256((repo/'src/libs/math/include/math3d/vector.h').read_bytes()).hexdigest()
(here/'cases.json').write_text(json.dumps({'scope':__doc__,'retained_sha256':hashes,'deltas':deltas,'cases':cases},separators=(',',':'))+'\n')
print(f'{len(cases)} sequences / {len(cases)*(len(deltas)+1)} complete pool/motion/graph-call snapshots under UBSan')
