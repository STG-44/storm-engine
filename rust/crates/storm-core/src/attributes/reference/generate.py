"""Generate portable fixtures from retained ATTRIBUTES/STRING_CODEC C++ code.

Run from any directory with Python 3 and clang++. Only platform shims and the
debugger's VariableChanged callback are stubbed; attribute methods are unedited.
"""
from pathlib import Path
import json
import struct
import subprocess
import tempfile

here = Path(__file__).resolve().parent
repo = next(p for p in here.parents if (p / "src/libs/core/src/attributes.cpp").is_file())

cases = [
    [
        ["root", "r"], ["create", "r", "a", "FoO", None],
        ["create", "r", "duplicate", "foo", ""], ["set", "r", "fOo", "12"],
        ["get", "r", "FOO"], ["verify", "r", "verified", "Bar"],
        ["create_path", "r", "hole", "ship.sails.rey_b2.s1.hd"],
        ["value", "hole", "37"], ["set", "r", "ship.sails", "literal"],
        ["get", "r", "SHIP.SAILS"], ["find", "r", "found", "SHIP.SAILS.rey_b2.S1.HD"],
        ["same", "found", "hole"], ["parent", "a", "r"],
        ["set", "r", "foo", None], ["get", "r", "foo"],
        ["rename", "verified", "RENAMED"], ["names"],
    ],
    [
        ["root", "r"], ["find", "r", "same_root", ""], ["same", "r", "same_root"],
        ["create_path", "r", "empty", ""], ["create_path", "r", "double", "."],
        ["create_path", "r", "trailing", "ship."], ["find", "r", "ship", "ship."],
        ["parent", "trailing", "ship"], ["find", "r", "first", "."],
        ["same", "first", "empty"], ["find", "r", "second", ".."],
        ["same", "second", "double"], ["get", "ship", ""],
        ["names"], ["find", "r", "missing", "Unseen.NeverInterned"],
        ["names"], ["get", "r", "AnotherNeverInterned"], ["names"],
    ],
    [
        ["root", "r"], ["value", "r", "root-value"],
        ["create", "r", "watched", "watched", "initial"], ["break", "watched", True],
        ["value", "watched", "same"], ["value", "watched", "same"], ["breaks"],
        ["set", "r", "WATCHED", "direct-write"], ["breaks"], ["break", "r", True],
        ["copy", "r", "clone"], ["find", "clone", "copy_child", "watched"],
        ["value", "copy_child", "copied-write"], ["breaks"], ["delete", "r", "r"],
        ["breaks"], ["value", "r", "still-alive"], ["breaks"],
    ],
    [
        ["root", "r"], ["create_path", "r", "leaf", "a.b.c"],
        ["value", "leaf", "original"], ["find", "r", "subtree", "a.b"],
        ["copy", "subtree", "clone"], ["find", "r", "a", "a"],
        ["delete", "r", "a"], ["find", "r", "gone", "a.b"],
        ["delete", "r", "clone"], ["create", "r", "last", "last", ""],
    ],
    [
        ["root", "r"], ["dword", "r", "index", 7], ["dword", "r", "index", 8],
        ["dword", "r", "other", 19], ["dword", "r", None, 4294967295],
        ["float", "r", "float", 0.00009999999], ["float", "r", None, 999999.5],
        ["create", "r", "unset", "unset", None], ["verify", "r", "blank", "blank"],
        ["get_dword", "r", "unset", 99], ["get_dword", "r", "blank", 99],
        ["get_dword", "r", "missing", 99], ["get_dword", "r", None, 0],
    ],
]

def literal(value):
    if value is None:
        return "nullptr"
    return json.dumps(str(value), ensure_ascii=True)

