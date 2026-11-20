// Compatibility diagnostic. Original commercial message codes are not used.
native ref PreviewShip();
int kept;

int main()
{
    ref ship;
    ship = PreviewShip();
    ship.index = 19;
    CreateEntity(&ship, "script_message_probe");
    int value;
    value = 7;
    int wind;
    string text;
    ptr token;
    SendMessage(ship, "leeee", 1, &value, &wind, &text, &token);
    int positions[3];
    int count;
    SendMessage(ship, "lee", 2, &positions, &count);
    int temporary;
    temporary = 99;
    SendMessage(ship, "le", 3, temporary);
    SendMessage(ship, "le", 4, &kept);
    SendMessage(ship, "l", 5);
    ship.native_value = value;
    ship.native_wind = wind;
    ship.native_text = text;
    ship.native_ptr_low = sti(token);
    ship.array_total = positions[0] + positions[1] + positions[2];
    ship.array_count = count;
    ship.temporary = temporary;
    ship.kept = kept;
    DeleteClass(ship);
    SendMessage(ship, "le", 0, &value);
    ship.deleted_value = value;
    return kept;
}
