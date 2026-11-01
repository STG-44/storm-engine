// New bounded compatibility fixture; not commercial weather/game source.
native ref PreviewSea();
int Main()
{
    ref sea = PreviewSea();
    sea.Sea2.Amp1 = 8.0;
    sea.Sea2.Amp2 = 1.0;
    sea.Sea2.AnimSpeed1 = 6.0;
    sea.Sea2.AnimSpeed2 = 3.0;
    sea.Sea2.Scale1 = 0.5;
    sea.Sea2.Scale2 = 2.0;
    sea.Sea2.MoveSpeed1 = "5, 0, 0";
    sea.Sea2.MoveSpeed2 = "0, 0, 2";
    sea.MaxSeaDistance = 1600.0;
    sea.MaxSeaHeight = 1.5;
    sea.isDone.refresh = 1;
    SetEventHandler("SeaCalm", "OnCalm", 0);
    SetEventHandler("SeaRough", "OnRough", 0);
    SetEventHandler("SeaStop", "OnStop", 0);
    SetEventHandler("SeaResume", "OnResume", 0);
    SetEventHandler("SeaPending", "OnPending", 0);
    SetEventHandler("SeaApply", "OnApply", 0);
    PostEvent("SeaCalm", 500);
    PostEvent("SeaRough", 1000);
    PostEvent("SeaStop", 1500);
    PostEvent("SeaResume", 2000);
    PostEvent("SeaPending", 2500);
    PostEvent("SeaApply", 2750);
    return 6;
}
void OnCalm() { ref sea = PreviewSea(); sea.MaxSeaHeight = 0.0; }
void OnRough()
{
    ref sea = PreviewSea();
    sea.Sea2.Amp1 = 8.0;
    sea.Sea2.Amp2 = 1.0;
    sea.MaxSeaHeight = 3.0;
    sea.isDone.refresh = 2;
}
void OnStop() { ref sea = PreviewSea(); sea.Stop = 1; sea.isDone.refresh = 3; }
void OnResume() { ref sea = PreviewSea(); sea.Stop = 0; sea.isDone.refresh = 4; }
void OnPending() { ref sea = PreviewSea(); sea.Sea2.Amp1 = 2.0; sea.Sea2.Amp2 = 0.5; }
void OnApply() { ref sea = PreviewSea(); sea.MaxSeaHeight = 1.0; sea.isDone.refresh = 5; }
