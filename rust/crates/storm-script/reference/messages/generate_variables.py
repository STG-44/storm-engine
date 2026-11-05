"""Exercise e bindings and native DATA setters with unedited retained bodies.

The stack, MESSAGE storage, lookup and native diagnostic are adapters. By-value
stack DATA is explicitly prepared; original compiler/object copying is outside
this oracle. Scalar getters/setters, entity IDs, GetVarPointer, CreateMessage
and the live SendMessage case are compiled verbatim under UBSan.
"""
from pathlib import Path
import hashlib
import json
import subprocess
import tempfile

here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/core/src/data.cpp').is_file())
data=(repo/'src/libs/core/src/data.cpp').read_text()
internal=(repo/'src/libs/core/src/internal_functions.cpp').read_text()
def method(source,signature):
    start=source.index(signature)
    return source[start:source.index('\n}',start)+2]
bodies={s:method(data,s) for s in [
    'DATA *DATA::GetVarPointer()', 'void DATA::Set(int32_t value)',
    'void DATA::Set(float value)', 'void DATA::Set(std::string value)',
    'void DATA::SetPtr(uintptr_t value)', 'void DATA::Set(entid_t eid)',
    'bool DATA::Get(int32_t &value)', 'bool DATA::Get(float &value)',
    'bool DATA::Get(const char *&value)', 'bool DATA::GetPtr(uintptr_t &value)',
    'void DATA::Get(entid_t &eid)']}
bodies['CreateMessage']=method(internal,'bool COMPILER::CreateMessage(')
start=internal.rindex('    case FUNC_SEND_MESSAGE:')
bodies['SendMessage']=internal[start:internal.index('    case FUNC_',start+10)]
source=r'''
#include <bit>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <stdexcept>
#include <string>
#include <vector>
using std::isnan;
using entid_t=uint64_t;
enum {VAR_INTEGER,VAR_FLOAT,VAR_STRING,VAR_PTR,VAR_OBJECT,VAR_AREFERENCE,VAR_REFERENCE};
#define UNINIT_REF "uninitialized ref"
#define NO_INDEX "array requires index"
#define INVALID_FA "invalid argument"
struct ATTRIBUTES {};
struct DATA {
 int Data_type=VAR_INTEGER;bool bArray=false,bEntity=false;int32_t lValue=0;float fValue=0;
 std::string sValue;uintptr_t pValue=0;entid_t object_id=0;DATA* pReference=nullptr;
 ATTRIBUTES* AttributesClass=nullptr;
 void Error(const char* s){throw std::runtime_error(s);}
 int GetType(){return Data_type;}
 DATA* GetVarPointer();
 bool Get(int32_t&);bool Get(float&);bool Get(const char*&);bool GetPtr(uintptr_t&);void Get(entid_t&);
 void Set(int32_t);void Set(float);void Set(std::string);void SetPtr(uintptr_t);void Set(entid_t);
 ATTRIBUTES* GetAClass(){return GetVarPointer()->AttributesClass;}
 bool Convert(int type){if(Data_type==VAR_INTEGER&&type==VAR_FLOAT){Set(static_cast<float>(lValue));return true;}return false;}
};
__DATA__
std::string snapshot(DATA* d){
 d=d->GetVarPointer();int32_t n=0;float f=0;uintptr_t p=0;entid_t id=0;const char* s=nullptr;
 switch(d->GetType()){
 case VAR_INTEGER:d->Get(n);return "i:"+std::to_string(n);
 case VAR_FLOAT:d->Get(f);return "f:"+std::to_string(std::bit_cast<uint32_t>(f));
 case VAR_STRING:d->Get(s);return "s:"+std::string(s);
 case VAR_PTR:d->GetPtr(p);return "p:"+std::to_string(p);
 case VAR_OBJECT:d->Get(id);return "o:"+std::to_string(id);
 case VAR_AREFERENCE:d->Get(id);return "a:"+std::to_string(id);
 default:throw std::runtime_error("unexpected DATA type");
 }
}
void initial(DATA& d,char kind){
 if(kind=='i')d.Set(int32_t(7));
 if(kind=='f')d.Set(7.25f);
 if(kind=='s')d.Set(std::string("before"));
 if(kind=='p')d.SetPtr(18364758544493064720ULL);
 if(kind=='o')d.Set(entid_t(9920249030594524246ULL));
 if(kind=='a'){d.Data_type=VAR_AREFERENCE;d.Set(entid_t(9920249030594524246ULL));}
}
void write(DATA* d,char kind){
 if(kind=='i')d->Set(int32_t(-19));
 if(kind=='f')d->Set(3.25f);
 if(kind=='s')d->Set(std::string("after"));
 if(kind=='p')d->SetPtr(18446744073709551615ULL);
 if(kind=='o')d->Set(entid_t(18364758544493064720ULL));
}
struct MESSAGE {
 std::string format;std::vector<DATA*> cells;size_t cursor=0;
 void Reset(const char* s){format=s;cells.assign(format.size(),nullptr);cursor=0;}
 void Move2Start(){cursor=0;}
 void Set(DATA* d){cells.at(cursor++)=d;}
 DATA* ScriptVariablePointer(){return cells.at(cursor++);}
 void Set(int32_t){throw std::runtime_error("non-e field");}
 void Set(uintptr_t){throw std::runtime_error("non-e field");}
 void Set(float){throw std::runtime_error("non-e field");}
 void Set(const char*){throw std::runtime_error("non-e field");}
 void SetEntity(entid_t){throw std::runtime_error("non-e field");}
 void Set(ATTRIBUTES*){throw std::runtime_error("non-e field");}
};
struct Stack {
 std::vector<DATA*> args;DATA output;
 uint32_t GetDataNum(){return args.size();}
 DATA* Read(uint32_t a,uint32_t b){return args.at(a+b);}
 DATA* Pop(){return nullptr;}DATA* Push(){return &output;}
};
struct Probe {
 char operation='i';int calls=0;bool same=false;
 std::string before="-",first="-",second="-";
 uint64_t ProcessMessage(MESSAGE& m){
  ++calls;auto* a=m.ScriptVariablePointer();auto* b=m.ScriptVariablePointer();
  same=a==b;before=snapshot(a);write(a,operation);first=snapshot(a);second=snapshot(b);
  return 18364758544493064720ULL;
 }
};
struct Core {bool valid=true;Probe probe;Probe* GetEntityPointerSafe(entid_t){return valid?&probe:nullptr;}};
struct COMPILER {
 Stack SStack;Core core;
 void SetError(const char* s){throw std::runtime_error(s);}
 bool CreateMessage(MESSAGE*,uint32_t,uint32_t,bool=false);
 DATA* invoke(uint32_t arguments){
  uint32_t s_off=0,n=0;DATA *pV=nullptr,*pVResult=nullptr;Probe* pE=nullptr;entid_t ent=0;MESSAGE ms;
  constexpr int FUNC_SEND_MESSAGE=1;
  switch(FUNC_SEND_MESSAGE){__SEND__}return nullptr;
 }
};
__CREATE__
void run(char kind,char operation,bool ref,bool valid){
 COMPILER c;c.core.valid=valid;c.core.probe.operation=operation;
 DATA caller,destination,format,alias,a,b;initial(caller,kind);
 destination.Set(entid_t(4294967296ULL));format.Set(std::string("ee"));
 alias.Data_type=VAR_REFERENCE;alias.pReference=&caller;
 if(ref){a.Data_type=b.Data_type=VAR_REFERENCE;a.pReference=b.pReference=&alias;}
 else {a=caller;b=caller;}
 c.SStack.args={&destination,&format,&a,&b};auto* result=c.invoke(4);auto& p=c.core.probe;
 std::cout<<result->pValue<<" "<<p.calls<<" "<<p.same<<" "<<p.before<<" "<<p.first<<" "<<p.second<<" "<<snapshot(&caller)<<"\n";
}
int main(){
__CASES__
}
'''
cases=[{'initial':kind,'write':write,'ref':ref,'valid':valid}
       for kind in 'ifs poa'.replace(' ','') for write in 'ifspo'
       for ref in [False,True] for valid in [False,True]]
