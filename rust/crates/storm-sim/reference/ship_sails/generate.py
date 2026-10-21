"""Run the two unedited SHIP sail cases, setters and DATA float setter under UBSan.

MESSAGE transport and DATA allocation/error handling are adapters. Constructor
initialization is extracted from SHIP; Mount, InitSailState, other messages,
the compiler and sailing/cloth physics are outside this reference.
"""
from pathlib import Path
import hashlib
import json
import re
import subprocess
import tempfile

here = Path(__file__).resolve().parent
repo = next(p for p in here.parents if (p / 'src/libs/ship/src/ship.cpp').is_file())
ship = (repo / 'src/libs/ship/src/ship.cpp').read_text()
data = (repo / 'src/libs/core/src/data.cpp').read_text()
headers = (repo / 'src/libs/shared_headers/include/shared/messages.h').read_text()
def method(source, signature):
    start = source.index(signature)
    return source[start:source.index('\n}', start) + 2]
bodies = {s: method(ship, s) for s in ['void SHIP::SetSailState(', 'float SHIP::GetSailState()']}
bodies['DATA::Set(float)'] = method(data, 'void DATA::Set(float value)')
start = ship.index('    case MSG_SHIP_SET_SAIL_STATE:', ship.index('uint64_t SHIP::ProcessMessage'))
bodies['sail_cases'] = ship[start:ship.index('    case MSG_SHIP_SET_POS:', start)]
bodies['constructor_field'] = re.search(r'\bfSailState = NAN;', ship).group()
constants = {name: int(re.search(r'#define\s+' + name + r'\s+(\d+)', headers).group(1))
             for name in ['MSG_SHIP_SET_SAIL_STATE', 'MSG_SHIP_GET_SAIL_STATE']}
source = r'''
#include <bit>
#include <cmath>
#include <cstdint>
#include <iostream>
#include <stdexcept>
using std::isnan;
enum {VAR_INTEGER, VAR_FLOAT, VAR_REFERENCE};
const char* UNINIT_REF="UNINIT_REF";const char* NO_INDEX="NO_INDEX";
struct DATA {
 int Data_type=VAR_INTEGER;bool bArray=false;DATA* pReference=nullptr;float fValue=0;
 void Error(const char* text){throw std::runtime_error(text);}
 void Set(float value);
};
struct MESSAGE {
 int32_t code;float value;DATA* output;
 int32_t Long(){return code;}float Float(){return value;}
 DATA* ScriptVariablePointer(){return output;}
};
struct SHIP {
 float fSailState;
 SHIP(){__CONSTRUCTOR__}
 void SetSailState(float);float GetSailState();uint64_t ProcessMessage(MESSAGE&);
};
__CONSTANTS__
__METHODS__
uint64_t SHIP::ProcessMessage(MESSAGE& message){
 const auto code=message.Long();
 switch(code){__CASES__}
 return 0;
}
void run(bool set,uint32_t bits){
 SHIP ship;DATA out;MESSAGE m{MSG_SHIP_SET_SAIL_STATE,std::bit_cast<float>(bits),&out};
 auto set_result=set?ship.ProcessMessage(m):0;
 m.code=MSG_SHIP_GET_SAIL_STATE;uint64_t get_result=0;bool error=false;
 try{get_result=ship.ProcessMessage(m);}catch(const std::runtime_error&){error=true;}
 std::cout<<std::bit_cast<uint32_t>(ship.GetSailState())<<" "<<out.Data_type<<" "
          <<std::bit_cast<uint32_t>(out.fValue)<<" "<<error<<" "<<set_result<<" "<<get_result<<"\n";
}
int main(){__RUNS__}
'''
cases = [{'set': False, 'bits': 0}] + [{'set': True, 'bits': bits} for bits in [
    0, 0x80000000, 1, 0x80000001, 0x3e999999, 0x3e99999a, 0x3e99999b,
    0x3f000000, 0x3f333332, 0x3f333333, 0x3f333334, 0x3f800000,
    0xbf800000, 0x3fc00000, 0x7f7fffff, 0x7f800000, 0xff800000,
    0x7fc12345, 0xffc54321]]
source = source.replace('__CONSTRUCTOR__', bodies['constructor_field'])
source = source.replace('__CONSTANTS__', '\n'.join(f'constexpr int32_t {k}={v};' for k, v in constants.items()))
source = source.replace('__METHODS__', '\n'.join(v for k, v in bodies.items() if k not in ['sail_cases', 'constructor_field']))
source = source.replace('__CASES__', bodies['sail_cases'])
source = source.replace('__RUNS__', '\n'.join(f"run({str(c['set']).lower()},{c['bits']}u);" for c in cases))
with tempfile.TemporaryDirectory(prefix='storm-ship-sails-') as folder:
    path = Path(folder)
    (path / 'main.cpp').write_text(source)
    subprocess.run(['clang++', '-std=c++20', '-fsanitize=undefined', '-fno-sanitize-recover=all',
                    str(path / 'main.cpp'), '-o', str(path / 'oracle')], check=True)
    rows = subprocess.run([str(path / 'oracle')], capture_output=True, text=True, check=True, timeout=20).stdout.splitlines()
assert len(rows) == len(cases)
for case, row in zip(cases, rows):
    state, kind, output, error, set_result, get_result = map(int, row.split())
    assert kind == 1
    case['expected'] = {'state_bits': state, 'output_bits': output, 'get_error': bool(error),
                        'set_result': set_result, 'get_result': get_result}
(here / 'cases.json').write_text(json.dumps({'scope': __doc__, 'constants': constants,
    'retained_sha256': {k: hashlib.sha256(v.encode()).hexdigest() for k, v in bodies.items()},
    'cases': cases}, indent=2) + '\n')
print(f'{len(cases)} retained SHIP sail-message cases under UBSan')
