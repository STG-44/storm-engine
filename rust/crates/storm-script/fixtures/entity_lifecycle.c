// New diagnostic handlers, not original commercial game scripts or entities.
native ref PreviewShip();

void Finish()
{
    ref ship;
    ship = PreviewShip();
    // EntityUpdate is compiler state, shared with this posted handler.
    ship.silent = 1;
    EntityUpdate(1);
    ship.hd = 548;
    object copy;
    copy = ship;
    if (IsEntity(copy) != 1) return;
    DeleteClass(copy);
    // The original keeps a pending entity available to attribute callbacks.
    ship.deleted = 1;
    EntityUpdate(0);
    ship.valid = IsEntity(ship);
}

int main()
{
    ref ship;
    ship = PreviewShip();
    ship.index = 19;
    if (IsEntity(ship) != 1) return -1;
    EntityUpdate(0);
    ship.hd = 274;
    SetEventHandler("Finish", "Finish", 0);
    PostEvent("Finish", 0);
    return sti(ship.index);
}