def statement(op):
    kind, *a = op
    node = lambda x: "ids.at(" + literal(x) + ")"
    if kind == "root":
        return f'ids[{literal(a[0])}]=new ATTRIBUTES(*codec); roots.push_back(ids.at({literal(a[0])})); number(ids.at({literal(a[0])})->GetThisNameCode());'
    if kind in ("create", "verify", "create_path", "find", "copy"):
        parent, alias, *tail = a
        if kind == "create": expr = f'{node(parent)}->CreateAttribute({literal(tail[0])}, {literal(tail[1])})'
        elif kind == "verify": expr = f'{node(parent)}->VerifyAttributeClass({literal(tail[0])})'
        elif kind == "create_path": expr = f'{node(parent)}->CreateSubAClass({node(parent)}, {literal(tail[0])})'
        elif kind == "find": expr = f'{node(parent)}->FindAClass({node(parent)}, {literal(tail[0])})'
        else: expr = f'new ATTRIBUTES({node(parent)}->Copy())'
        line = f'ids[{literal(alias)}]={expr}; code(ids.at({literal(alias)}));'
        if kind == "copy": line += f' roots.push_back(ids.at({literal(alias)}));'
        return line
    if kind == "set": return f'number({node(a[0])}->SetAttribute({literal(a[1])},{literal(a[2])}));'
    if kind == "get": return f'value({node(a[0])}->GetAttribute({literal(a[1])}));'
    if kind == "value": return f'{node(a[0])}->SetValue({literal(a[1])}); null_result();'
    if kind == "rename": return f'{node(a[0])}->SetName({literal(a[1])}); code({node(a[0])});'
    if kind == "parent": return f'boolean({node(a[0])}->GetParent()=={node(a[1])});'
    if kind == "same": return f'boolean({node(a[0])}=={node(a[1])});'
    if kind == "break": return f'{node(a[0])}->SetBreak({str(a[1]).lower()}); null_result();'
    if kind == "breaks": return 'number(changes);'
    if kind == "names": return 'number(codec->GetNum());'
    if kind == "delete": return f'boolean({node(a[0])}->DeleteAttributeClassX({node(a[1])}));'
    if kind == "dword": return f'boolean({node(a[0])}->SetAttributeUseDword({literal(a[1])},{a[2]}u));'
    if kind == "float": return f'boolean({node(a[0])}->SetAttributeUseFloat({literal(a[1])},static_cast<float>({a[2]})));'
    if kind == "get_dword": return f'number({node(a[0])}->GetAttributeAsDword({literal(a[1])},{a[2]}));'
    raise ValueError(op)

parses = [
    "", " ", "0", "-0", "+0", "-1", "19", " \t-37trailing", "1.9", "1e2",
    "1e", "1e+", ".5x", "-.5", "+.5e-2end", "0.0000001", "3.14159265358979",
    "2147483647", "-2147483648", "4294967295", "-4294967295", "000012",
    "+", "-", "--1", "bad", " + 1", "1,25", "\r\n12.5", "6.02214076e23",
    "1e999", "-1e999", "1e-999", "-1e-999", "inf", "-INFINITY", "nan", "NaN(payload)", "-nan",
]
bits = [0, 0x80000000, 1, 0x7f7fffff, 0x00800000, 0x3f800000, 0x497423f8, 0x38d1b717]
seed = 0x541227ff
for _ in range(2048):
    seed = (1664525 * seed + 1013904223) & 0xffffffff
    if (seed & 0x7f800000) != 0x7f800000:
        bits.append(seed)

source = r'''
#include <memory>
#include <cassert>
#include <cctype>
#include <cstdlib>
#include <cstring>
#include <cmath>
#include <cstdio>
#include <map>
#include <sstream>
#include <iostream>
#include "attributes.h"
#include "ATTRIBUTES_SOURCE"
#include "string_codec.h"
static uint64_t changes=0;
void STRING_CODEC::VariableChanged() { ++changes; }
static std::string quote(const std::string &s) {
 std::string r="\""; for(unsigned char c:s) {
  if(c=='"'||c=='\\') {r+='\\';r+=c;}
  else if(c<32) {char b[7];std::snprintf(b,sizeof(b),"\\u%04x",c);r+=b;}
  else r+=c;
 } return r+'"';
}
static std::string snapshot(ATTRIBUTES *p) {
 std::ostringstream o;o<<"{\"name\":"<<quote(p->GetThisName())<<",\"code\":"<<p->GetThisNameCode()<<",\"value\":";
 if(p->HasValue())o<<quote(p->GetValue());else o<<"null";
 o<<",\"children\":[";for(size_t i=0;i<p->GetAttributesNum();++i){if(i)o<<',';o<<snapshot(p->GetAttributeClass(static_cast<uint32_t>(i)));}o<<"]}";return o.str();
}
static std::vector<std::string> results;
static void number(uint64_t n){results.push_back(std::to_string(n));}
static void boolean(bool b){results.push_back(b?"true":"false");}
static void null_result(){results.push_back("null");}
static void value(const char*p){results.push_back(p?quote(p):"null");}
static void code(ATTRIBUTES*p){if(p)number(p->GetThisNameCode());else null_result();}
static std::string array(const std::vector<std::string>&v){std::string r="[";for(size_t i=0;i<v.size();++i){if(i)r+=',';r+=v[i];}return r+']';}
int main(){
 std::cout<<"{\"cases\":[";
 CASES
 std::cout<<"],\"parses\":[";
 auto *codec=new STRING_CODEC;
 ATTRIBUTES attr(*codec);
 PARSES
 std::cout<<"],\"formats\":[";
 FORMATS
 std::cout<<"]}\n";
}
'''.replace("ATTRIBUTES_SOURCE", str(repo / "src/libs/core/src/attributes.cpp"))

