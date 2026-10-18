"""Build numeric golden cases from unedited retained DATA methods.

Only DATA storage/platform scaffolding is replaced. Original Convert, Set,
arithmetic, comparisons and boolean methods are extracted verbatim. fmt and
fast_float paths outside the tested subset throw, rather than supplying an
invented oracle. No original game data, Rust executable or network is needed.
"""
from pathlib import Path
import hashlib
import json
import subprocess
import tempfile

here = Path(__file__).resolve().parent
repo = next(p for p in here.parents if (p / "src/libs/core/src/data.cpp").is_file())
data = (repo / "src/libs/core/src/data.cpp").read_text()
tokens = (repo / "src/libs/core/src/token.h").read_text()

def extract(source, signature):
    start = source.index(signature)
    opening = source.index("{", start)
    depth = 1
    p = opening + 1
    while depth:
        if source[p] == "{": depth += 1
        if source[p] == "}": depth -= 1
        p += 1
    return source[start:p]

methods = [
    "void DATA::Set(int32_t value)", "void DATA::Set(float value)",
    "void DATA::Set(std::string value)", "void DATA::Set(const char *value)",
    "bool DATA::Convert(S_TOKEN_TYPE type)", "bool DATA::Neg()", "bool DATA::Inverse()",
    "bool DATA::Power(DATA *pV)", "bool DATA::Power(int32_t Deg)",
    "bool DATA::Multiply(DATA *pV)", "bool DATA::Divide(DATA *pV)",
    "bool DATA::Modul(DATA *pV)", "bool DATA::Plus(DATA *pV)",
    "bool DATA::Minus(DATA *pV)", "bool DATA::CompareAndSetResult(DATA *pV, S_TOKEN_TYPE op)",
    "bool DATA::BoolConvert()",
]
enum = extract(tokens, "enum S_TOKEN_TYPE") + ";\n"
source = r'''
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <cstdio>
#include <charconv>
#include <cstring>
#include <string>
#include <stdexcept>
#include "string_compare.hpp"
using std::isnan;
std::string to_string(const char* s) {return s;}
namespace fast_float {
void from_chars(const char*, const char*, float&) {throw std::runtime_error("untested float string conversion");}
}
namespace fmt {
std::string format(const char*, float) {throw std::runtime_error("untested float concatenation");}
}
#define UNINIT_REF "uninitialized reference"
#define INVALID_CONVERSATION "invalid conversion"
#define NO_INDEX "array index required"
struct ATTRIBUTES {const char* GetThisAttr(){return nullptr;}};
'''
source += enum
source += r'''
struct DATA {
    S_TOKEN_TYPE Data_type=VAR_INTEGER;
    bool bArray=false;
    int32_t lValue=0;
    float fValue=0;
    uintptr_t pValue=0;
    std::string sValue;
    DATA* pReference=nullptr;
    ATTRIBUTES* AttributesClass=nullptr;
    void Error(const char* s){throw std::runtime_error(s);}
    bool IsArray(){return bArray;}
    DATA* GetVarPointer(){return this;}
    S_TOKEN_TYPE GetType(){return Data_type;}
    bool Get(int32_t& n){n=lValue; return true;}
    void Set(int32_t);void Set(float);void Set(std::string);void Set(const char*);
    bool Convert(S_TOKEN_TYPE);bool Neg();bool Inverse();bool Power(DATA*);bool Power(int32_t);
    bool Multiply(DATA*);bool Divide(DATA*);bool Modul(DATA*);bool Plus(DATA*);bool Minus(DATA*);
    bool CompareAndSetResult(DATA*,S_TOKEN_TYPE);bool BoolConvert();
};
'''
source += "\n\n".join(extract(data, signature) for signature in methods)
source += r'''
void run(const char* op, char at, const char* av, char bt, const char* bv) {
    DATA a,b;
    auto init=[](DATA& d,char t,const char* v){
        if(t=='i')d.Set(static_cast<int32_t>(strtol(v,nullptr,10)));
        else if(t=='f')d.Set(strtof(v,nullptr)); else d.Set(v);
    };
    init(a,at,av);init(b,bt,bv);
    bool ok=true;
    std::string operation(op);
    if(operation=="+")ok=a.Plus(&b);else if(operation=="-")ok=a.Minus(&b);
    else if(operation=="*")ok=a.Multiply(&b);else if(operation=="/")ok=a.Divide(&b);
    else if(operation=="%")ok=a.Modul(&b);else if(operation=="^")ok=a.Power(&b);
    else if(operation=="neg")ok=a.Inverse();else if(operation=="!")ok=a.Neg();
    else if(operation=="bool")ok=a.BoolConvert();
    else {
        S_TOKEN_TYPE token=OP_BOOL_EQUAL;
        if(operation=="!=")token=OP_NOT_EQUAL;else if(operation=="<")token=OP_LESSER;
        else if(operation=="<=")token=OP_LESSER_OR_EQUAL;else if(operation==">")token=OP_GREATER;
        else if(operation==">=")token=OP_GREATER_OR_EQUAL;
        // The original method reports false even after computing the result.
        a.CompareAndSetResult(&b,token);
    }
    if(!ok)throw std::runtime_error("unsupported oracle case");
    if(a.Data_type==VAR_INTEGER)printf("i %d\n",a.lValue);
    else if(a.Data_type==VAR_FLOAT){uint32_t n;memcpy(&n,&a.fValue,4);printf("f %u\n",n);}
    else printf("s %s\n",a.sValue.c_str());
}
int main(){
'''

