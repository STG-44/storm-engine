// New compatibility diagnostic, not original commercial ship classes/scripts.
native ref PreviewShip();

int RunCreation(int use_address)
{
    ref ship;
    ship = PreviewShip();
    ship.index = 19;
    object previous;
    previous = ship;
    int created;
    if (use_address) created = CreateEntity(&ship, "SCRIPT_ATTRIBUTE_OBSERVER");
    else created = CreateEntity(ship, "SCRIPT_ATTRIBUTE_OBSERVER");
    if (created != 1) return -1;
    ship.ready = 1;
    int rejected;
    if (use_address) rejected = CreateEntity(&ship, "script_attribute_reject");
    else rejected = CreateEntity(ship, "script_attribute_reject");
    if (rejected != 0) return -2;
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

int main() { return RunCreation(0); }
int AddressMain() { return RunCreation(1); }
