// Same profile in both channels, at every original fireplace, across a restart.
native ref PreviewShip();
native float PreviewAdvanceSailing(float seconds);
native int PreviewActiveFires();

int main()
{
    ref ship;
    ship = PreviewShip();
    CreateEntity(&ship, "preview_ship_sails");
    int count;
    SendMessage(ship, "le", 50306, &count);
    int i;
    i = 0;
    while (i < count)
    {
        SendMessage(ship, "llsssfl", 50304, i, "Ship_Fire", "Ship_Fire", "silent", 1.0, 7);
        i = i + 1;
    }
    PreviewAdvanceSailing(1.0);
    if (PreviewActiveFires() != count) { return -1; }
    PreviewAdvanceSailing(0.125);
    if (PreviewActiveFires() != 0) { return -2; }
    i = 0;
    while (i < count)
    {
        SendMessage(ship, "llsssfl", 50304, i, "SHIP_FIRE.XPS", "Ship_Fire", "silent", 60.0, 19);
        i = i + 1;
    }
    PreviewAdvanceSailing(1.0);
    if (PreviewActiveFires() != count) { return -3; }
    return count;
}
