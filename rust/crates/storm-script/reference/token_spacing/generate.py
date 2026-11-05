"""Record lexical boundary spacing from the complete unedited native TOKEN.

Only the Windows compatibility header and output driver are scaffolding. The
reference uses signed char as on the retained desktop compiler targets. This
checks skipped bytes versus quoted/comment contents, not full compiler behavior.
"""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import tempfile

here = Path(__file__).resolve().parent
repo = next(p for p in here.parents if (p / 'src/libs/core/src/token.cpp').is_file())
core = repo / 'src/libs/core/src'
separators = [' ', '\t', '\v', '\f', '\x01', '\x1f', '\u00a0', '\u2003', '\ufeff', 'я', '☀', '\u00a0 \t']
cases = []
for separator in separators:
    cases.append({'source': f'int{separator}main(){{return{separator}7;}}'})
    cases.append({'source': f'int main(){{string s="a{separator}b";/*x{separator}y*/return 3;}}'})
cases += [{'source': 'int\r\nmain(){//я\nreturn\u00a0 7;}'},
          {'source': 'int\u00a0main(){string s="Корсары";return 7;}'}]

driver = r'''
#include "token.h"
#include <iostream>
#include <limits>
#include <string>
#include <vector>
static_assert(std::numeric_limits<char>::is_signed);
void quoted(const std::string& s) {
 std::cout<<'"';const char* hex="0123456789abcdef";
 for(unsigned char c:s) {
  if(c=='"'||c=='\\')std::cout<<'\\'<<char(c);
  else if(c<32)std::cout<<"\\u00"<<hex[c>>4]<<hex[c&15];
  else std::cout<<char(c);
 }
 std::cout<<'"';
}
void scan(std::string text) {
 TOKEN token;token.SetProgram(text.data(),text.data());bool first=true;
 std::cout<<'[';
 for(int n=0;n<1000;n++) {
  auto kind=token.Get(true);
  if(kind==END_OF_PROGRAMM)break;
  if(kind==COMMENT||kind==WHITESPACE||kind==DEBUG_LINEFEED)continue;
  if(!first)std::cout<<',';first=false;
  std::cout<<"{\"kind\":";quoted(kind==STRING?"string":kind==NUMBER?"int":"word");
  std::cout<<",\"text\":";auto data=token.GetData();quoted(kind==SEPARATOR?";":data?data:"");std::cout<<'}';
 }
 std::cout<<"]\n";
}
int main(){
'''
for case in cases:
    raw = case['source'].encode()
    literal = '"' + ''.join(f'\\x{byte:02x}' for byte in raw) + '"'
    driver += f'scan(std::string({literal},{len(raw)}));\n'
driver += '}\n'
with tempfile.TemporaryDirectory(prefix='storm-token-spacing-') as directory:
    root = Path(directory)
    for name in ['token.cpp', 'token.h']:
        shutil.copyfile(core / name, root / name)
    (root / 'windows.h').write_text('''#pragma once
#include <cstdlib>
#include <cstring>
#include <stdexcept>
template<size_t N> void strcpy_s(char(&dst)[N],const char*src){if(strlen(src)>=N)throw std::runtime_error("copy limit");strcpy(dst,src);}
''')
    (root / 'oracle.cpp').write_text(driver)
    subprocess.run(['clang++', '-std=c++20', '-O0', '-fsigned-char', '-fsanitize=undefined',
                    '-fno-sanitize-recover=all', '-I', str(root), '-I', str(repo/'src/libs/util/include'),
                    str(root/'oracle.cpp'), str(root/'token.cpp'), '-o', str(root/'oracle')], check=True)
    lines = subprocess.check_output([str(root/'oracle')], text=True).splitlines()
assert len(lines) == len(cases)
for case, line in zip(cases, lines):
    case['expected'] = json.loads(line)
fixture = {'source': 'Complete unedited TOKEN::Get/StartArgument/ProcessToken; signed char, UBSan',
           'files_sha256': {name: hashlib.sha256((core/name).read_bytes()).hexdigest() for name in ['token.cpp','token.h']},
           'utf8_sha256': hashlib.sha256((repo/'src/libs/util/include/utf8.h').read_bytes()).hexdigest(),
           'cases': cases}
raw = (json.dumps(fixture, ensure_ascii=False, indent=2)+'\n').encode()
(here/'cases.json').write_bytes(raw)
print('Complete TOKEN spacing:', len(cases), 'streams; SHA256', hashlib.sha256(raw).hexdigest())
