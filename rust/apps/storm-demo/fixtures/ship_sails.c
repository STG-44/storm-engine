// Diagnostic source; the two numeric SHIP message codes are original.
native ref PreviewShip();
native float PreviewAdvanceSailing(float seconds);

int main()
{
    ref ship;
    ship = PreviewShip();
    int requested;
    if (SendMessage(ship, "lf", 50305, 0.5) != 0) { return -1; }
    SendMessage(ship, "le", 50325, &requested);
    if (requested != 0.5) { return -2; }
    PreviewAdvanceSailing(4.0);
    SendMessage(ship, "lf", 50305, 0.0);
    SendMessage(ship, "le", 50325, &requested);
    if (requested != 0.0) { return -3; }
    PreviewAdvanceSailing(4.0);
    SendMessage(ship, "lf", 50305, 1.0);
    SendMessage(ship, "le", 50325, &requested);
    if (requested != 1.0) { return -4; }
    PreviewAdvanceSailing(4.0);
    DeleteClass(ship);
    SendMessage(ship, "lf", 50305, 0.0);
    SendMessage(ship, "le", 50325, &requested);
    if (requested != 1.0) { return -5; }
    return 3;
}
