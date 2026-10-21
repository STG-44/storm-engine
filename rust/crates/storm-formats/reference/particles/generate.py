"""Retained unedited XPS field loaders and graph evaluators, with memory/type adapters.

Schema Convert, field-list/type dispatch, C rand, particle dynamics and renderer
are outside this oracle. Synthetic bytes contain no commercial resources.
"""
from pathlib import Path
import struct,json,subprocess,tempfile,hashlib
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/particles').is_dir())
base=repo/'src/libs/particles/src/system/data_source'
def method(source,sig):
 start=source.index(sig);return source[start:source.index('\n}',start)+2]
bodies={}
for cls in ['Bool','Float','Position','String','Graph','UV','Color']:
 source=(base/('data_'+cls.lower()+'.cpp')).read_text()
 sig='void Data'+cls+'::Load(MemFile *File)';bodies[sig]=method(source,sig)
source=(base/'data_graph.cpp').read_text()
for sig in ['float Lerp(', 'float DataGraph::GetMinAtTime(', 'float DataGraph::GetMaxAtTime(', 'float DataGraph::GetValue(', 'float RandomRange(', 'float DataGraph::GetRandomValue(']:bodies[sig]=method(source,sig)
bodies['Color DataColor::GetValue(']=method((base/'data_color.cpp').read_text(),'Color DataColor::GetValue(')
bodies['inline Color &Color::Lerp(']=method((repo/'src/libs/math/include/math3d/color.h').read_text(),'inline Color &Color::Lerp(')
def u(n):return struct.pack('<I',n)
def f(n):return struct.pack('<f',n)
def name(s):b=s.encode()+b'\0';return u(len(b))+b
def graph(negative,relative):
 points=[(0,-2),(0.0005,4),(1,8),(100,12)]
 return u(negative)+u(relative)+u(len(points))+b''.join(f(t)+f(v) for t,v in points)+u(3)+b''.join(f(t)+f(v) for t,v in [(0,1),(1,3),(100,6)])+name('Curve')
fields=[(1,u(7)+name('Enabled')),(2,f(-2.5)+name('Float')),(4,b''.join(f(v) for v in [1,2,-3])+name('Position')),(5,b'profile\0'+bytes(120)+name('Name')),(6,u(2)+b''.join(f(v) for v in [0.25,0.5,0.25,0.125,0,0,1,1])+name('Texture'))]
for negative in [0,1]:
 for relative in [0,1]:fields.append((3,graph(negative,relative)))
