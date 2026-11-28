"""Run unedited CreateMessage/SendMessage and DATA ptr accessors under UBSan.

DATA scalar getters/conversion, stack, MESSAGE storage and entity lookup are
instrumented adapters. This is a binding oracle, not a complete interpreter or
native class simulation. Error paths use throwing SetError to stop immediately.
"""
from pathlib import Path
import hashlib
import json
import random
import subprocess
import tempfile

here = Path(__file__).resolve().parent
repo = next(p for p in here.parents if (p/'src/libs/core/src/data.cpp').is_file())
internal = (repo/'src/libs/core/src/internal_functions.cpp').read_text()
data = (repo/'src/libs/core/src/data.cpp').read_text()

def method(source, signature):
    start = source.index(signature)
    return source[start:source.index('\n}', start)+2]

bodies = {signature: method(data, signature) for signature in [
    'void DATA::SetPtr(uintptr_t value)', 'bool DATA::GetPtr(uintptr_t &value)']}
bodies['CreateMessage'] = method(internal, 'bool COMPILER::CreateMessage(')
start = internal.rindex('    case FUNC_SEND_MESSAGE:')
bodies['SendMessage'] = internal[start:internal.index('    case FUNC_', start+10)]
assert 'pV->SetPtr(mresult)' in bodies['SendMessage']

source = r'''
#include <bit>
#include <cstdint>
#include <iostream>
#include <stdexcept>
#include <string>
#include <vector>
using entid_t=uint64_t;
enum {VAR_INTEGER,VAR_FLOAT,VAR_STRING,VAR_PTR,VAR_OBJECT,VAR_AREFERENCE,VAR_REFERENCE};
#define UNINIT_REF "uninitialized ref"
#define NO_INDEX "array requires index"
#define INVALID_FA "invalid argument"
struct ATTRIBUTES {};
struct DATA {
 int Data_type=VAR_INTEGER;bool bArray=false;int32_t number=0;float real=0;
 std::string text;uintptr_t pValue=0;entid_t id=0;DATA* pReference=nullptr;
 ATTRIBUTES* attributes=nullptr;
 void Error(const char* s){throw std::runtime_error(s);}
 int GetType(){return Data_type;}
 DATA* GetVarPointer(){return Data_type==VAR_REFERENCE?pReference->GetVarPointer():this;}
 bool Get(int32_t& n){auto* p=GetVarPointer();n=p->number;return p->Data_type==VAR_INTEGER;}
 bool Get(float& n){auto* p=GetVarPointer();n=p->real;return p->Data_type==VAR_FLOAT;}
 bool Get(const char*& s){auto* p=GetVarPointer();s=p->text.c_str();return p->Data_type==VAR_STRING;}
 void Get(entid_t& n){auto* p=GetVarPointer();n=p->id;}
 ATTRIBUTES* GetAClass(){return GetVarPointer()->attributes;}
 bool Convert(int type){if(Data_type==VAR_INTEGER&&type==VAR_FLOAT){real=static_cast<float>(number);Data_type=type;return true;}return false;}
 void SetPtr(uintptr_t);bool GetPtr(uintptr_t&);
};
__PTR__
struct MESSAGE {
 std::string format;std::vector<std::string> fields;bool rewound=false;
 void Reset(const char* s){format=s;fields.clear();rewound=false;}
 void Move2Start(){rewound=true;}
 void Set(int32_t n){fields.push_back("l:"+std::to_string(n));}
 void Set(uintptr_t n){fields.push_back("p:"+std::to_string(n));}
 void Set(float n){fields.push_back("f:"+std::to_string(std::bit_cast<uint32_t>(n)));}
 void Set(const char* n){fields.push_back("s:"+std::string(n));}
 void SetEntity(entid_t n){fields.push_back("i:"+std::to_string(n));}
 void Set(ATTRIBUTES* n){fields.push_back(n?"a:91":"a:0");}
 void Set(DATA*){throw std::runtime_error("e excluded");}
};
struct Stack {
 std::vector<DATA*> args;DATA output;
 uint32_t GetDataNum(){return args.size();}
 DATA* Read(uint32_t start,uint32_t offset){return args.at(start+offset);}
 DATA* Pop(){return nullptr;}DATA* Push(){return &output;}
};
struct Probe {
 uint64_t result=0;int calls=0;MESSAGE last;
 uint64_t ProcessMessage(MESSAGE& m){++calls;last=m;if(!m.rewound)throw std::runtime_error("not rewound");return result;}
};
struct Core {
 bool valid=true;Probe probe;
 Probe* GetEntityPointerSafe(entid_t){return valid?&probe:nullptr;}
};
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
int main(){
__CASES__
}
'''

