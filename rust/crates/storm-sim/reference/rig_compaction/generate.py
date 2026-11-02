"""Unedited SAIL/ROPE/FLAG/VANT deletion and buffer-release macros under UBSan.
Minimal owned metadata, renderer allocation/fill hooks and pointer tags are
explicit ports; this does not evaluate geometry fills or allocation failures.
"""
from pathlib import Path
import hashlib,json,subprocess,tempfile
here=Path(__file__).resolve().parent
repo=next(p for p in here.parents if (p/'src/libs/rigging/src/sail.cpp').exists())
rig=repo/'src/libs/rigging/src'
def method(path,start):
 s=path.read_text();a=s.index(start);return s[a:s.index('\n}',a)+2]
parts={name:method(rig/file,start) for name,file,start in [('sail','sail.cpp','void SAIL::DeleteSailGroup()'),('rope','rope.cpp','void ROPE::DoSTORM_DELETE()'),('flag','flag.cpp','void FLAG::DoSTORM_DELETE()'),('vant','vant.cpp','void VANT_BASE::DoSTORM_DELETE()')]}
s=(repo/'src/libs/renderer/include/dx9render.h').read_text();a=s.index('#define VERTEX_BUFFER_RELEASE');parts['buffer_macros']=s[a:s.index('//\n',a)]
s=(repo/'src/libs/core/include/core.h').read_text();a=s.index('#define STORM_DELETE');parts['delete_macro']=s[a:s.index('#define RELEASE',a)]
driver=r'''
#include <algorithm>
#include <array>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>
#define D3DUSAGE_DYNAMIC 1
#define D3DUSAGE_WRITEONLY 2
#define SAILVERTEX_FORMAT 0
#define ROPEVERTEX_FORMAT 1
#define FLAGLXVERTEX_FORMAT 2
#define VANTVERTEX_FORMAT 3
struct CVECTOR{float x,y,z;};
struct SAILVERTEX{CVECTOR pos,norm;float tu1,tv1,tu2,tv2,tu3,tv3;};
struct ROPEVERTEX{CVECTOR pos;float tu,tv;};using FLAGLXVERTEX=ROPEVERTEX;using VANTVERTEX=ROPEVERTEX;
std::vector<std::vector<uint64_t>> events;
std::array<SAILVERTEX,65536> storage;
struct Renderer{int next=100;bool lock=true;
 void ReleaseVertexBuffer(int id){events.push_back({0,uint64_t(id)});}
 void ReleaseIndexBuffer(int id){events.push_back({1,uint64_t(id)});}
 int CreateVertexBuffer(int,uint32_t bytes,int usage){int id=next++;events.push_back({2,uint64_t(id),bytes,uint64_t((usage&D3DUSAGE_DYNAMIC)!=0)});return id;}
 int CreateIndexBuffer(uint32_t bytes){int id=next++;events.push_back({3,uint64_t(id),bytes});return id;}
 void*LockVertexBuffer(int id){events.push_back({4,uint64_t(id)});return lock?storage.data():nullptr;}
 void UnLockVertexBuffer(int id){events.push_back({5,uint64_t(id)});}
};
__MACROS__
struct Item{uint32_t id;int HostNum,HostGroup;bool bDeleted;uint32_t sv,nv,st,nt;struct{uint32_t sVert,nVert,sIndx,nIndx;}ss;
 void FillVertex(SAILVERTEX*p){events.push_back({6,id,uint64_t(p-storage.data())});}
 void SetTexGrid(SAILVERTEX*p){events.push_back({7,id,uint64_t(p-storage.data())});}
};using SAILONE=Item;using FLAGDATA=Item;
struct GROUPDATA {uint64_t owner;bool bDeleted;int sailQuantity,ropeQuantity,vantQuantity;int*sailIdx,*ropeIdx,*vantIdx;int32_t sv,nv,st,nt,sVert,nVert,sIndx,nIndx;};
struct Base{
 bool bUse=true,bDeleteState=true,bYesDeleted=true,verticesNeedUpdate_=false;
 int groupQuantity=0,sailQuantity=0,ropeQuantity=0,flagQuantity=0,vantQuantity=0,wFirstIndx=17,wRopeLast=17,wFlagLast=17,wVantLast=17;
 GROUPDATA*gdata=nullptr;Item**slist=nullptr,**rlist=nullptr,**flist=nullptr,**vlist=nullptr;
 int32_t vBuf=13,iBuf=17;uint32_t nVert=0,nIndx=0;struct{int32_t vertBuf=13;uint32_t nVert=0;}sg;
 Renderer*RenderService;
 void SetVertexes(){events.push_back({8});}void SetIndex(){events.push_back({9});}void SetTreangle(){events.push_back({10});}void SetTextureCoordinate(){events.push_back({11});}
};
struct SAIL:Base{void DeleteSailGroup();};struct ROPE:Base{void DoSTORM_DELETE();};struct FLAG:Base{void DoSTORM_DELETE();};struct VANT_BASE:Base{void DoSTORM_DELETE();};
__METHODS__
void array(const auto&values){std::cout<<"[";bool first=true;for(auto v:values){if(!first)std::cout<<",";first=false;std::cout<<v;}std::cout<<"]";}
int&quantity(Base&b,int k){return k==0?b.sailQuantity:k==1?b.ropeQuantity:k==2?b.flagQuantity:b.vantQuantity;}
Item**&items(Base&b,int k){return k==0?b.slist:k==1?b.rlist:k==2?b.flist:b.vlist;}
void snapshot(Base&b,Renderer&r,int k){std::cout<<"{\"groups\":[";for(int gn=0;gn<b.groupQuantity;gn++){if(gn)std::cout<<",";auto&g=b.gdata[gn];std::cout<<"["<<g.owner<<","<<(g.bDeleted?"true":"false")<<",";if(k==2)std::cout<<"[]";else{int count=k==0?g.sailQuantity:k==1?g.ropeQuantity:g.vantQuantity;int*p=k==0?g.sailIdx:k==1?g.ropeIdx:g.vantIdx;array(std::vector<int>(p,p+count));}std::cout<<",";if(k==1)array(std::array<int,4>{g.sv,g.nv,g.st,g.nt});else if(k==3)array(std::array<int,4>{g.sVert,g.nVert,g.sIndx,g.nIndx});else std::cout<<"null";std::cout<<"]";}
 std::cout<<"],\"items\":[";for(int n=0;n<quantity(b,k);n++){if(n)std::cout<<",";auto*p=items(b,k)[n];if(!p){std::cout<<"null";continue;}std::cout<<"["<<p->id<<","<<(k==0?p->HostNum:p->HostGroup)<<","<<(p->bDeleted?"true":"false")<<","<<(k==0?p->ss.nVert:p->nv)<<","<<p->nt<<","<<(k==0?p->ss.sVert:p->sv)<<","<<(k==0?p->ss.sIndx:p->st)<<"]";}
 std::cout<<"],\"vertices\":"<<(k==0?b.sg.nVert:b.nVert)<<",\"triangles\":";if(k==0)std::cout<<"null";else std::cout<<b.nIndx;std::cout<<",\"active\":"<<(b.bUse?"true":"false")<<",\"pending\":"<<((k==0?b.bDeleteState:b.bYesDeleted)?"true":"false")<<",\"last\":"<<(k==0?b.wFirstIndx:k==1?b.wRopeLast:k==2?b.wFlagLast:b.wVantLast)<<",\"update\":"<<(b.verticesNeedUpdate_?"true":"false")<<",\"vertex_buffer\":"<<(k==0?b.sg.vertBuf:b.vBuf)<<",\"index_buffer\":"<<b.iBuf<<",\"next_buffer\":"<<r.next<<",\"lock\":"<<(r.lock?"true":"false")<<",\"events\":[";for(size_t i=0;i<events.size();i++){if(i)std::cout<<",";array(events[i]);}std::cout<<"]}";
}
int main(int argc,char**argv){int k=std::stoi(argv[1]),profile=std::stoi(argv[2]);SAIL sail;ROPE rope;FLAG flag;VANT_BASE vant;Base&b=k==0?static_cast<Base&>(sail):k==1?static_cast<Base&>(rope):k==2?static_cast<Base&>(flag):static_cast<Base&>(vant);Renderer render;render.lock=profile%8!=0;b.RenderService=&render;b.groupQuantity=profile%5;b.gdata=new GROUPDATA[b.groupQuantity]{};std::vector<Item*>list;uint32_t id=1;
 for(int gn=0;gn<b.groupQuantity;gn++){auto&g=b.gdata[gn];g.owner=101+gn;g.bDeleted=((profile>>(gn+1))&1)!=0;g.sv=g.sVert=300+gn;g.nv=g.nVert=400+gn;g.st=g.sIndx=500+gn;g.nt=g.nIndx=600+gn;std::vector<int>members;int count=(profile/3+gn*3)%4;
  for(int n=0;n<count;n++,id++){auto*p=new Item{};p->id=id;p->HostNum=p->HostGroup=gn;p->bDeleted=(profile+id)%5==0;p->nv=p->ss.nVert=3+((profile+id)*7)%40;p->nt=1+((profile+id)*3)%12;p->sv=p->ss.sVert=33+id;p->st=p->ss.sIndx=55+id;
   if(k==2 && id%4==0)p->nv=p->ss.nVert=0;
   if(k==2 && id%7==0){delete p;p=nullptr;}
   if(p){b.nVert+=p->nv;b.nIndx+=p->nt;}
   members.push_back(list.size());list.push_back(p);
  }
  int*idx=new int[members.size()];std::copy(members.begin(),members.end(),idx);g.sailQuantity=g.ropeQuantity=g.vantQuantity=members.size();if(k==0)g.sailIdx=idx;else if(k==1)g.ropeIdx=idx;else if(k==3)g.vantIdx=idx;else delete[]idx;
 }
 quantity(b,k)=list.size();items(b,k)=new Item*[list.size()];std::copy(list.begin(),list.end(),items(b,k));b.sg.nVert=b.nVert;if(profile%7==0)b.nVert+=17;if(profile%4==0)b.vBuf=b.sg.vertBuf=-1;if(profile%6==0)b.iBuf=-1;b.verticesNeedUpdate_=profile%11==0;
 std::cout<<"[";for(int step=0;step<3;step++){if(step)std::cout<<",";if(step==1&&b.groupQuantity>0)b.gdata[b.groupQuantity-1].bDeleted=true;b.bDeleteState=b.bYesDeleted=true;std::cout<<"{\"before\":";snapshot(b,render,k);if(k==0)sail.DeleteSailGroup();else if(k==1)rope.DoSTORM_DELETE();else if(k==2)flag.DoSTORM_DELETE();else vant.DoSTORM_DELETE();std::cout<<",\"after\":";snapshot(b,render,k);std::cout<<"}";}std::cout<<"]";
}
'''
driver=driver.replace('__MACROS__',parts['buffer_macros']+'\n'+parts['delete_macro']).replace('__METHODS__','\n'.join(parts[k] for k in ['sail','rope','flag','vant']))
with tempfile.TemporaryDirectory(prefix='storm-rig-compaction-') as folder:
 p=Path(folder);(p/'main.cpp').write_text(driver)
 subprocess.run(['clang++','-std=c++20','-fsanitize=undefined','-fno-sanitize-recover=all',str(p/'main.cpp'),'-o',str(p/'oracle')],check=True)
 cases=[{'kind':k,'profile':i,'snapshots':json.loads(subprocess.check_output([str(p/'oracle'),str(k),str(i)],text=True))} for k in range(4) for i in range(64)]
(here/'cases.json').write_text(json.dumps({'scope':__doc__,'retained_sha256':{k:hashlib.sha256(v.encode()).hexdigest() for k,v in parts.items()},'cases':cases},separators=(',',':'))+'\n')
print('256 native rigging profiles / 768 stateful compaction and buffer-intent snapshots under UBSan')
