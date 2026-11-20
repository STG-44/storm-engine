// Newly written compatibility fixture, not original game scripts.
#include "helpers\identity.c"
int frames;
void Main()
{
    Trace(Identity());
    Trace(GetTargetPlatform());
    LayerSetExecute(0);
    LayerSetRealize(1);
    SetEventHandler("frame", "OnFrame", 0);
    SetEventHandler("posted", "OnPosted", 0);
    SetEventHandler("ExitApplication", "OnExit", 0);
    PostEvent("posted", 10);
}
void OnFrame()
{
    frames++;
    Trace("frame=" + frames);
    Trace("delta=" + GetDeltaTime());
}
void OnPosted() { Trace("posted"); }
void OnExit() { Trace("exit"); }