rng = random.Random(0x53454e44)
cases = []
for index in range(48):
    fmt = ['','l','f','s','p','i','a','lfsiap','fflpai'][index % 9]
    fields = []
    for code in fmt:
        value = {'l':rng.choice([-2147483648,-1,0,19,2147483647]),
                 'f':rng.choice([-16777217,-1,0,7,16777217]), 's':'cloth',
                 'p':rng.choice([0,4294967296,18364758544493064720,18446744073709551615]),
                 'i':9920249030594524246, 'a':91}[code]
        fields.append({'code':code,'value':value,'ref':code!='s' and rng.choice([False,True])})
    cases.append({'valid':index%4!=0,'result':[0,4294967296,18364758544493064720,18446744073709551615][index%4],
                  'format':fmt,'fields':fields,'extra':index%3==0,'format_ref':index%2==0})
cases.append({'valid':True,'result':18446744073709551615,'format':'l'*128,
              'fields':[{'code':'l','value':i-64,'ref':False} for i in range(128)],
              'extra':False,'format_ref':False})
for number in [-0.0, 7.25, -19.5, 16777216.0]:
    cases.append({'valid':True,'result':4294967296,'format':'f',
                  'fields':[{'code':'f','value':number,'ref':True,'float':True}],
                  'extra':False,'format_ref':False})

blocks=[]
for case in cases:
    fmt=case['format']; lines=['{COMPILER c;ATTRIBUTES attr;',
        f'c.core.valid={str(case["valid"]).lower()};c.core.probe.result={case["result"]}ULL;',
        'DATA destination,format,format_ref;destination.Data_type=VAR_OBJECT;destination.id=9920249030594524246ULL;',
        f'format.Data_type=VAR_STRING;format.text={json.dumps(fmt)};',
        'format_ref.Data_type=VAR_REFERENCE;format_ref.pReference=&format;',
        'c.SStack.args={&destination,'+('&format_ref' if case['format_ref'] else '&format')+'};',
        f'std::vector<DATA> values({len(fmt)}),refs({len(fmt)});']
    for i, field in enumerate(case['fields']):
        code=field['code'];v=field['value'];expr=f'values[{i}]'
        statements={'l':f'Data_type=VAR_INTEGER;{expr}.number={v};',
                    'f':f'Data_type=VAR_INTEGER;{expr}.number={v};',
                    's':f'Data_type=VAR_STRING;{expr}.text={json.dumps(v)};',
                    'p':f'Data_type=VAR_PTR;{expr}.pValue={v}ULL;',
                    'i':f'Data_type=VAR_OBJECT;{expr}.id={v}ULL;',
                    'a':f'Data_type=VAR_AREFERENCE;{expr}.attributes=&attr;'}
        if field.get('float'):statements['f']=f'Data_type=VAR_FLOAT;{expr}.real={v}f;'
        lines.append(expr+'.'+statements[code])
        lines.append(f'refs[{i}].Data_type=VAR_REFERENCE;refs[{i}].pReference=&{expr};')
        lines.append(f'c.SStack.args.push_back(&'+(f'refs[{i}]' if field['ref'] else expr)+');')
    if case['extra']:lines.append('c.SStack.args.push_back(&destination);')
    lines += ['auto* result=c.invoke(c.SStack.args.size());',
              'std::cout<<result->pValue<<" "<<c.core.probe.calls<<" "<<c.core.probe.last.rewound;',
              'for(auto& f:c.core.probe.last.fields)std::cout<<" "<<f;',
              'std::cout<<"\\n";}']
    blocks.append('\n'.join(lines))
source=source.replace('__PTR__','\n'.join(bodies[s] for s in bodies if s.startswith(('void DATA','bool DATA'))))
source=source.replace('__SEND__',bodies['SendMessage']).replace('__CREATE__',bodies['CreateMessage']).replace('__CASES__','\n'.join(blocks))
with tempfile.TemporaryDirectory(prefix='storm-message-reference-') as folder:
    path=Path(folder);(path/'main.cpp').write_text(source)
    subprocess.run(['c++','-std=c++20','-fsanitize=undefined','-fno-sanitize-recover=all',str(path/'main.cpp'),'-o',str(path/'oracle')],check=True)
    rows=subprocess.run([str(path/'oracle')],capture_output=True,text=True,check=True,timeout=20).stdout.splitlines()
assert len(rows)==len(cases)
for case,row in zip(cases,rows):
    parts=row.split();case['expected']={'result':int(parts[0]),'calls':int(parts[1]),'rewound':bool(int(parts[2])),'fields':parts[3:]}
result={'scope':__doc__,'retained_sha256':{k:hashlib.sha256(v.encode()).hexdigest() for k,v in bodies.items()},'cases':cases}
(here/'cases.json').write_text(json.dumps(result,indent=2)+'\n')
print(f'{len(cases)} retained SendMessage cases under UBSan')
