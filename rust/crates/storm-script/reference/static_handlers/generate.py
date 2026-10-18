"""Check retained #event_handler parsing and static event-table lifetimes.

TOKEN and S_EVENTTAB source/headers are copied unedited. The compiler's complete
EVENT_HANDLER case and SetEventHandler method are extracted verbatim. Function/
define lookup, cache/error/platform storage and the outer scan are adapters;
this does not execute the full compiler or any original game program. A final
isolated process demonstrates the original static-deletion frame hang with a
one-second timeout. No production process runs that unbounded loop.
"""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import tempfile

here = Path(__file__).resolve().parent
repo = next(p for p in here.parents if (p / 'src/libs/core/src/compiler.cpp').is_file())
core = repo / 'src/libs/core/src'
compiler = (core / 'compiler.cpp').read_text()
start = compiler.index('        case EVENT_HANDLER:', compiler.index('bool COMPILER::CompileBlock('))
end = compiler.index('\n        case GOTO_COMMAND:', start)
case_body = compiler[start:end]
start = compiler.index('void COMPILER::SetEventHandler(')
end = compiler.index('\nvoid COMPILER::DelEventHandler(', start)
set_handler = compiler[start:end]
files = ['token.cpp', 'token.h', 's_eventtab.cpp', 's_eventtab.h']

cases = []
for source, defines in [
    ('#event_handler("E", "A")', {}),
    ('#event_handler("e", "a"); #event_handler("E", "B") #event_handler("E", "A")', {}),
    ('#event_handler(NAME, "A") #event_handler("Other", "B")', {'name': 'E'}),
    ('void Never(){if(0){#event_handler(NAME, "B");} return; #event_handler("E", "A")}', {'name': 'E'}),
    ('#event_handler("E", "B") #event_handler(NAME, "A") #event_handler("e", "b")', {'name': 'e'}),
    ('#EVENT_HANDLER("E", "A")', {}),
    ('#event_handler "E", "A")', {}),
    ('#event_handler(7, "A")', {}),
    ('#event_handler(NAME, "A")', {}),
    ('#event_handler(NAME, "A")', {'name': 7}),
    ('#event_handler("E" "A")', {}),
    ('#event_handler("E", A)', {}),
    ('#event_handler("E", NAME)', {'name': 'A'}),
    ('#event_handler("E", "A", 0)', {}),
    ('#event_handler("E", "A"', {}),
    ('#event_handler("E", "Missing")', {}),
]:
    cases.append({'source': source, 'defines': defines, 'functions': ['A', 'B']})

# add: event, function code, activation flag, static flag. Duplicate adds must
# retain the first entry's static flag; deletion marks static entries inactive.
table_cases = [
    [['add', 'E', 0, 0, True], ['add', 'E', 1, 1, False], ['delete', 'E', 0], ['frame'], ['frame'], ['add', 'e', 0, 1, False], ['frame']],
    [['add', 'E', 0, 0, False], ['add', 'E', 0, 1, True], ['delete', 'E', 0], ['frame'], ['add', 'E', 0, 0, True], ['frame']],
    [['add', 'E', 0, 0, True], ['add', 'e', 0, -1, False], ['delete', 'E', 0], ['frame'], ['add', 'E', 0, 1, True], ['frame']],
    [['add', 'E', 0, 0, False], ['add', 'E', 1, 0, True], ['delete', 'E', 0], ['delete', 'E', 1], ['frame'], ['frame'], ['add', 'E', 1, -1, False], ['frame']],
    [['add', 'E', 0, 0, True], ['add', 'E', 1, 1, False], ['add', 'E', 2, 1, True], ['delete', 'E', 0], ['frame'], ['add', 'E', 0, 0, False], ['frame']],
    [['add', 'Other', 0, 1, True], ['add', 'E', 1, -1, True], ['frame'], ['delete', 'Other', 0], ['frame'], ['add', 'OTHER', 0, -1, False], ['frame']],
    [['add', 'E', 0, 0, True], ['add', 'E', 1, 0, True], ['delete', 'E', 1], ['add', 'e', 1, 1, False], ['frame']],
    [['add', 'E', 0, 0, True], ['add', 'E', 1, 0, False], ['add', 'E', 2, 1, False], ['delete', 'E', 1], ['frame'], ['frame']],
]
hang = [['add', 'E', 0, 0, True], ['add', 'E', 1, 0, True], ['delete', 'E', 1], ['frame']]

