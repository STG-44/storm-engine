"""Check retained SHIP query cases, GetIndex, ATTRIBUTES and DATA under UBSan.

The two switch cases, GetIndex, GetAttributeAsDword and DATA::Set(int32_t)
are compiled verbatim. Character/tree lookup, MESSAGE transport, DATA storage
and assertion-to-error handling are adapters. All numeric strings fit signed
32-bit C long; out-of-range atol behavior and full SHIP initialization are excluded.
"""
from pathlib import Path
import hashlib,json,re,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/ship/src/ship.cpp').is_file())
ship=(repo/'src/libs/ship/src/ship.cpp').read_text()
data=(repo/'src/libs/core/src/data.cpp').read_text()
attributes=(repo/'src/libs/core/src/attributes.cpp').read_text()
character=(repo/'src/libs/location/include/character.h').read_text()
headers=(repo/'src/libs/shared_headers/include/shared/messages.h').read_text()
def method(source,signature):
 start=source.index(signature);return source[start:source.index('\n}',start)+2]
start=ship.index('    case MSG_SHIP_GET_CHARACTER_INDEX:',ship.index('uint64_t SHIP::ProcessMessage'))
bodies={'cases':ship[start:ship.index('    case MSG_SHIP_RESET_TRACK:',start)],
 'index':method(character,'inline int32_t GetIndex('),
 'dword':method(attributes,'uint32_t ATTRIBUTES::GetAttributeAsDword('),
 'integer':method(data,'void DATA::Set(int32_t value)')}
constants={name:int(re.search(r'#define\s+'+name+r'\s+(\d+)',headers).group(1)) for name in ['MSG_SHIP_GET_CHARACTER_INDEX','MSG_SHIP_GET_NUM_FIRE_PLACES']}
source=r'''
#include <bit>
#include <cstdint>
#include <cstdlib>
#include <iostream>
#include <stdexcept>
#include <string>
#include <vector>
#define Assert(value) do {if(!(value))throw std::runtime_error("assertion");} while(false)
enum {VAR_INTEGER,VAR_FLOAT,VAR_REFERENCE};
const char* UNINIT_REF="UNINIT_REF";const char* NO_INDEX="NO_INDEX";
struct DATA {
 int Data_type=VAR_FLOAT;bool bArray=false;DATA* pReference=nullptr;int32_t lValue=0;float fValue=0.25f;
 void Error(const char* text){throw std::runtime_error(text);}void Set(int32_t);
};
struct ATTRIBUTES {
 bool exists=false,has_value=false;std::string text;std::string* value_=&text;
 ATTRIBUTES* FindAClass(ATTRIBUTES*,const char*){return exists?this:nullptr;}
 const char* GetAttribute(const char*)const{return exists&&has_value?text.c_str():nullptr;}
 uint32_t GetAttributeAsDword(const char* name=nullptr,uint32_t def=0)const;
};
struct MESSAGE {int32_t code;DATA* output;int32_t Long(){return code;}DATA* ScriptVariablePointer(){return output;}};
struct SHIP {
 ATTRIBUTES* character=nullptr;std::vector<int> aFirePlaces;
 ATTRIBUTES* GetACharacter(){return character;}uint64_t ProcessMessage(MESSAGE&);
};
__CONSTANTS__
__METHODS__
uint64_t SHIP::ProcessMessage(MESSAGE& message){auto code=message.Long();switch(code){__CASES__}return 0;}
void query(SHIP& ship,int32_t code){
 DATA out;MESSAGE message{code,&out};bool error=false;uint64_t result=0;
 try{result=ship.ProcessMessage(message);}catch(const std::runtime_error&){error=true;}
 std::cout<<error<<" "<<result<<" "<<out.Data_type<<" "<<(out.Data_type==VAR_INTEGER?int64_t(out.lValue):int64_t(std::bit_cast<uint32_t>(out.fValue)))<<" ";
}
void run(bool bound,bool index,const char* text,size_t count){
 ATTRIBUTES attributes;attributes.exists=index;attributes.has_value=text;attributes.text=text?text:"";
 SHIP ship;ship.character=bound?&attributes:nullptr;ship.aFirePlaces.resize(count);
 query(ship,MSG_SHIP_GET_CHARACTER_INDEX);query(ship,MSG_SHIP_GET_NUM_FIRE_PLACES);
 attributes.text="-9after";attributes.has_value=true;
 ship.aFirePlaces.resize(count?0:3);
 query(ship,MSG_SHIP_GET_CHARACTER_INDEX);query(ship,MSG_SHIP_GET_NUM_FIRE_PLACES);
 std::cout<<"\n";
}
int main(){__RUNS__}
'''
index_cases=[(False,False,None),(True,False,None),(True,True,None),(True,True,''),
 (True,True,'19'),(True,True,' -7tail'),(True,True,'\t+42end'),(True,True,'2147483647'),
 (True,True,'-2147483648'),(True,True,'oops'),(True,True,'0x13'),(True,True,'3.5'),
 (True,True,'0019'),(True,True,'+27')]
cases=[{'bound':b,'index_exists':i,'index_name':'InDeX','index_text':s,'fire_places':n} for b,i,s in index_cases for n in [0,1,7,256]]
source=source.replace('__CONSTANTS__','\n'.join(f'constexpr int32_t {k}={v};' for k,v in constants.items()))
source=source.replace('__METHODS__','\n'.join(v for k,v in bodies.items() if k!='cases')).replace('__CASES__',bodies['cases'])
source=source.replace('__RUNS__','\n'.join('run('+','.join([str(c['bound']).lower(),str(c['index_exists']).lower(),json.dumps(c['index_text']) if c['index_text'] is not None else 'nullptr',str(c['fire_places'])])+');' for c in cases))
with tempfile.TemporaryDirectory(prefix='storm-ship-queries-') as folder:
 path=Path(folder);(path/'main.cpp').write_text(source)
 subprocess.run(['clang++','-std=c++20','-fsanitize=undefined','-fno-sanitize-recover=all',str(path/'main.cpp'),'-o',str(path/'oracle')],check=True)
 rows=subprocess.run([str(path/'oracle')],capture_output=True,text=True,check=True,timeout=20).stdout.splitlines()
assert len(rows)==len(cases)
for case,row in zip(cases,rows):
 values=list(map(int,row.split()));assert len(values)==16
 case['expected']=[{'error':bool(values[n]),'result':values[n+1],'type':'int' if values[n+2]==0 else 'float','value':values[n+3]} for n in range(0,16,4)]
(here/'cases.json').write_text(json.dumps({'scope':__doc__,'constants':constants,'retained_sha256':{k:hashlib.sha256(v.encode()).hexdigest() for k,v in bodies.items()},'cases':cases},indent=2)+'\n')
print(f'{len(cases)} retained query sequences / {len(cases)*4} complete query snapshots under UBSan')
