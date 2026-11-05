"""Retained native INI storage/parser/lookup and project/default/texture methods.

File I/O, renderer handles, cache writes, manager reset/geometry cache and output
are adapters. This does not execute DataSource loading, GPU shading or live
project switching. Only bounded, well-terminated fixture inputs are compared.
"""
from pathlib import Path
import hashlib,json,subprocess,tempfile,struct
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/particles').is_dir())
ifs=(repo/'src/libs/core/src/ifs.cpp').read_text()
header=(repo/'src/libs/core/include/ifs.h').read_text()
manager=(repo/'src/libs/particles/src/manager/particle_manager.cpp').read_text()
def method(text,signature):
 start=text.index(signature);return text[start:text.index('\n}',start)+2]
bodies={'KEY_NODE/SECTION storage':ifs[ifs.index('KEY_NODE::KEY_NODE()'):ifs.index('IFS::IFS(')]}
for signature in ['bool IFS::VoidSym(','void IFS::Format(','KEY_NODE *IFS::FindKey(const char *section_name, const char *key_name)\n','KEY_NODE *IFS::FindKey(const char *section_name, const char *key_name, const char *key_value)\n','SECTION *IFS::FindSection(const char *section_name)\n','bool IFS::ReadString(']:
 bodies[signature]=method(ifs,signature)
for signature in ['bool ParticleManager::OpenProject(','void ParticleManager::OpenDefaultProject(','void ParticleManager::SetProjectTexture(']:
 bodies[signature]=method(manager,signature)
