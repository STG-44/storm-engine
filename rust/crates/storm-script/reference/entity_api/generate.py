"""Compile retained entity native cases, DATA getters and MOVE notification gate.

Stack/storage and core dispatch targets are instrumented adapters. This checks
binding types, complete IDs, calls/results and the notification switch, not the
original entity manager, complete interpreter, class factories or gameplay.
"""
from pathlib import Path
import hashlib
import json
import random
import subprocess
import tempfile

here = Path(__file__).resolve().parent
repo = next(p for p in here.parents if (p/'src/libs/core/src/data.cpp').is_file())
core = repo/'src/libs/core/src'
data = (core/'data.cpp').read_text()
internal = (core/'internal_functions.cpp').read_text()
compiler = (core/'compiler.cpp').read_text()

def method(source, signature):
    start = source.index(signature)
    return source[start:source.index('\n}', start)+2]

bodies = {s: method(data, s) for s in [
    'bool DATA::Get(int32_t &value)', 'void DATA::Get(entid_t &eid)',
    'void DATA::Set(int32_t value)', 'DATA *DATA::GetVarPointer()']}
for name, end in [('FUNC_IS_Entity_LOADED', 'FUNC_Entity_UPDATE'),
                  ('FUNC_Entity_UPDATE', 'FUNC_FRAND'),
                  ('FUNC_DELETE_Entity', 'FUNC_DEL_EVENT_HANDLER')]:
    start = internal.index('    case '+name+':')
    finish = internal.index('    case '+end+':', start)
    bodies[name] = internal[start:finish]
# The earlier match is inside a disabled/commented VM branch. Extract the live
# MOVE branch, whose notification takes an entity ID by value.
start = compiler.rindex('                pLeftOperandAClass->SetValue(pChar);')
finish = compiler.index('\n                break;', start)
bodies['MOVE notification'] = compiler[start:finish]
assert 'core_internal.Entity_AttributeChanged(eid, pLeftOperandAClass);' in bodies['MOVE notification']

source = r'''
#include <cstdint>
#include <cstring>
#include <iostream>
#include <set>
#include <stdexcept>
#include <string>
#include <vector>
using entid_t=uint64_t;
enum {VAR_INTEGER,VAR_OBJECT,VAR_AREFERENCE,VAR_REFERENCE};
#define UNINIT_REF "uninitialized ref"
#define NO_INDEX "index required"
#define INVALID_FA "invalid argument"
struct ATTRIBUTES {void SetValue(const char*){}};
struct DATA {
 int Data_type=VAR_INTEGER;bool bArray=false;int32_t lValue=0;
 entid_t object_id=0;DATA* pReference=nullptr;
 void Error(const char* s){throw std::runtime_error(s);}
 bool Get(int32_t&);void Get(entid_t&);void Set(int32_t);DATA* GetVarPointer();
 DATA* GetArrayElement(uint32_t){throw std::runtime_error("unused indexed MOVE");}
};
__DATA__
struct Stack {DATA* argument;DATA output;DATA* Pop(){return argument;}DATA* Push(){return &output;}};
struct Core {
 std::set<entid_t> valid{4294967296ULL,9920249030594524246ULL};
 std::vector<std::pair<char,entid_t>> calls;
 bool IsEntityValid(entid_t id){calls.emplace_back('i',id);return valid.contains(id);}
 void EraseEntity(entid_t id){calls.emplace_back('d',id);valid.erase(id);}
 void Entity_AttributeChanged(entid_t id,ATTRIBUTES*){calls.emplace_back('w',id);}
};
int main(){
 Core core;auto& core_internal=core;bool bEntityUpdate=true;
 constexpr int FUNC_IS_Entity_LOADED=0,FUNC_Entity_UPDATE=1,FUNC_DELETE_Entity=2;
 char op;int shape;uint64_t value;
 while(std::cin>>op>>shape>>value){
  DATA object,ref,ref2;object.Data_type=op=='u'?VAR_INTEGER:(shape==1||shape==3?VAR_AREFERENCE:VAR_OBJECT);
  object.object_id=value;object.lValue=static_cast<int32_t>(static_cast<uint32_t>(value));
  ref.Data_type=VAR_REFERENCE;ref.pReference=&object;ref2.Data_type=VAR_REFERENCE;ref2.pReference=&ref;
  DATA* target=shape>=2?&ref2:&object;Stack SStack{target};DATA* pV=nullptr;DATA* pVResult=nullptr;
  int32_t TempLong1=0;entid_t TempEid=0,ent=0;
  auto SetError=[](const char* s){throw std::runtime_error(s);};
  int function=op=='i'?0:op=='u'?1:2;
  auto native=[&]()->DATA*{switch(function){
__NATIVE__
  }return nullptr;};
  DATA* result=nullptr;
  if(op=='w'){
   ATTRIBUTES attr;auto* pLeftOperandAClass=&attr;auto* pVDst=target;
   const char* pChar="19";constexpr uint32_t INVALID_ARRAY_INDEX=0xffffffff;
   uint32_t nLeftOperandIndex=INVALID_ARRAY_INDEX;entid_t eid=0;
   auto write=[&]()->bool{__MOVE__ return true;};write();
  }else result=native();
  if(result)std::cout<<result->lValue;else std::cout<<"null";
  std::cout<<"|[";bool first=true;
  for(auto [kind,id]:core.calls){if(!first)std::cout<<',';first=false;std::cout<<"[\""<<kind<<"\","<<id<<"]";}
  std::cout<<"]\n";
 }
}
'''.replace('__DATA__', '\n'.join(bodies[s] for s in list(bodies)[:4]))
source = source.replace('__NATIVE__', '\n'.join(bodies[n] for n in ['FUNC_IS_Entity_LOADED','FUNC_Entity_UPDATE','FUNC_DELETE_Entity']))
source = source.replace('__MOVE__', bodies['MOVE notification'])
ids = [0,4294967296,8589934593,9920249030594524246,18446744073709551615]
sequences = [[['i',s,n],['d',s,n],['i',s,n],['w',s,n]] for s in range(4) for n in ids]
rng = random.Random(4410192)
for _ in range(12):
    sequence = []
    for _ in range(40):
        op = rng.choice(['i','d','u','w'])
        value = rng.choice([0,1,7,0xffffffff,0x80000000]) if op=='u' else rng.choice(ids)
        sequence.append([op,rng.randrange(4),value])
    sequences.append(sequence)
with tempfile.TemporaryDirectory(prefix='storm-entity-api-') as directory:
    root=Path(directory)
    (root/'oracle.cpp').write_text(source)
    subprocess.run(['clang++','-std=c++20','-O0','-fsanitize=undefined','-fno-sanitize-recover=all',str(root/'oracle.cpp'),'-o',str(root/'oracle')],check=True)
    cases=[]
    for steps in sequences:
        run=subprocess.run([str(root/'oracle')],input='\n'.join(' '.join(map(str,s)) for s in steps)+'\n',text=True,capture_output=True,timeout=5,check=True)
        snapshots=[]
        for line in run.stdout.splitlines():
            result,calls=line.split('|');snapshots.append({'result':json.loads(result),'calls':json.loads(calls)})
        assert len(snapshots)==len(steps)
        cases.append({'operations':steps,'snapshots':snapshots})
fixture={'scope':__doc__,'bodies_sha256':{n:hashlib.sha256(s.encode()).hexdigest() for n,s in bodies.items()},'cases':cases}
(here/'cases.json').write_text(json.dumps(fixture,separators=(',',':'))+'\n')
print('Generated',len(cases),'entity-binding sequences,',sum(len(c['snapshots']) for c in cases),'snapshots under UBSan')