driver = r'''
#include "token.h"
#include "s_eventtab.h"
#include <iomanip>
#include <iostream>
#include <map>
#include <string>
#include <vector>
static std::string lower(std::string s){for(auto&c:s)if(c>='A'&&c<='Z')c+='a'-'A';return s;}
static void dump(S_EVENTTAB& table){
    std::cout<<'[';bool first=true;
    for(auto name:{"E","Other"}){
        if(!first)std::cout<<',';first=false;
        auto code=table.FindEvent(name);if(code==INVALID_EVENT_CODE){std::cout<<"null";continue;}
        EVENTINFO info;table.GetEvent(info,code);std::cout<<'[';
        for(uint32_t n=0;n<info.elements;n++){
            if(n)std::cout<<',';auto&h=info.pFuncInfo[n];
            std::cout<<'['<<h.func_code<<','<<h.status<<','<<(h.bStatic?"true":"false")<<']';
        }
        std::cout<<']';
    }
    std::cout<<']';
}
constexpr auto INVALID_FUNC_CODE=0xffffffffu;
constexpr auto INVALID_DEF_CODE=0xffffffffu;
struct FuncInfo {uint32_t segment_id=1;};
struct DEFINFO {uint32_t deftype;uintptr_t data4b;};
struct Functions {
    std::vector<std::string> names;
    uint32_t FindFunc(const char* name){for(uint32_t n=0;n<names.size();n++)if(lower(names[n])==lower(name))return n;return INVALID_FUNC_CODE;}
    bool GetFunc(FuncInfo&info,uint32_t code){return code<names.size();}
};
struct Defines {
    std::vector<std::pair<std::string,DEFINFO>> entries;
    std::map<std::string,std::string> strings;
    void add(const std::string& name,const std::string& text){auto&stored=strings[name];stored=text;entries.push_back({name,{STRING,reinterpret_cast<uintptr_t>(stored.c_str())}});}
    void add(const std::string& name,int number){entries.push_back({name,{NUMBER,static_cast<uintptr_t>(number)}});}
    uint32_t FindDef(const char* name){for(uint32_t n=0;n<entries.size();n++)if(entries[n].first==lower(name))return n;return INVALID_DEF_CODE;}
    bool GetDef(DEFINFO& info,uint32_t code){if(code>=entries.size())return false;info=entries[code].second;return true;}
};
namespace storm::script_cache {struct EventHandler{std::string event,name;};}
constexpr int kCacheDisabled=0;
struct COMPILER {
    TOKEN Token;S_EVENTTAB EventTab;Functions FuncTab;Defines DefTab;
    char gs[4096];unsigned errors=0;int script_cache_mode_=kCacheDisabled;
    struct {std::vector<storm::script_cache::EventHandler> event_handlers;} script_cache_;
    void SetError(const char*){errors++;}
    void SetEventHandler(const char*,const char*,int32_t,bool);
    bool scan(std::string& source){
        Token.SetProgram(source.data(),source.data());uint32_t def_code;DEFINFO di;
        while(true){auto type=Token.Get();if(type==END_OF_PROGRAMM)return true;
            switch(type){
__CASE__
            default:break;
            }
        }
    }
};
__SET_HANDLER__
int main(int argc,char**argv){
    if(argc>1){
        S_EVENTTAB table;char op;std::string event;unsigned fn;int flag,stat;
        while(std::cin>>op){
            if(op=='a'){std::cin>>event>>fn>>flag>>stat;table.AddEventHandler(event.c_str(),fn,1,flag,stat);}
            if(op=='d'){std::cin>>event>>fn;table.SetStatus(event.c_str(),fn,FSTATUS_DELETED);}
            if(op=='f')table.ProcessFrame();
            dump(table);std::cout<<std::endl;
        }
        return 0;
    }
'''.replace('__CASE__', case_body).replace('__SET_HANDLER__', set_handler)
for case in cases:
    driver += '{COMPILER c;\n'
    for name in case['functions']:
        driver += f'c.FuncTab.names.push_back({json.dumps(name)});\n'
    for name, value in case['defines'].items():
        driver += f'c.DefTab.add({json.dumps(name)},{json.dumps(value)});\n'
    driver += f'std::string source={json.dumps(case["source"])};auto ok=c.scan(source);\n'
    driver += r'''std::cout<<"{\"ok\":"<<((ok&&c.errors==0)?"true":"false")<<",\"table\":";dump(c.EventTab);std::cout<<"}"<<std::endl;}
'''
driver += '}\n'