values = [["i", str(n)] for n in [-32768, -19, -7, -1, 0, 1, 2, 7, 19, 32767, 16777217]]
values += [["f", str(n)] for n in [-19.5, -2.5, -0.5, -0.0, 0.5, 1.5, 2.5, 19.5, 16777216.0]]
cases = []
for a in values:
    for b in values:
        for op in ["+", "-", "*", "/", "%", "==", "!=", "<", "<=", ">", ">="]:
            if op == "*" and a[0] == b[0] == "i" and not -2147483648 <= int(a[1])*int(b[1]) <= 2147483647: continue
            if op in ["/", "%"] and float(b[1]) == 0: continue
            if op == "%" and a[0] == "i" and b[0] == "f" and int(float(b[1]) // 1) == 0: continue
            cases.append({"a":a,"b":b,"op":op})
    for op in ["neg", "!", "bool"]:
        cases.append({"a":a,"b":["i","0"],"op":op})
    for exponent in [-5, -3, -2, -1, 0, 1, 2, 3, 5]:
        if exponent < -1 and float(a[1]) == 0: continue
        # Avoid signed C++ overflow; wrapping behavior is tested separately.
        if a[0] == "i" and exponent > 1 and abs(int(a[1])) ** exponent > 2147483647: continue
        cases.append({"a":a,"b":["i",str(exponent)],"op":"^"})
strings = ["", "Sails", "sAILS", "mast", "12tail", " 12", "+12", "-12tail", "2147483648", "_", "["]
for s in strings:
    for op in ["neg", "!", "bool"]:
        cases.append({"a":["s",s],"b":["i","0"],"op":op})
    for other in strings:
        for op in ["+", "==", "!=", "<", "<=", ">", ">="]:
            cases.append({"a":["s",s],"b":["s",other],"op":op})
    for n in [-12, 0, 12]:
        for op in ["+", "==", "!=", "<", "<=", ">", ">="]:
            cases.append({"a":["s",s],"b":["i",str(n)],"op":op})
        cases.append({"a":["i",str(n)],"b":["s",s],"op":"+"})

literal = lambda value: json.dumps(value, ensure_ascii=True)
for case in cases:
    a, b = case["a"], case["b"]
    source += f'run({literal(case["op"])},\'{a[0]}\',{literal(a[1])},\'{b[0]}\',{literal(b[1])});\n'
source += "}\n"
with tempfile.TemporaryDirectory(prefix="storm-script-reference-") as directory:
    root = Path(directory)
    (root / "reference.cpp").write_text(source)
    subprocess.run(["clang++", "-std=c++20", "-O0", "-Wno-switch", "-Wno-deprecated-declarations",
                    "-fsanitize=undefined", "-fno-sanitize-recover=all",
                    "-I", str(repo / "src/libs/util/include"), str(root / "reference.cpp"),
                    "-o", str(root / "reference")], check=True)
    output = subprocess.check_output([str(root / "reference")], text=True).splitlines()
assert len(output) == len(cases)
for case, line in zip(cases, output):
    kind, value = line.split(" ", 1)
    case["result"] = [kind, value if kind == "s" else int(value)]
fixture = {"source":"retained src/libs/core/src/data.cpp methods", "methods":methods,
           "methods_sha256":hashlib.sha256("\n\n".join(extract(data,s) for s in methods).encode()).hexdigest(),
           "cases":cases}
(here / "cases.json").write_text(json.dumps(fixture, separators=(",", ":")) + "\n")
print(f"Generated {len(cases)} retained-C++ scalar cases")
