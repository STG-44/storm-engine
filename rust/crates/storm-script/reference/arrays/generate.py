"""Exercise unedited retained DATA array methods under UBSan.

Scalar-only storage scaffolding replaces the rest of DATA, error reporting and
attributes/compiler implementations. No C++ interpreter or game program runs.
The script wrapper rejects zero resize sizes; all resizing here is positive.
"""
from pathlib import Path
import hashlib
import json
import random
import subprocess
import tempfile

here = Path(__file__).resolve().parent
repo = next(p for p in here.parents if (p/'src/libs/core/src/data.cpp').is_file())
original = (repo/'src/libs/core/src/data.cpp').read_text()
tokens = (repo/'src/libs/core/src/token.h').read_text()

def extract(source, signature):
    start = source.index(signature)
    p = source.index('{', start)
    depth = 1
    p += 1
    while depth:
        if source[p] == '{': depth += 1
        if source[p] == '}': depth -= 1
        p += 1
    return source[start:p]

methods = ['DATA::DATA()', 'DATA::DATA(S_TOKEN_TYPE _element_type)',
           'void DATA::ClearType()', 'void DATA::SetType(S_TOKEN_TYPE _element_type, uint32_t array_size)',
           'void DATA::SetElementsNum(uint32_t _asize)', 'uint32_t DATA::GetElementsNum()',
           'DATA *DATA::GetArrayElement(uint32_t index)', 'DATA *DATA::GetVarPointer()',
           'void DATA::Set(int32_t value)', 'void DATA::Set(float value)',
           'void DATA::Set(std::string value)', 'void DATA::Set(const char *value)',
           'void DATA::SetVCompiler(VIRTUAL_COMPILER *pVC)']
# Constructor initializers are retained as well as their bodies.
code = '\n\n'.join(extract(original,m) for m in methods)
source = r'''
#include <cmath>
#include <cstdint>
#include <cstring>
#include <iomanip>
#include <iostream>
#include <stdexcept>
#include <string>
#include <vector>
using std::isnan;
using entid_t=uint64_t;
struct ATTRIBUTES {};
struct VIRTUAL_COMPILER {};
#define UNINIT_REF "uninitialized ref"
#define NO_INDEX "missing index"
#define INDEX_ON_SINGLE "index on scalar"
'''+extract(tokens, 'enum S_TOKEN_TYPE')+';\n'+r'''
struct DATA {
    VIRTUAL_COMPILER* pVCompiler;
    S_TOKEN_TYPE Data_type;
    uint32_t Number_of_elements;
    bool bArray,bEntity;
    int32_t lValue;
    uintptr_t pValue;
    float fValue;
    std::string sValue;
    entid_t object_id;
    uint32_t Segment_id,nGlobalVarTableIndex;
    DATA* pReference;
    ATTRIBUTES* AttributesClass;
    std::vector<DATA> ArrayPTR;
    DATA();DATA(S_TOKEN_TYPE);
    void ClearType();void SetType(S_TOKEN_TYPE,uint32_t=1);
    void SetElementsNum(uint32_t);uint32_t GetElementsNum();
    DATA* GetArrayElement(uint32_t);DATA* GetVarPointer();
    void Set(int32_t);void Set(float);void Set(std::string);void Set(const char*);
    void SetVCompiler(VIRTUAL_COMPILER*);
    static void Error(const char* message){throw std::runtime_error(message);}
    static void BadIndex(uint32_t,uint32_t){throw std::runtime_error("bad index");}
};
'''+code+r'''
void snapshot(DATA& value) {
    DATA ref(VAR_REFERENCE);ref.pReference=&value;
    std::cout<<"{\"array\":"<<(value.bArray?"true":"false")<<",\"count\":"<<ref.GetElementsNum()<<",\"values\":[";
    for(uint32_t i=0;i<value.GetElementsNum();i++){
        if(i)std::cout<<',';
        DATA& v=value.bArray?*ref.GetVarPointer()->GetArrayElement(i):value;
        if(v.Data_type==VAR_INTEGER)std::cout<<"[\"i\","<<v.lValue<<']';
        else if(v.Data_type==VAR_FLOAT){uint32_t bits;std::memcpy(&bits,&v.fValue,4);std::cout<<"[\"f\","<<bits<<']';}
        else std::cout<<"[\"s\","<<std::quoted(v.sValue)<<']';
    }
    std::cout<<"]}\n";
}
int main(){
'''
rng = random.Random(441019)
sequences = []
for kind, token in [('i','VAR_INTEGER'),('f','VAR_FLOAT'),('s','VAR_STRING')]:
    for size in [0,1,2,7]:
        steps = [['init',size]]
        current = max(1,size)
        for n in range(36):
            if size>1 and n%3==0:
                current=rng.choice([1,2,3,8,16])
                steps.append(['resize',current])
            else:
                index=rng.randrange(current)
                value=rng.randint(-1000,1000) if kind=='i' else rng.randint(-1000,1000)/4 if kind=='f' else 'sail-'+str(n)
                steps.append(['write',index,value])
        sequences.append({'kind':kind,'steps':steps})
        source += '{ DATA value;DATA ref(VAR_REFERENCE);ref.pReference=&value;\n'
        for step in steps:
            if step[0]=='init': source+=f'value.SetType({token},{step[1]});\n'
            elif step[0]=='resize': source+=f'ref.SetElementsNum({step[1]});\n'
            else:
                _,index,value=step
                arg=f'static_cast<int32_t>({value})' if kind=='i' else f'static_cast<float>({value})' if kind=='f' else json.dumps(value)
                target=f'*ref.GetVarPointer()->GetArrayElement({index})' if size>1 else 'value'
                source+=f'({target}).Set({arg});\n'
            source+='snapshot(value);\n'
        source+='}\n'
source+='}\n'
with tempfile.TemporaryDirectory(prefix='storm-array-reference-') as directory:
    root=Path(directory)
    (root/'oracle.cpp').write_text(source)
    subprocess.run(['clang++','-std=c++20','-O0','-fsanitize=undefined','-fno-sanitize-recover=all',str(root/'oracle.cpp'),'-o',str(root/'oracle')],check=True)
    snapshots=iter(subprocess.check_output([str(root/'oracle')],text=True).splitlines())
    for sequence in sequences:
        sequence['snapshots']=[json.loads(next(snapshots)) for _ in sequence['steps']]
    assert next(snapshots,None) is None
fixture={'source':'retained src/libs/core/src/data.cpp array methods','methods':methods,
         'methods_sha256':hashlib.sha256(code.encode()).hexdigest(),'sequences':sequences}
(here/'cases.json').write_text(json.dumps(fixture,separators=(',',':'))+'\n')
print('Generated',len(sequences),'array sequences and',sum(len(s['steps']) for s in sequences),'snapshots')
