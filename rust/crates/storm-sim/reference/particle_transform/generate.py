"""Unedited native emitter transform methods and full vector/matrix/quaternion headers.

Constructor/storage, D3D pointer type and profile-position assignment are adapters.
No complete particle manager, ship simulation or global rendering is executed.
"""
from pathlib import Path
import hashlib,json,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/particles').is_dir())
s=(repo/'src/libs/particles/src/system/emitters/base.cpp').read_text()
def method(signature):
 start=s.index(signature);return s[start:s.index('\n}',start)+2]
bodies={sig:method(sig) for sig in ['void BaseEmitter::SetTransform(','void BaseEmitter::Teleport(','void BaseEmitter::BlendMatrix(']}
source=r'''
#include <cstdint>
#include <bit>
#include <iostream>
#include "math3d/quaternion.h"
struct BaseEmitter {bool OldMatrixNotInitialized=true;Matrix matWorldTransformOld,matWorldTransformNew;void SetTransform(const Matrix&);void Teleport(const Matrix&);void BlendMatrix(Matrix&,const Matrix&,const Matrix&,float);};
__METHODS__
uint32_t bits(float f){return std::bit_cast<uint32_t>(f);}
void matrix(const Matrix& m){std::cout<<"[";for(int i=0;i<4;i++){if(i)std::cout<<",";std::cout<<"[";for(int j=0;j<3;j++){if(j)std::cout<<",";std::cout<<bits(m.m[i][j]);}std::cout<<"]";}std::cout<<"]";}
void vector(const Vector& v){std::cout<<"["<<bits(v.x)<<","<<bits(v.y)<<","<<bits(v.z)<<"]";}
int main(){Vector angles[]={{0,0,0},{0,1,0},{0,3.14f,0},{2.8f,0,0},{0,0,-2.9f},{0.5f,-1.1f,0.75f},{-0.75f,0.2f,1.0f}};
 for(int a=0;a<7;a++)for(int b=0;b<7;b++){
  Matrix old(angles[a],Vector(-3.5f,2.0f,1.25f)),next(angles[b],Vector(8.0f,-0.5f,-4.0f));BaseEmitter e;e.SetTransform(old);e.SetTransform(next);
  std::cout<<"{\"old\":";matrix(e.matWorldTransformOld);std::cout<<",\"new\":";matrix(e.matWorldTransformNew);std::cout<<",\"births\":[";
  for(int step=0;step<=32;step++){if(step)std::cout<<",";Matrix m;e.BlendMatrix(m,e.matWorldTransformOld,e.matWorldTransformNew,float(step)/32.0f);m.v.pos=Vector(0.5f,1.25f,-0.75f)*m;std::cout<<"{\"matrix\":";matrix(m);std::cout<<",\"point\":";vector(Vector(-0.25f,0.75f,2.0f)*m);std::cout<<",\"normal\":";vector(m.MulNormal(Vector(1.25f,-0.5f,2.0f)));std::cout<<"}";}
  e.Teleport(old);std::cout<<"],\"teleport_old\":";matrix(e.matWorldTransformOld);std::cout<<",\"teleport_new\":";matrix(e.matWorldTransformNew);BaseEmitter fresh;fresh.Teleport(old);fresh.SetTransform(next);std::cout<<",\"first_set_after_teleport\":";matrix(fresh.matWorldTransformOld);std::cout<<"}\n";
 }
}
'''.replace('__METHODS__','\n'.join(bodies.values()))
with tempfile.TemporaryDirectory(prefix='storm-transform-reference-') as folder:
 p=Path(folder);(p/'d3d9.h').write_text('#pragma once\nstruct D3DMATRIX;\n');(p/'main.cpp').write_text(source)
 subprocess.run(['clang++','-std=c++20','-ffp-contract=off','-fsanitize=undefined','-fno-sanitize-recover=all','-I',str(p),'-I',str(repo/'src/libs/math/include'),str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
 cases=[json.loads(row) for row in subprocess.check_output([str(p/'oracle')],text=True).splitlines()]
hashes={sig:hashlib.sha256(body.encode()).hexdigest() for sig,body in bodies.items()}
for name in ['vector.h','vector4.h','matrix.h','quaternion.h','plane.h']:hashes['math3d/'+name]=hashlib.sha256((repo/'src/libs/math/include/math3d'/name).read_bytes()).hexdigest()
(here/'cases.json').write_text(json.dumps({'scope':__doc__,'retained_sha256':hashes,'cases':cases},separators=(',',':'))+'\n')
print(f'{len(cases)} matrix pairs / {len(cases)*33} complete birth matrix/point/normal snapshots under UBSan')
