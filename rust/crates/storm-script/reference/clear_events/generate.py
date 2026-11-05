"""Run original ClearEvents routing and unedited S_EVENTTAB under UBSan.

The native case, CoreImpl::ClearEvents and COMPILER::ClearEvents are extracted
verbatim. The driver supplies their owning classes and routes each clear through
all three bodies. This checks table order/status/static flags, not the complete
interpreter or game. One isolated final process confirms a retained clear-time
frame hang with a one-second timeout. Posted messages are outside this oracle.
"""
from pathlib import Path
import hashlib
import json
import random
import shutil
import subprocess
import tempfile

here = Path(__file__).resolve().parent
repo = next(p for p in here.parents if (p/'src/libs/core/src/compiler.cpp').is_file())
core = repo/'src/libs/core/src'
compiler = (core/'compiler.cpp').read_text()
impl = (core/'core_impl.cpp').read_text()
internal = (core/'internal_functions.cpp').read_text()

def method(text, signature):
    start = text.index(signature)
    end = text.index('\n}', start)+2
    return text[start:end]

compiler_clear = method(compiler, 'void COMPILER::ClearEvents()')
core_clear = method(impl, 'void CoreImpl::ClearEvents()')
start = internal.index('    case FUNC_CLEAR_EVENTS:')
native_clear = internal[start:internal.index('    case FUNC_CLEAR_POST_EVENTS:', start)]
files = ['s_eventtab.cpp', 's_eventtab.h']
sequences = [
    [['clear'], ['clear'], ['frame']],
    [['add','E',0,0,False], ['add','E',1,1,False], ['add','E',2,0,False], ['clear'], ['clear'], ['add','E',3,1,False], ['frame']],
    [['add','E',0,0,True], ['add','E',1,0,False], ['add','E',2,1,False], ['clear'], ['clear'], ['frame']],
    [['add','E',0,0,False], ['add','E',1,1,True], ['clear'], ['frame'], ['clear']],
    [['add','E',0,0,True], ['delete','E',0], ['add','E',1,1,False], ['clear'], ['clear'], ['add','e',0,1,False], ['clear']],
    [['add','E',0,0,False], ['add','E',0,0,True], ['clear'], ['add','E',0,0,True], ['add','e',0,0,False], ['clear']],
    [['add','Other',0,1,True], ['add','E',1,1,False], ['add','Other',2,0,False], ['clear'], ['clear'], ['frame']],
    [['add','E',0,0,False], ['add','E',1,0,True], ['add','E',2,0,False], ['delete','E',1], ['clear'], ['clear'], ['add','E',1,1,False], ['clear']],
]
rng = random.Random(441019)
for _ in range(16):
    steps = []
    # Static function 0 always occupies the first slot; function 3 is never
    # explicitly deleted. Random finite cases cannot enter the retained hang.
    for event in ['E','Other']:
        steps += [['add',event,0,rng.choice([0,1]),True], ['add',event,3,rng.choice([0,1]),True]]
    for _ in range(32):
        event = rng.choice(['E','e','Other'])
        fn = rng.randrange(4)
        op = rng.randrange(6)
        if op < 3:
            steps.append(['add',event,fn,rng.choice([0,1,-1]),fn in [0,3]])
        elif op == 3:
            steps.append(['delete',event,fn if fn != 3 else 0])
        else:
            steps.append([rng.choice(['clear','frame'])])
    sequences.append(steps)
hang = [['add','E',0,0,True], ['add','E',1,0,True], ['delete','E',1], ['clear']]

driver = r'''
#include "s_eventtab.h"
#include <iostream>
#include <string>
struct COMPILER {S_EVENTTAB EventTab;void ClearEvents();};
struct CoreImpl {COMPILER* Compiler;void ClearEvents();};
__COMPILER__
__CORE__
int main(){
    COMPILER compiler;CoreImpl core_internal{&compiler};auto&table=compiler.EventTab;
    auto clear_builtin=[&](){constexpr int FUNC_CLEAR_EVENTS=0;switch(FUNC_CLEAR_EVENTS){
__NATIVE__
    }};
    char op;std::string event;unsigned fn;int flag,stat;
    while(std::cin>>op){
        if(op=='a'){std::cin>>event>>fn>>flag>>stat;table.AddEventHandler(event.c_str(),fn,1,flag,stat);}
        if(op=='d'){std::cin>>event>>fn;table.SetStatus(event.c_str(),fn,FSTATUS_DELETED);}
        if(op=='f')table.ProcessFrame();
        if(op=='c')clear_builtin();
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
        std::cout<<']'<<std::endl;
    }
}
'''.replace('__COMPILER__', compiler_clear).replace('__CORE__', core_clear).replace('__NATIVE__', native_clear)

def commands(steps):
    return '\n'.join(' '.join([{'add':'a','delete':'d','clear':'c','frame':'f'}[s[0]], *[str(int(v)) if isinstance(v,bool) else str(v) for v in s[1:]]]) for s in steps)+'\n'

with tempfile.TemporaryDirectory(prefix='storm-clear-events-') as directory:
    root = Path(directory)
    for name in files:
        shutil.copyfile(core/name,root/name)
    (root/'data.h').write_text('#pragma once\n#include <cstdint>\n#include <cstring>\n#include <vector>\n')
    (root/'oracle.cpp').write_text(driver)
    subprocess.run(['clang++','-std=c++20','-O0','-fsanitize=undefined','-fno-sanitize-recover=all',
                    '-I',str(root),'-I',str(repo/'src/libs/util/include'),str(root/'oracle.cpp'),
                    str(root/'s_eventtab.cpp'),'-o',str(root/'oracle')],check=True)
    cases=[]
    for steps in sequences:
        run=subprocess.run([str(root/'oracle')],input=commands(steps),text=True,capture_output=True,timeout=5,check=True)
        snapshots=[json.loads(line) for line in run.stdout.splitlines()]
        assert len(snapshots)==len(steps)
        cases.append({'operations':steps,'snapshots':snapshots,'nonterminating_clear':False})
    try:
        subprocess.run([str(root/'oracle')],input=commands(hang),text=True,capture_output=True,timeout=1,check=True)
        raise AssertionError('retained clear-time loop unexpectedly terminated')
    except subprocess.TimeoutExpired as error:
        output=error.stdout.decode() if isinstance(error.stdout,bytes) else error.stdout
        snapshots=[json.loads(line) for line in output.splitlines()]
        assert len(snapshots)==len(hang)-1
        cases.append({'operations':hang,'snapshots':snapshots,'nonterminating_clear':True})

fixture={'source':'unedited S_EVENTTAB plus verbatim native/CoreImpl/COMPILER ClearEvents routing; table-state driver, no posted messages',
         'files_sha256':{name:hashlib.sha256((core/name).read_bytes()).hexdigest() for name in files},
         'bodies_sha256':{name:hashlib.sha256(body.encode()).hexdigest() for name,body in [('native',native_clear),('core',core_clear),('compiler',compiler_clear)]},
         'cases':cases}
(here/'cases.json').write_text(json.dumps(fixture,separators=(',',':'))+'\n')
print('Generated',len(cases),'ClearEvents sequences,',sum(len(c['snapshots']) for c in cases),'table snapshots and one confirmed nonterminating clear')
