// Resource-backed dispatch check; cache "smoke" with --particle-profile smoke.
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
    if (count > 0)
    {
        // Swap channels: visuals must follow the requested XPS, not the channel.
        SendMessage(ship, "llsssfl", 50304, 0, "SHIP_FIRE.XPS", "ship_smoke", "silent", 0.125, 7);
        PreviewAdvanceSailing(0.25);
        if (PreviewActiveFires() != 0) { return -1; }
        SendMessage(ship, "llsssfl", 50304, 0, "smoke", "not_cached", "silent", 60.0, 19);
        PreviewAdvanceSailing(1.0);
        if (PreviewActiveFires() != 1) { return -2; }
    }
    return count;
}