blocks = []
for index, operations in enumerate(cases):
    block = '{ changes=0;results.clear();auto*codec=new STRING_CODEC;std::map<std::string,ATTRIBUTES*>ids;std::vector<ATTRIBUTES*>roots;\n'
    block += "\n".join(statement(op) for op in operations)
    block += '\nstd::vector<std::string>trees;for(auto*p:roots)trees.push_back(snapshot(p));'
    block += ('std::cout<<",";' if index else '') + 'std::cout<<"{\\"results\\":"<<array(results)<<",\\"trees\\":"<<array(trees)<<",\\"names\\":"<<codec->GetNum()<<",\\"breaks\\":"<<changes<<"}";}\n'
    blocks.append(block)
source = source.replace("CASES", "\n".join(blocks))
blocks = []
for index, text in enumerate(parses):
    blocks.append(('std::cout<<",";' if index else '') + '{ attr.SetValue(' + literal(text) + ');float f=attr.GetAttributeAsFloat();uint32_t b;std::memcpy(&b,&f,4);std::cout<<"{\\"dword\\":"<<attr.GetAttributeAsDword()<<",\\"float_bits\\":"<<b<<",\\"nan\\":"<<(std::isnan(f)?"true":"false")<<"}";}')
source = source.replace("PARSES", "\n".join(blocks))
blocks = []
for index, value in enumerate(bits):
    blocks.append(('std::cout<<",";' if index else '') + '{ uint32_t b=' + str(value) + 'u;float f;std::memcpy(&f,&b,4);attr.SetAttributeUseFloat(nullptr,f);std::cout<<quote(attr.GetValue());}')
source = source.replace("FORMATS", "\n".join(blocks))

with tempfile.TemporaryDirectory(prefix="storm-attributes-reference-") as temporary:
    temp = Path(temporary)
    (temp / "platform").mkdir()
    (temp / "storm_assert.h").write_text("#pragma once\n#include <cassert>\n#define Assert(x) assert(x)\n")
    (temp / "platform/platform.hpp").write_text(r'''#pragma once
#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <cassert>
template<size_t N> int sprintf_s(char(&out)[N], const char*fmt,...) {va_list v;va_start(v,fmt);int r=vsnprintf(out,N,fmt,v);va_end(v);return r;}
template<size_t N> int strncpy_s(char(&out)[N],const char*s,int n) {assert(n>=0 && static_cast<size_t>(n)<N);memcpy(out,s,n);out[n]=0;return 0;}
''')
    (temp / "main.cpp").write_text(source)
    subprocess.run(["clang++", "-std=c++20", "-w", "-I"+str(temp), "-I"+str(repo/"src/libs/core/include"), "-I"+str(repo/"src/libs/core/src"), "-I"+str(repo/"src/libs/util/include"), str(temp/"main.cpp"), "-o", str(temp/"reference")], check=True)
    result = json.loads(subprocess.check_output([str(temp/"reference")], text=True))
for row, operations in zip(result["cases"], cases):
    row["operations"] = operations
for row, text in zip(result["parses"], parses):
    row["input"] = text
result["formats"] = [{"bits": b, "text": text} for b, text in zip(bits, result["formats"])]
result["scope"] = "Retained ATTRIBUTES.cpp and STRING_CODEC; ASCII names, defined decimal numeric prefixes, native LP64 low-32-bit DWORD behavior, six-significant-digit float formatting. C++ platform/debugger glue only is stubbed. Does not assert script VM or savegame compatibility."
(here / "cases.json").write_text(json.dumps(result, ensure_ascii=False, separators=(",", ":")) + "\n")
print("Generated", len(cases), "tree sequences,", len(parses), "numeric prefixes and", len(bits), "float formats")
