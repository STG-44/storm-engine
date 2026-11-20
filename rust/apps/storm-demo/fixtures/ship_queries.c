// Diagnostic embedding initialization; both SHIP query codes are original.
native ref PreviewShip();

int main()
{
    ref ship;
    ship = PreviewShip();
    CreateEntity(&ship, "preview_ship_sails");
    string index;
    SendMessage(ship, "le", 50307, &index);
    ship.initial_index = index;
    int count;
    SendMessage(ship, "le", 50306, &count);
    ship.fire_place_count = count;
    ship.index = 27;
    SendMessage(ship, "le", 50307, &index);
    if (index != 27) { return -1; }
    ship.updated_index = index;
    ship.index = -7;
    SendMessage(ship, "le", 50307, &index);
    if (index != -7) { return -2; }
    ship.negative_index = index;
    int temporary;
    temporary = 99;
    SendMessage(ship, "le", 50307, temporary);
    if (temporary != 99) { return -3; }
    DeleteClass(ship);
    SendMessage(ship, "le", 50307, &index);
    SendMessage(ship, "le", 50306, &count);
    ship.deleted_index = index;
    ship.deleted_count = count;
    return count;
}