def commands(steps):
    return '\n'.join(' '.join([{'add':'a','delete':'d','frame':'f'}[step[0]], *[str(int(v)) if isinstance(v,bool) else str(v) for v in step[1:]]]) for step in steps)+'\n'

with tempfile.TemporaryDirectory(prefix='storm-static-handlers-') as directory:
    root = Path(directory)
    for name in files:
        shutil.copyfile(core / name, root / name)
    (root / 'data.h').write_text('#pragma once\n#include <cstdint>\n#include <cstring>\n#include <vector>\n')
    (root / 'windows.h').write_text('''#pragma once
#include <cstdlib>
#include <cstring>
#include <stdexcept>
#define MAX_PATH 260
template<size_t N> void strcpy_s(char(&dst)[N],const char*src){if(strlen(src)>=N)throw std::runtime_error("copy limit");strcpy(dst,src);}
''')
    (root / 'oracle.cpp').write_text(driver)
    subprocess.run(['clang++','-std=c++20','-O0','-fsanitize=undefined','-fno-sanitize-recover=all',
                    '-I',str(root),'-I',str(repo/'src/libs/util/include'),str(root/'oracle.cpp'),
                    str(root/'token.cpp'),str(root/'s_eventtab.cpp'),'-o',str(root/'oracle')],check=True)
    lines = subprocess.check_output([str(root/'oracle')],text=True).splitlines()
    assert len(lines)==len(cases)
    for case,line in zip(cases,lines):
        case['expected']=json.loads(line)
    sequences=[]
    for steps in table_cases:
        run=subprocess.run([str(root/'oracle'),'table'],input=commands(steps),text=True,capture_output=True,timeout=5,check=True)
        snapshots=[json.loads(line) for line in run.stdout.splitlines()]
        assert len(snapshots)==len(steps)
        sequences.append({'operations':steps,'snapshots':snapshots,'nonterminating_frame':False})
    try:
        subprocess.run([str(root/'oracle'),'table'],input=commands(hang),text=True,capture_output=True,timeout=1,check=True)
        raise AssertionError('retained static-deletion loop unexpectedly terminated')
    except subprocess.TimeoutExpired as error:
        output=error.stdout.decode() if isinstance(error.stdout,bytes) else error.stdout
        snapshots=[json.loads(line) for line in output.splitlines()]
        assert len(snapshots)==len(hang)-1
        sequences.append({'operations':hang,'snapshots':snapshots,'nonterminating_frame':True})

fixture={'source':'unedited TOKEN/S_EVENTTAB plus verbatim compiler EVENT_HANDLER and SetEventHandler; lookup/scan adapters',
         'files_sha256':{name:hashlib.sha256((core/name).read_bytes()).hexdigest() for name in files},
         'case_sha256':hashlib.sha256(case_body.encode()).hexdigest(),
         'set_handler_sha256':hashlib.sha256(set_handler.encode()).hexdigest(),
         'cases':cases,'table_cases':sequences}
(here/'cases.json').write_text(json.dumps(fixture,separators=(',',':'))+'\n')
print('Generated',len(cases),'declaration cases;',len(sequences),'static lifetime sequences,',sum(len(s['snapshots']) for s in sequences),'snapshots and one confirmed nonterminating frame')
