"""Run unedited S_EVENTTAB source/header with only its unused DATA include stubbed.

The fixtures check registration/status/frame behavior, not the full C++ compiler.
No game resources or Rust program is used. Requires clang++ and Python 3.
"""
from pathlib import Path
import hashlib
import json
import random
import subprocess
import tempfile

here = Path(__file__).resolve().parent
repo = next(p for p in here.parents if (p / 'src/libs/core/src/s_eventtab.cpp').is_file())
source = repo / 'src/libs/core/src/s_eventtab.cpp'
header = source.with_suffix('.h')
sequences = [[
    ['add', 'E', 0, 0], ['add', 'e', 1, 1], ['delete', 'E', 0],
    ['frame'], ['frame'], ['add', 'E', 0, 1], ['delete', 'E', 1],
    ['add', 'e', 1, 1], ['frame'],
]]
rng = random.Random(44017)
for _ in range(16):
    steps = []
    for _ in range(32):
        event = rng.choice(['E', 'e', 'Other'])
        function = rng.randrange(4)
        choice = rng.randrange(6)
        if choice < 3:
            steps.append(['add', event, function, rng.choice([0, 1, -1])])
        elif choice < 5:
            steps.append(['delete', event, function])
        else:
            steps.append(['frame'])
    sequences.append(steps)

driver = r'''
#include "s_eventtab.h"
#include <iostream>
int main() {
    S_EVENTTAB table;
    char op; std::string event; unsigned fn; int flag;
    while(std::cin >> op) {
        if(op == 'a') {std::cin >> event >> fn >> flag; table.AddEventHandler(event.c_str(),fn,0,flag,false);}
        if(op == 'd') {std::cin >> event >> fn; table.SetStatus(event.c_str(),fn,FSTATUS_DELETED);}
        if(op == 'f') table.ProcessFrame();
        std::cout << '[';
        bool firstEvent=true;
        for(auto name : {"E","Other"}) {
            if(!firstEvent)std::cout << ','; firstEvent=false;
            auto code=table.FindEvent(name);
            if(code == INVALID_EVENT_CODE) {std::cout << "null";continue;}
            EVENTINFO info;table.GetEvent(info,code);
            std::cout << '[';
            for(unsigned n=0;n<info.elements;n++) {
                if(n)std::cout << ',';
                std::cout << '[' << info.pFuncInfo[n].func_code << ',' << info.pFuncInfo[n].status << ']';
            }
            std::cout << ']';
        }
        std::cout << "]\n";
    }
}
'''
with tempfile.TemporaryDirectory(prefix='storm-event-reference-') as tmp:
    tmp = Path(tmp)
    (tmp / source.name).write_bytes(source.read_bytes())
    (tmp / header.name).write_bytes(header.read_bytes())
    (tmp / 'data.h').write_text('#pragma once\n#include <cstdint>\n#include <cstring>\n#include <vector>\n')
    (tmp / 'main.cpp').write_text(driver)
    subprocess.run(['clang++', '-std=c++20', '-fsanitize=undefined', '-fno-sanitize-recover=all',
                    '-I' + str(repo / 'src/libs/util/include'), str(tmp / source.name),
                    str(tmp / 'main.cpp'), '-o', str(tmp / 'oracle')], check=True)
    cases = []
    for steps in sequences:
        commands = []
        for step in steps:
            op = {'add':'a','delete':'d','frame':'f'}[step[0]]
            commands.append(' '.join([op, *map(str, step[1:])]))
        run = subprocess.run([str(tmp / 'oracle')],input='\n'.join(commands)+'\n',text=True,capture_output=True,check=True)
        snapshots = [json.loads(line) for line in run.stdout.splitlines()]
        assert len(snapshots) == len(steps)
        cases.append({'operations':steps, 'snapshots':snapshots})
    result = {'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),
              'header_sha256':hashlib.sha256(header.read_bytes()).hexdigest(), 'cases':cases}
    (here / 'cases.json').write_text(json.dumps(result,separators=(',', ':'))+'\n')
    print(f'Generated {len(cases)} sequences, {sum(len(c["operations"]) for c in cases)} status snapshots')
