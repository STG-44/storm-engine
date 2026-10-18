// New compatibility diagnostic, not original commercial ship classes/scripts.
native ref PreviewShip();

int main()
{
    ref ship;
    ship = PreviewShip();
    ship.index = 19;
    object previous;
    previous = ship;
    if (CreateEntity(ship, "SCRIPT_ATTRIBUTE_OBSERVER") != 1) return -1;
    ship.ready = 1;
    if (CreateEntity(ship, "script_attribute_reject") != 0) return -2;
    if (IsEntity(ship) != 1) return -3;
    ship.failed = 1;
    object standalone;
    standalone = CreateClass("script_attribute_observer");
    if (IsEntity(standalone) != 1) return -4;
    if (CheckAttribute(standalone, "initialized") != 0) return -5;
    standalone.test = 7;
    DeleteClass(standalone);
    DeleteClass(previous);
    ship.new_valid = IsEntity(ship);
    ship.old_valid = IsEntity(previous);
    return sti(ship.index);
}