declarations=header[header.index('#define KNF_KEY'):header.index('class IFS :')]
inputs=[
 b'',
 b'[Textures]\r\nMainTexture = atlas.tga\r\n[Manager]\r\nSystem_0002=c\r\nSystem_0000=a\r\nSystem_0001=b\r\n',
 b'[Manager]\nSystem_0000=a\nSystem_0002=ignored\n',
 b'[Manager]\nSystem_0000=a\nSystem_0001= \t\nSystem_0002=ignored',
 b'[TEXTURES]\nMainTexture=first\nmaintexture=second\n[Manager]\nSystem_0000=a\nSystem_0000=b',
 b'[Textures]\nMainTexture=\nMainTexture=later\n[Manager]\nSystem_0000\nSystem_0001=b',
 b'[Textures ]\nMainTexture=ignored\n[Manager]\nSystem_0000=a\n[manager]\nSystem_0001=b',
 b'[Textures]\nMainTexture="atlas" ; literal\n[Manager]\nSystem_0000=smoke;literal',
 b'; comment\n[Textures] trailing text\nMainTexture=effects/Atlas.TGA\n[Manager]\nSystem_0000= same ',
 b'[Textures]\nMainTexture=resource/textures/atlas.tga.TX\n[Manager]\nSystem_0000=a',
 b'[Textures]\nMainTexture=Particles_3.txx\n[Manager]\nSystem_0000=FortFire .xps',
 b'[Manager]\nSystem_0000=\xe4\xfb\xec\0System_0001=smoke',
 b'[Manager]\nSystem_0000  \nSystem_0001=b',
 b'[Manager]\nSystem_0000=a\n[bad\nSystem_0001=b',
 b'[Textures]\nMainTexture=.tx\n[Manager]\nSystem_0000=a',
]
source=r'''
#include <algorithm>
#include <cstdint>
#include <cstring>
#include <filesystem>
#include <iostream>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>
template<class S> int strcpy_s(char *dst,S n,const char *src){if(strlen(src)+1>size_t(n))throw std::runtime_error("copy bound");memcpy(dst,src,strlen(src)+1);return 0;}
namespace storm {bool iEquals(std::string a,std::string b){std::transform(a.begin(),a.end(),a.begin(),[](unsigned char c){return std::tolower(c);});std::transform(b.begin(),b.end(),b.begin(),[](unsigned char c){return std::tolower(c);});return a==b;}}
struct Core {template<class... T>void Trace(T...) {}};Core core,core_internal;
#define COMMENT ';'
#define SECTION_A '['
#define SECTION_B ']'
#define INI_EQUAL '='
#define VOIDSYMS_NUM 2
const char INI_LINEFEED[3]={13,10,0};const char INI_VOIDSYMS[2]={32,9};
__DECLARATIONS__
struct IFS {
 SECTION *SectionRoot=nullptr,*SectionTop=nullptr;
 ~IFS(){while(SectionRoot){auto *n=SectionRoot->GetRightNode();delete SectionRoot;SectionRoot=n;}}
 bool VoidSym(char);void Format(char*,int32_t);
 KEY_NODE *FindKey(const char*,const char*);KEY_NODE *FindKey(const char*,const char*,const char*);
 SECTION *FindSection(const char*);
 bool ReadString(SEARCH_DATA*,const char*,const char*,char*,uint32_t,const char*);
 bool ReadString(const char *s,const char *k,char *b,uint32_t n,const char *d){SEARCH_DATA search{};return ReadString(&search,s,k,b,n,d);}
};
struct FIO {IFS *ini=nullptr;std::string requested;std::vector<std::string> entries;
 IFS *OpenIniFile(const char *p){requested=std::filesystem::path(p).generic_string();return ini;}
 std::vector<std::string> _GetPathsOrFilenamesByMask(const char*,const char*,bool){return entries;}
} files;FIO *fio=&files;
struct Cache {std::vector<std::string> names;void CacheSystem(const char *name){names.push_back(name);}};
struct Renderer {std::vector<std::string> names;void TextureRelease(int){}int TextureCreate(const char *p){names.push_back(p);return int(names.size());}};
struct ParticleManager {
 std::string ShortProjectName,TextureName;int pProjectTexture=0,pProjectTextureNormalMap=0,closed=0,geom=0;Cache cache;Cache *pDataCache=&cache;Renderer renderer;Renderer *pRS=&renderer;
 void CloseProject(){closed++;cache.names.clear();renderer.names.clear();pProjectTexture=0;pProjectTextureNormalMap=0;}
 void CreateGeomCache(){geom++;}
 bool OpenProject(const char*);void OpenDefaultProject();void SetProjectTexture(const char*);
};
__BODIES__
void bytes(const std::string &v){std::cout<<"[";bool first=true;for(unsigned char b:v){if(!first)std::cout<<",";first=false;std::cout<<unsigned(b);}std::cout<<"]";}
void strings(const std::vector<std::string> &v){std::cout<<"[";bool first=true;for(auto &s:v){if(!first)std::cout<<",";first=false;bytes(s);}std::cout<<"]";}
void output(ParticleManager &m,bool ok){std::cout<<"{\"ok\":"<<(ok?"true":"false")<<",\"texture\":";bytes(m.TextureName);std::cout<<",\"requests\":";strings(m.renderer.names);std::cout<<",\"systems\":";strings(m.cache.names);std::cout<<",\"closed\":"<<m.closed<<",\"geom\":"<<m.geom<<",\"project\":";bytes(m.ShortProjectName);std::cout<<",\"path\":";bytes(files.requested);std::cout<<"}\n";}
void run(std::vector<char> input){IFS ini;input.push_back(0);ini.Format(input.data(),int32_t(input.size()-1));files.ini=&ini;files.requested.clear();ParticleManager m;bool ok=m.OpenProject("fixture");output(m,ok);}
int main(){__RUNS__
 files.ini=nullptr;files.requested.clear();{ParticleManager m;bool ok=m.OpenProject("missing.ini");output(m,ok);}
 files.entries={"Ship_Fire.xps","Ship_Smoke.xps"};files.requested.clear();{ParticleManager m;m.OpenDefaultProject();output(m,true);}
}
'''.replace('__DECLARATIONS__',declarations).replace('__BODIES__','\n'.join(bodies.values())).replace('__RUNS__','\n'.join('run({'+','.join(map(str,((b if b<128 else b-256) for b in data)))+'});' for data in inputs))
with tempfile.TemporaryDirectory(prefix='storm-particle-project-') as folder:
 p=Path(folder);(p/'main.cpp').write_text(source)
 subprocess.run(['clang++','-std=c++20','-fsanitize=undefined','-fno-sanitize-recover=all',str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
 rows=[json.loads(line) for line in subprocess.check_output([str(p/'oracle')],text=True).splitlines()]
(here/'cases.json').write_text(json.dumps({'scope':__doc__,'retained_sha256':{k:hashlib.sha256(v.encode()).hexdigest() for k,v in bodies.items()},'cases':[{'bytes':list(data),'expected':row} for data,row in zip(inputs,rows)],'missing':rows[-2],'default':rows[-1]},indent=2)+'\n')
(here/'project.prj').write_bytes(b'[Textures]\r\nMainTexture=fixture.tga\r\n[Manager]\r\nSystem_0001=smoke\r\nSystem_0000=fire\r\nSystem_0003=ignored_after_gap\r\n')
# A complete generated billboard description for loader/GPU tests, no game data.
u=lambda v:struct.pack('<I',v);f=lambda v:struct.pack('<f',v)
def name(s):b=s.encode()+b'\0';return u(len(b))+b
def field(k,n,data):return u(k)+data+name(n)
def fields(values):return u(len(values))+b''.join(values)
def graph(n,v):return field(3,n,u(1)+u(1)+u(2)+f(0)+f(v)+f(100)+f(v)+u(2)+f(0)+f(v)+f(100)+f(v))
emitter=fields([field(2,'Life time',f(1)),field(4,'Position',f(0)*3),field(1,'Looping',u(1))]+[graph(n,0) for n in ['Physics Dir X','Physics Dir Y','Physics Dir Z']])
particle=[field(2,'Max Particles count',f(8)),field(6,'Texture',u(1)+f(0)+f(0)+f(1)+f(1)),field(7,'Color',u(2)+b''.join(f(t)+f(1)*8 for t in [0,1]))]
for n,v in [('Life time',2),('Emission rate',8),('Size',1),('Transparency',50),('Animation frame',0),('Track X',0),('Track Y',0),('Track Z',0),('Mass',1),('Spin',0),('Velocity',0),('Friction',0),('Spin Friction',0),('ForceField_gravity',0),('Physic blend',0)]:particle.append(graph(n,v))
(here/'billboard.xps').write_bytes(b'PSYSv3.5'+u(1)+u(0)+emitter+u(1)+u(1)+fields(particle))
print(f'{len(rows)} complete native INI/project/default/texture snapshots under UBSan')
