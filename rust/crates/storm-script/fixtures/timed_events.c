// Newly written compatibility fixture, not original commercial game code.
native ref PreviewShip();

int Main()
{
    SetEventHandler("Shot", "OnShot", 0);
    SetEventHandler("Forward", "OnForward", 0);
    SetEventHandler("Commit", "OnCommit", 0);
    SetEventHandler("Cleanup", "OnCleanup", 0);
    Event("Shot", "ll", 548, 274);
    PostEvent("Commit", 10, "l", 6);
    return 6;
}

void OnShot()
{
    int first = GetEventData();
    int second = GetEventData();
    ref ship = PreviewShip();
    string group = "2";
    ship.ship.sails.rey_b1.(group).hd = second;
    Event("Forward", "l", first);
}

void OnForward()
{
    int mask = GetEventData();
    ref ship = PreviewShip();
    string group = "1";
    ship.ship.sails.rey_a1.(group).hd = mask;
}

void OnCommit()
{
    int count = GetEventData();
    ref ship = PreviewShip();
    ship.ship.sails.total = count;
    PostEvent("Cleanup", 0);
}

void OnCleanup()
{
    ref ship = PreviewShip();
    ship.ship.sails.committed = 1;
}
