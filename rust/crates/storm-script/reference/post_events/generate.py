"""Run unedited retained timer/list headers and compiler queue loops under UBSan.

Only MESSAGE storage, platform wrappers and the ProcessEvent callback are stubbed.
This checks queue/frame/cancellation behavior, not original game program execution.
"""
from pathlib import Path
import hashlib
import json
import random
import subprocess
import tempfile

here = Path(__file__).resolve().parent
repo = next(p for p in here.parents if (p/'src/libs/core/src/compiler.cpp').is_file())
core = repo/'src/libs/core/src'
compiler = (core/'compiler.cpp').read_text()
frame_start = compiler.index('    for (int32_t ln = 0;', compiler.index('void COMPILER::ProcessFrame'))
frame_end = compiler.index('    EventMsg.RemoveInvalidated();', frame_start)+len('    EventMsg.RemoveInvalidated();')
frame = compiler[frame_start:frame_end]
cancel_start = compiler.index('    for (int32_t n = 0;', compiler.index('void COMPILER::DelEventHandler'))
cancel_end = compiler.index('\n}\n', cancel_start)
cancel = compiler[cancel_start:cancel_end]
sequences = [
    [['post','A',0,1],['frame',0],['frame',0],['frame',0]],
    [['post','A',10,1],['frame',100],['frame',0]],
    [['post','A',20,1],['post','B',10,2],['delete','a'],['frame',10]],
    [['post','A',0,1],['post','B',0,2],['clear'],['frame',0],['frame',0]],
    [['hook','A','post','B',0,2],['post','A',0,1],['frame',0],['frame',5],['frame',0]],
    [['hook','A','clear'],['post','A',0,1],['post','B',0,2],['frame',0],['frame',0],['frame',0]],
    [['hook','A','delete','B'],['post','A',0,1],['post','B',100,2],['frame',0],['frame',1]],
    [['post','A',-1,1],['frame',2147483647],['frame',2147483647],['frame',2],['frame',4294967295]],
    [['post','A',0,-1],['delete','Other'],['frame',0]],
    [['hook','A','delete','B'],['post','B',100,-1],['post','A',0,1],['post','C',0,3],['frame',0],['frame',1],['frame',0]],
]
rng = random.Random(441018)
for _ in range(12):
    steps = []
    for n in range(32):
        choice = rng.randrange(8)
        if choice < 3:
            steps.append(['post',rng.choice(['A','B','a']),rng.choice([0,1,10,100,-1]),n])
        elif choice < 6:
            steps.append(['frame',rng.choice([0,1,9,10,100,2147483647,4294967295])])
        elif choice == 6:
            steps.append(['delete',rng.choice(['A','b','Missing'])])
        else:
            steps.append(['clear'])
    sequences.append(steps)

driver = r'''
#include "s_postevents.h"
#include "string_compare.hpp"
#include <iostream>
#include <map>
#include <string>
#include <vector>
struct Hook {std::string kind,name;int delay=0,id=-1;};
struct COMPILER {
    POSTEVENTS_LIST EventMsg;
    std::map<std::string,Hook> hooks;
    std::vector<std::pair<std::string,int>> fired;
    static std::string lower(std::string s){for(auto&c:s)if(c>='A'&&c<='Z')c+='a'-'A';return s;}
    void post(const std::string& name,int delay,int id){EventMsg.Add(new S_EVENTMSG(name.c_str(),id<0?nullptr:new MESSAGE{id},static_cast<uint32_t>(delay)));}
    void cancel(const char* event_name) {
__CANCEL__
    }
    void ProcessEvent(const char* name,MESSAGE* message) {
        fired.emplace_back(name,message?message->id:-1);
        auto found=hooks.find(lower(name));
        if(found==hooks.end())return;
        auto hook=found->second;hooks.erase(found);
        if(hook.kind=="post")post(hook.name,hook.delay,hook.id);
        if(hook.kind=="delete")cancel(hook.name.c_str());
        if(hook.kind=="clear")EventMsg.InvalidateAll();
    }
    void frame(uint32_t DeltaTime) {
__FRAME__
    }
    void snapshot() {
        std::cout<<"{\"fired\":[";
        for(unsigned n=0;n<fired.size();n++){if(n)std::cout<<',';std::cout<<"[\""<<fired[n].first<<"\","<<fired[n].second<<']';}
        std::cout<<"],\"queue\":[";
        for(unsigned n=0;n<EventMsg.GetClassesNum();n++){
            if(n)std::cout<<',';auto*e=EventMsg.Read(n);
            std::cout<<"[\""<<e->pEventName<<"\","<<e->nTime<<','<<e->nPeriod<<','<<e->bProcess<<','<<e->bInvalide<<','<<(e->pMessageClass?e->pMessageClass->id:-1)<<']';
        }
        std::cout<<"]}\n";fired.clear();
    }
};
int main(){
    COMPILER c;std::string op,name;int delay,id;uint32_t dt;
    while(std::cin>>op){
        if(op=="post"){std::cin>>name>>delay>>id;c.post(name,delay,id);}
        if(op=="delete"){std::cin>>name;c.cancel(name.c_str());}
        if(op=="clear")c.EventMsg.InvalidateAll();
        if(op=="frame"){std::cin>>dt;c.frame(dt);}
        if(op=="hook"){
            Hook hook;std::cin>>name>>hook.kind;
            if(hook.kind=="post")std::cin>>hook.name>>hook.delay>>hook.id;
            if(hook.kind=="delete")std::cin>>hook.name;
            c.hooks[COMPILER::lower(name)]=hook;
        }
        c.snapshot();
    }
}
'''.replace('__CANCEL__',cancel).replace('__FRAME__',frame)
with tempfile.TemporaryDirectory(prefix='storm-post-reference-') as tmp:
    tmp = Path(tmp)
    for name in ['s_eventmsg.h','s_postevents.h']:
        (tmp/name).write_bytes((core/name).read_bytes())
    (tmp/'platform').mkdir()
    (tmp/'platform/platform.hpp').write_text('#pragma once\n#include <cstdint>\n#include <cstring>\n#include <cstdlib>\ninline void strcpy_s(char*d,size_t,const char*s){std::strcpy(d,s);}\n')
    (tmp/'message.h').write_text('#pragma once\nstruct MESSAGE {int id;};\n')
    (tmp/'main.cpp').write_text(driver)
    subprocess.run(['clang++','-std=c++20','-fsanitize=undefined','-fno-sanitize-recover=all',
                    '-I'+str(tmp),'-I'+str(repo/'src/libs/util/include'),str(tmp/'main.cpp'),'-o',str(tmp/'oracle')],check=True)
    cases = []
    for operations in sequences:
        commands='\n'.join(' '.join(map(str,op)) for op in operations)+'\n'
        run=subprocess.run([str(tmp/'oracle')],input=commands,text=True,capture_output=True,check=True)
        snapshots=[json.loads(line) for line in run.stdout.splitlines()]
        assert len(snapshots)==len(operations)
        cases.append({'operations':operations,'snapshots':snapshots})
    sources={name:hashlib.sha256((core/name).read_bytes()).hexdigest() for name in ['compiler.cpp','s_eventmsg.h','s_postevents.h']}
    fixture={'source_sha256':sources,'frame_loop_sha256':hashlib.sha256(frame.encode()).hexdigest(),
             'cancellation_loop_sha256':hashlib.sha256(cancel.encode()).hexdigest(),'cases':cases}
    (here/'cases.json').write_text(json.dumps(fixture,separators=(',',':'))+'\n')
    print(f'Generated {len(cases)} queue sequences, {sum(len(c["operations"]) for c in cases)} snapshots')
