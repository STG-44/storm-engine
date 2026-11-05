"""Run the complete original TIMER with an explicit nanosecond clock provider.

Only the std namespace spelling is redirected while including the unchanged
header. chrono operations/types still use the standard library; now() is injected.
"""
from pathlib import Path
import subprocess,tempfile,json,hashlib
root=Path(__file__).resolve().parents[5]
header=root/'src/libs/core/src/timer.h'
source=r'''
#include <chrono>
#include <ratio>
#include <cstdint>
#include <cstring>
#include <iostream>
struct InjectedClock {
    using duration=std::chrono::nanoseconds;
    using rep=duration::rep;
    using period=duration::period;
    using time_point=std::chrono::time_point<InjectedClock>;
    static constexpr bool is_steady=true;
    inline static int64_t ticks=0;
    static time_point now(){return time_point(duration(ticks));}
};
namespace timer_std {
using milli=std::milli;
namespace chrono {
using high_resolution_clock=InjectedClock;
using milliseconds=std::chrono::milliseconds;
template<class Rep,class Period>using duration=std::chrono::duration<Rep,Period>;
template<class Clock>using time_point=std::chrono::time_point<Clock>;
}}
#define std timer_std
#include "HEADER_PATH"
#undef std
uint32_t bits(float n){uint32_t u;std::memcpy(&u,&n,4);return u;}
void state(const TIMER&t){std::cout
 <<"{\"ring\":"<<t.Ring<<",\"fixed\":"<<t.FixedDelta
 <<",\"fixed_value\":"<<t.FixedDeltaValue<<",\"delta_ms\":"<<t.Delta_Time
 <<",\"high_precision_bits\":"<<bits(t.fDeltaTime)<<",\"real_delta_ms\":"<<t.rDelta_Time
 <<",\"fps\":"<<t.fps<<",\"fps_count\":"<<t.fps_count<<",\"fps_time_ms\":"<<t.fps_time<<"}";}
int main(){
 const int32_t fixed[]={-1,0,1,16,20,100,200,2147483647};
 const int64_t elapsed[]={0,1,999999,1000000,1999999,16666666,99999999,100000001,1000000000,75000000,33333333,10000000};
 std::cout<<"[";
 for(int p=0;p<16;p++){
  InjectedClock::ticks=0;TIMER timer;if(p)std::cout<<",";
  std::cout<<"{\"profile\":"<<p<<",\"frames\":[{\"initial\":true,\"elapsed_nanos\":0,\"set_delta\":null,\"returned\":"<<timer.GetDeltaTime()<<",\"state\":";state(timer);std::cout<<"}";
  for(int s=0;s<64;s++){
   bool setting=false;int32_t delta=0;
   if(s%16==1){setting=true;delta=fixed[p%8];}else if(s%16==9||s==63){setting=true;delta=s==63?-1:0;}
   if(setting)timer.SetDelta(delta);
   auto nanos=elapsed[(s*5+p*3)%12];InjectedClock::ticks+=nanos;
   auto returned=timer.Run();
   std::cout<<",{\"initial\":false,\"elapsed_nanos\":"<<nanos<<",\"set_delta\":";
   if(setting)std::cout<<delta;else std::cout<<"null";
   std::cout<<",\"returned\":"<<returned<<",\"state\":";state(timer);std::cout<<"}";
  }
  std::cout<<"]}";
 }
 std::cout<<"]\n";
}
'''.replace('HEADER_PATH',str(header))
with tempfile.TemporaryDirectory(prefix='storm-timer-') as directory:
    directory=Path(directory);cpp=directory/'timer.cpp';binary=directory/'timer'
    cpp.write_text(source)
    subprocess.run(['clang++','-std=c++17','-O0','-ffp-contract=off','-fsanitize=undefined','-fno-sanitize-recover=undefined',str(cpp),'-o',str(binary)],check=True)
    result=subprocess.check_output([str(binary)])
    values=json.loads(result)
    assert len(values)==16 and all(len(v['frames'])==65 for v in values)
    destination=Path(__file__).with_name('cases.json');destination.write_bytes(result)
    print('Complete TIMER:16profiles/1040snapshots, UBSan, SHA256',hashlib.sha256(result).hexdigest())