source=source.replace('__DATA__','\n'.join(v for k,v in bodies.items() if k not in ['CreateMessage','SendMessage']))
source=source.replace('__SEND__',bodies['SendMessage']).replace('__CREATE__',bodies['CreateMessage'])
source=source.replace('__CASES__','\n'.join(f"run('{c['initial']}','{c['write']}',{str(c['ref']).lower()},{str(c['valid']).lower()});" for c in cases))
with tempfile.TemporaryDirectory(prefix='storm-variable-reference-') as folder:
    path=Path(folder);(path/'main.cpp').write_text(source)
    subprocess.run(['clang++','-std=c++20','-fsanitize=undefined','-fno-sanitize-recover=all',str(path/'main.cpp'),'-o',str(path/'oracle')],check=True)
    rows=subprocess.run([str(path/'oracle')],capture_output=True,text=True,check=True,timeout=20).stdout.splitlines()
assert len(rows)==len(cases)
for case,row in zip(cases,rows):
    result,calls,same,before,first,second,caller=row.split()
    case['expected']={'result':int(result),'calls':int(calls),'same':bool(int(same)),
                      'before':before,'first':first,'second':second,'caller':caller}
(here/'variable_cases.json').write_text(json.dumps({'scope':__doc__,'retained_sha256':{k:hashlib.sha256(v.encode()).hexdigest() for k,v in bodies.items()},'cases':cases},indent=2)+'\n')
print(f'{len(cases)} retained native e binding/setter cases under UBSan')
