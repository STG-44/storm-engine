"""Check source registration using retained TOKEN, STRINGS_LIST and S_DEFTAB.

Those sources/headers are copied unedited. Compiler AppendProgram and include/
define case bodies are extracted verbatim. The outer scan, virtual file reads,
cache/debug storage and Windows compatibility header are scaffolding. This is
not the full C++ compiler or execution of an original game program. Numeric
constants only avoid the retained definition table's mismatched string delete.
"""
from pathlib import Path
import hashlib
import json
import random
import shutil
import subprocess
import tempfile

here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/core/src/compiler.cpp').is_file())
core=repo/'src/libs/core/src'
compiler=(core/'compiler.cpp').read_text()

def extract(text, signature):
    start=text.index(signature); p=text.index('{',start)+1; depth=1
    while depth:
        if text[p]=='{':depth+=1
        if text[p]=='}':depth-=1
        p+=1
    return text[start:p]

append=extract(compiler,'bool COMPILER::AppendProgram(char *&pBase_program')
start=compiler.index('        case INCLIDE_FILE:',compiler.index('bool COMPILER::Compile(SEGMENT_DESC'))
end=compiler.index('        case DEFINE_COMMAND:',start)
include=compiler[start:end]
start=end; end=compiler.index('\n            /*case DEFINE_COMMAND:',start)
define=compiler[start:end]
files=['token.cpp','token.h','strings_list.cpp','strings_list.h','s_deftab.cpp','s_deftab.h']

rng=random.Random(441020)
cases=[]
for n in range(20):
    names=['main.c','first.c','second.c','nested/third.c']
    sources={}
    for index,name in enumerate(names):
        constant=f'C{index}'
        value=rng.choice(['0','-7','548','4294967295','1.5','-2.25'])
        includes=names[1:] if index==0 else rng.sample(names,rng.randrange(4))
        text=f'// source {name}\n#define {constant} {value};\n'
        for target in includes:
            if rng.randrange(2):target=target.upper()
            text+=f'#include "{target}"\n'
        if n%5==0 and index==2:text+='#define c0 8;\n'
        sources[name]=text
    cases.append({'files':sources})
# Include a valid constant declared in the parent and repeated/cyclic file names.
cases.append({'files':{'main.c':'#define N 3\n#include "A.C"\n#include "b.c"\n#include "a.c"',
                      'a.c':'#include "C.C"\n#define NEG -19',
                      'b.c':'#include "MAIN.C"\n#define SCALE -1.25',
                      'c.c':'#include "b.c"\n#define END 4294967295'}})

# Values straddling binary32 midpoints expose atof(double)->float rounding.
cases.append({'files':{'main.c':
    '#define ABOVE 1.000000059604644775390625000000000001\n'
    '#define BELOW 1.000000178813934326171874999999999999\n'
    '#define NEGATIVE -1.000000059604644775390625000000000001\n'}})

