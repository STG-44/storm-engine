"""Exercise retained DATA pointer conversions, booleans and comparisons.

Reuse the scalar oracle's extraction/scaffolding prefix without running it or
rewriting its fixture. Requires its pinned fast_float/3.4.0 headers.
"""
from pathlib import Path
import hashlib
import json
import subprocess
import tempfile

output = Path(__file__).resolve().parent
shared = output.parent/'generate.py'
prefix = shared.read_text().split("source += r'''\nvoid run",1)[0]
assert 'source += "\\n\\n".join' in prefix
context = {'__file__':str(shared),'__name__':'pointer_oracle_scaffolding'}
exec(compile(prefix,str(shared),'exec'),context)
source=context['source']+r'''
#include <iostream>
void run(uint64_t bits,bool integer,const char* op,bool reference,int32_t right){
 DATA original,a,b;b.Set(right);
 if(integer)original.Set(static_cast<int32_t>(bits));
 else {original.Data_type=VAR_PTR;original.pValue=bits;}
 if(reference){a.Data_type=VAR_REFERENCE;a.pReference=&original;}else a=original;
 std::string operation(op);bool ok=true;
 if(operation=="int")ok=a.Convert(VAR_INTEGER);
 else if(operation=="ptr")ok=a.Convert(VAR_PTR);
 else if(operation=="bool")ok=a.BoolConvert();
 else if(operation=="!")ok=a.Neg();
 else if(operation=="neg")ok=a.Inverse();
 else {
  S_TOKEN_TYPE token=OP_BOOL_EQUAL;
  if(operation=="!=")token=OP_NOT_EQUAL;
  if(operation=="<")token=OP_LESSER;
  if(operation=="<=")token=OP_LESSER_OR_EQUAL;
  if(operation==">")token=OP_GREATER;
  if(operation==">=")token=OP_GREATER_OR_EQUAL;
  a.CompareAndSetResult(&b,token);
 }
 if(!ok)throw std::runtime_error("unsupported pointer oracle case");
 auto* result=a.GetVarPointer();
 if(result->Data_type==VAR_PTR)std::cout<<"p "<<result->pValue<<"\n";
 else if(result->Data_type==VAR_INTEGER)std::cout<<"i "<<result->lValue<<"\n";
 else throw std::runtime_error("unexpected pointer result type");
}
int main(){
'''
cases=[]
for bits in [0,1,2147483648,4294967296,4294967297,9223372036854775808,18364758544493064720,18446744073709551615]:
    for op in ['int','bool','!','neg','==','!=','<','<=','>','>=']:
        for ref in ([False,True] if op in ['int','bool','neg'] else [False]):
            for right in ([-1,0,19] if op in ['==','!=','<','<=','>','>='] else [0]):
                cases.append({'bits':bits,'integer':False,'op':op,'ref':ref,'right':right})
for number in [-2147483648,-1,0,2147483647]:
    for ref in [False,True]:
        cases.append({'bits':number&0xffffffffffffffff,'integer':True,'op':'ptr','ref':ref,'right':0})
for case in cases:
    source+=f'run({case["bits"]}ULL,{str(case["integer"]).lower()},{json.dumps(case["op"])},{str(case["ref"]).lower()},{case["right"]});\n'
source+='}\n'
with tempfile.TemporaryDirectory(prefix='storm-ptr-reference-') as folder:
    root=Path(folder);(root/'main.cpp').write_text(source)
    subprocess.run(['clang++','-std=c++20','-O0','-Wno-switch','-Wno-deprecated-declarations',
        '-fsanitize=undefined','-fno-sanitize-recover=all','-I',str(context['repo']/'src/libs/util/include'),
        '-I',str(context['args'].fast_float_include),str(root/'main.cpp'),'-o',str(root/'oracle')],check=True)
    rows=subprocess.run([str(root/'oracle')],capture_output=True,text=True,check=True,timeout=20).stdout.splitlines()
assert len(rows)==len(cases)
for case,row in zip(cases,rows):
    kind,value=row.split();case['expected']={'kind':kind,'value':int(value)}
retained={s:hashlib.sha256(context['extract'](context['data'],s).encode()).hexdigest() for s in context['methods']}
(output/'ptr_cases.json').write_text(json.dumps({'retained_sha256':retained,'cases':cases},indent=2)+'\n')
print(f'{len(cases)} retained DATA ptr cases under UBSan')
