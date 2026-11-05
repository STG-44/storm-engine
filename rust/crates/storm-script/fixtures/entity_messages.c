// Compatibility diagnostic. These are not original game class/message codes.
native ref PreviewShip();

ptr main()
{
    ref ship;
    ship = PreviewShip();
    ship.index = 19;
    CreateEntity(&ship, "script_message_probe");
    ptr token;
    token = -1;
    int number;
    number = 7;
    ptr result;
    result = SendMessage(ship, "lfsiap", 548, &number, "парус", ship, ship, token);
    // The native f field converted the referenced caller's int to float.
    ship.number = number;
    ptr echo;
    echo = SendMessage(ship, "p", result, "surplus argument is ignored");
    ship.low_bits = sti(echo);
    if (SendMessage(ship, "") != 0) return token;
    DeleteClass(ship);
    // Pending deletion must skip ProcessMessage and return a zero ptr.
    ship.deleted_result = sti(SendMessage(ship, "p", echo));
    return echo;
}