colors=[(0,[1,.5,0,.2],[.25,0,0,.3]),(.0005,[.5,1,0,.4],[0,.25,0,.5]),(1,[0,.5,1,.6],[0,0,.25,.7])]
fields.append((7,u(len(colors))+b''.join(f(t)+b''.join(f(v) for v in high+low) for t,high,low in colors)+name('Color')))
# A complete resource-free container for the Rust reader / CLI / CI.
emitter=[(5,b'emitter\0'+bytes(120)+name('Name')),(2,f(2)+name('Life time')),(4,b''.join(f(v) for v in [0,1,0])+name('Position')),(1,u(1)+name('Looping'))]
def fieldlist(fs):return u(len(fs))+b''.join(u(k)+b for k,b in fs)
profile=b'PSYSv3.5'+u(1)+u(0)+fieldlist(emitter)+u(2)+u(1)+fieldlist(fields)+u(2)+fieldlist([(5,b'model\0'+bytes(122)+name('Model names'))])
(here/'synthetic.xps').write_bytes(profile)
source=r'''
#include <bit>
#include <cstdint>
#include <cstring>
#include <vector>
#include <string>
#include <iostream>
#include <stdexcept>
#include <algorithm>
#define Assert(x) do{if(!(x))throw std::runtime_error("assert");}while(false)
struct MemFile{std::vector<uint8_t> data;size_t pos=0;void Read(void* p,size_t n){Assert(pos+n<=data.size());memcpy(p,data.data()+pos,n);pos+=n;}template<class T>void ReadType(T& p){Read(&p,sizeof(p));}};
struct Vector{float x,y,z;};struct Vector4{float x,y,z,w;};
struct Color{float r=0,g=0,b=0,a=1;Color()=default;Color(uint32_t v){r=float((v>>16)&255)/255;g=float((v>>8)&255)/255;b=float(v&255)/255;a=float(v>>24)/255;}Color& Lerp(const Color&,const Color&,float);};
struct GraphVertex{float Time,Val;};struct ColorVertex{float Time;Color MaxValue,MinValue;};
struct Base{std::string Name;void SetName(const char* s){Name=s;}};
struct DataBool:Base{bool Value=false;void SetValue(bool v){Value=v;}void Load(MemFile*);};
struct DataFloat:Base{float Value=0;void SetValue(float v){Value=v;}void Load(MemFile*);};
struct DataPosition:Base{Vector Value;void SetValue(Vector v){Value=v;}void Load(MemFile*);};
struct DataString:Base{std::string Value;void SetValue(const char* v){Value=v;}void Load(MemFile*);};
struct DataUV:Base{std::vector<Vector4> Frames;void Load(MemFile*);};
struct DataGraph:Base{bool bRelative=false,bNegative=false;float MaxCachedTime=999999999999.0f,MinCachedTime=999999999999.0f;uint32_t MaxCachedIndex=0,MinCachedIndex=0;std::vector<GraphVertex> MinGraph,MaxGraph;
void SetNegative(bool b){bNegative=b;}void SetRelative(bool b){bRelative=b;}void Load(MemFile*);float GetMinAtTime(float,float);float GetMaxAtTime(float,float);float GetValue(float,float,float);float GetRandomValue(float,float);};
struct DataColor:Base{Color ZeroColor{uint32_t(0xffffffff)};std::vector<ColorVertex> ColorGraph;void Load(MemFile*);Color GetValue(float,float,float);};
uint32_t GraphRead=0;
uint32_t random_sample=0;int portable_rand(){return int(random_sample);}
#define rand portable_rand
__METHODS__
void string(const std::string& s){std::cout<<"[";bool first=true;for(unsigned char c:s){if(!first)std::cout<<",";first=false;std::cout<<unsigned(c);}std::cout<<"]";}
void number(float f){std::cout<<std::bit_cast<uint32_t>(f);}
void vector(Vector v){std::cout<<"[";number(v.x);std::cout<<",";number(v.y);std::cout<<",";number(v.z);std::cout<<"]";}
void color(Color c){std::cout<<"[";number(c.r);std::cout<<",";number(c.g);std::cout<<",";number(c.b);std::cout<<",";number(c.a);std::cout<<"]";}
void points(const std::vector<GraphVertex>& ps){std::cout<<"[";bool first=true;for(auto p:ps){if(!first)std::cout<<",";first=false;std::cout<<"[";number(p.Time);std::cout<<",";number(p.Val);std::cout<<"]";}std::cout<<"]";}
void prefix(const char* kind,Base& p){std::cout<<"{\"kind\":\""<<kind<<"\",\"name\":";string(p.Name);std::cout<<",\"value\":";}
void run(int kind,std::vector<uint8_t> bytes){MemFile file{bytes};
if(kind==1){DataBool v;v.Load(&file);prefix("bool",v);std::cout<<(v.Value?"true":"false");}
if(kind==2){DataFloat v;v.Load(&file);prefix("float",v);number(v.Value);}
if(kind==4){DataPosition v;v.Load(&file);prefix("position",v);vector(v.Value);}
if(kind==5){DataString v;v.Load(&file);prefix("string",v);string(v.Value);}
if(kind==6){DataUV v;v.Load(&file);prefix("uv",v);std::cout<<"[";bool first=true;for(auto p:v.Frames){if(!first)std::cout<<",";first=false;std::cout<<"[";number(p.x);std::cout<<",";number(p.y);std::cout<<",";number(p.z);std::cout<<",";number(p.w);std::cout<<"]";}std::cout<<"]";}
if(kind==3){DataGraph v;v.Load(&file);prefix("graph",v);std::cout<<"{\"negative\":"<<(v.bNegative?"true":"false")<<",\"relative\":"<<(v.bRelative?"true":"false")<<",\"max\":";points(v.MaxGraph);std::cout<<",\"min\":";points(v.MinGraph);std::cout<<"},\"samples\":[";bool first=true;
for(float t:{-1.0f,0.0f,0.00025f,0.0005f,0.5f,1.0f,99.0f,100.0f,101.0f,0.1f})for(float life:{1.0f,2.0f})for(float k:{0.0f,0.5f,1.0f}){if(!first)std::cout<<",";first=false;std::cout<<"[";number(t);std::cout<<",";number(life);std::cout<<",";number(k);std::cout<<",";number(v.GetValue(t,life,k));std::cout<<"]";}std::cout<<"],\"range_samples\":[";first=true;for(float t:{-1.0f,0.0f,0.00025f,0.0005f,0.5f,1.0f,99.0f,100.0f,101.0f,0.1f})for(float life:{1.0f,2.0f})for(uint32_t random:{0u,500u,1999u}){random_sample=random;if(!first)std::cout<<",";first=false;std::cout<<"[";number(t);std::cout<<",";number(life);std::cout<<","<<random<<",";number(v.GetRandomValue(t,life));std::cout<<"]";}std::cout<<"]";}
if(kind==7){DataColor v;v.Load(&file);prefix("color",v);std::cout<<"[";bool first=true;for(auto p:v.ColorGraph){if(!first)std::cout<<",";first=false;std::cout<<"{\"time\":";number(p.Time);std::cout<<",\"max\":";color(p.MaxValue);std::cout<<",\"min\":";color(p.MinValue);std::cout<<"}";}std::cout<<"],\"samples\":[";first=true;for(float t:{-1.0f,0.0f,0.00025f,0.0005f,0.5f,1.0f,1.1f})for(float life:{1.0f,2.0f})for(float k:{0.0f,0.5f,1.0f}){if(!first)std::cout<<",";first=false;std::cout<<"[";number(t);std::cout<<",";number(life);std::cout<<",";number(k);std::cout<<",";color(v.GetValue(t,life,k));std::cout<<"]";}std::cout<<"]";}
std::cout<<",\"consumed\":"<<file.pos<<"}\n";}
int main(){__RUNS__}
'''
source=source.replace('__METHODS__','\n'.join(bodies.values())).replace('__RUNS__','\n'.join('run('+str(k)+',{'+','.join(map(str,b))+'});' for k,b in fields))
with tempfile.TemporaryDirectory(prefix='storm-xps-reference-') as folder:
 p=Path(folder);(p/'main.cpp').write_text(source)
 subprocess.run(['clang++','-std=c++20','-ffp-contract=off','-fsanitize=undefined','-fno-sanitize-recover=all',str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
 rows=subprocess.check_output([str(p/'oracle')],text=True).splitlines()
assert len(rows)==len(fields)
cases=[{'kind':k,'bytes':list(b),'expected':json.loads(row)} for (k,b),row in zip(fields,rows)]
(here/'cases.json').write_text(json.dumps({'scope':__doc__,'retained_sha256':{sig:hashlib.sha256(body.encode()).hexdigest() for sig,body in bodies.items()},'fields':cases},indent=2)+'\n')
print(f'{len(cases)} loaded fields; '+str(sum(len(c['expected'].get('samples',[])) for c in cases))+' complete value samples + '+str(sum(len(c['expected'].get('range_samples',[])) for c in cases))+' random-range samples under UBSan')