source=r'''
#include "token.h"
#include "strings_list.h"
#include "s_deftab.h"
#include "string_compare.hpp"
#include <cstring>
#include <iomanip>
#include <iostream>
#include <map>
#include <string>
#include <vector>
namespace storm::script_cache {struct Define {std::string name;uint32_t type;uintptr_t value;};}
constexpr int kCacheDisabled=0;
struct COMPILER {
    TOKEN Token;STRINGS_LIST strings;S_DEFTAB DefTab;
    struct {STRINGS_LIST* Files_list;char* pData=nullptr;uint32_t id=1;} Segment;
    struct {std::vector<std::string> files;std::vector<storm::script_cache::Define> defines;} script_cache_;
    int script_cache_mode_=kCacheDisabled;
    bool bDebugInfo=false;
    std::map<std::string,std::string> files;
    char* pProgram=new char[1]{0};uint32_t Program_size=0;
    static std::string lower(std::string s){for(auto&c:s)if(c>='A'&&c<='Z')c+='a'-'A';return s;}
    COMPILER():Segment{&strings}{}
    ~COMPILER(){Token.Release();delete[]pProgram;}
    char* LoadFile(const char* path,uint32_t& size){
        auto found=files.find(lower(path));if(found==files.end())return nullptr;
        size=found->second.size();auto* data=new char[size+1];std::memcpy(data,found->second.c_str(),size+1);return data;
    }
    bool AppendProgram(char*&,uint32_t&,const char*,uint32_t&,bool);
    void SetError(const char*,const char*){}
    bool scan(){
        strings.AddUnicalString("main.c");
        uint32_t root_size;auto* root=LoadFile("main.c",root_size);AppendProgram(pProgram,Program_size,root,root_size,true);
        Token.SetProgram(pProgram,pProgram);
        uint32_t file_code=0,Append_file_size=0,inout=0,def_code=0;
        ptrdiff_t Control_offset=0;char* pApend_file=nullptr;char dbg_file_name[MAX_PATH],var_name[MAX_PATH];
        DEFINFO di{};int32_t lvalue=0;float fvalue=0;
        while(true){
            auto type=Token.Get();if(type==END_OF_PROGRAMM)return true;
            switch(type){
__INCLUDE__
__DEFINE__
            case BLOCK_IN:inout++;break;
            case BLOCK_OUT:inout--;break;
            default:break;
            }
        }
    }
    void dump(bool ok){
        std::cout<<"{\"ok\":"<<(ok?"true":"false")<<",\"files\":[";
        for(uint32_t i=0;i<strings.GetStringsCount();i++){if(i)std::cout<<',';std::cout<<std::quoted(lower(strings.GetString(i)));}
        std::cout<<"],\"defines\":[";
        for(uint32_t i=0;i<DefTab.GetDefNum();i++){
            if(i)std::cout<<',';DEFINFO info{};DefTab.GetDef(info,i);
            std::cout<<'['<<std::quoted(lower(info.name))<<',';
            if(info.deftype==NUMBER){int32_t n;std::memcpy(&n,&info.data4b,4);std::cout<<"\"i\","<<n;}
            else {uint32_t bits;std::memcpy(&bits,&info.data4b,4);std::cout<<"\"f\","<<bits;}
            std::cout<<']';
        }
        std::cout<<"]}\n";
    }
};
__APPEND__
int main(){
'''.replace('__INCLUDE__',include).replace('__DEFINE__',define).replace('__APPEND__',append)
for case in cases:
    source+='{COMPILER c;\n'
    for name,text in case['files'].items():source+=f'c.files[{json.dumps(name)}]={json.dumps(text)};\n'
    source+='auto ok=c.scan();c.dump(ok);}\n'
source+='}\n'
with tempfile.TemporaryDirectory(prefix='storm-source-reference-') as directory:
    root=Path(directory)
    for name in files:shutil.copyfile(core/name,root/name)
    (root/'windows.h').write_text('''#pragma once
#include <cstdlib>
#include <cstring>
#include <stdexcept>
#define MAX_PATH 260
inline void strcpy_s(char*dst,size_t n,const char*src){if(strlen(src)>=n)throw std::runtime_error("copy limit");strcpy(dst,src);}
template<size_t N> void strcpy_s(char(&dst)[N],const char*src){if(strlen(src)>=N)throw std::runtime_error("copy limit");strcpy(dst,src);}
template<size_t N> void strcat_s(char(&dst)[N],const char*src){if(strlen(dst)+strlen(src)>=N)throw std::runtime_error("copy limit");strcat(dst,src);}
''')
    (root/'data.h').write_text('#pragma once\n#include "token.h"\n#include <cstring>\n')
    (root/'oracle.cpp').write_text(source)
    subprocess.run(['clang++','-std=c++20','-O0','-fsanitize=undefined','-fno-sanitize-recover=all',
                    '-I',str(root),'-I',str(repo/'src/libs/util/include'),str(root/'oracle.cpp'),
                    *[str(root/name) for name in ['token.cpp','strings_list.cpp','s_deftab.cpp']],'-o',str(root/'oracle')],check=True)
    lines=subprocess.check_output([str(root/'oracle')],text=True).splitlines()
assert len(lines)==len(cases)
for case,line in zip(cases,lines):case['expected']=json.loads(line)
fixture={'source':'unedited TOKEN/STRINGS_LIST/S_DEFTAB plus retained compiler include/define/append bodies',
         'files_sha256':{name:hashlib.sha256((core/name).read_bytes()).hexdigest() for name in files},
         'include_sha256':hashlib.sha256(include.encode()).hexdigest(),
         'define_sha256':hashlib.sha256(define.encode()).hexdigest(),
         'append_sha256':hashlib.sha256(append.encode()).hexdigest(),'cases':cases}
(here/'cases.json').write_text(json.dumps(fixture,separators=(',',':'))+'\n')
print('Generated',len(cases),'source-registration cases;',sum(c['expected']['ok'] for c in cases),'accepted and',sum(not c['expected']['ok'] for c in cases),'duplicate-define failures')
