// Diagnostic service embedding; activation/cleanup codes and lifecycle are original.
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
    ship.fire_places_before = count;
    if (count > 0)
    {
        SendMessage(ship, "llsssfl", 50304, 0, "smoke", "fire", "sound", 1.0, 7);
        SendMessage(ship, "llsssfl", 50304, 0, "ignored", "ignored", "ignored", 99.0, 19);
        if (PreviewActiveFires() != 1) { return -1; }
        PreviewAdvanceSailing(1.0);
        if (PreviewActiveFires() != 1) { return -2; }
        PreviewAdvanceSailing(0.125);
        if (PreviewActiveFires() != 0) { return -3; }
        SendMessage(ship, "llsssfl", 50304, 0, "smoke", "fire", "sound", 4.0, 19);
        SendMessage(ship, "l", 50315);
        int cleared;
        SendMessage(ship, "le", 50306, &cleared);
        ship.fire_places_cleared = cleared;
        if (cleared != 0 || PreviewActiveFires() != 0) { return -4; }
        SendMessage(ship, "l", 50315);
        CreateEntity(&ship, "preview_ship_sails");
        SendMessage(ship, "le", 50306, &cleared);
        if (cleared != count) { return -5; }
        SendMessage(ship, "llsssfl", 50304, 0, "smoke", "fire", "sound", 2.0, 23);
        PreviewAdvanceSailing(1.0);
        if (PreviewActiveFires() != 1) { return -6; }
    }
    return count;
}
