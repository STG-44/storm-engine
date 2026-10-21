// New diagnostic source. Commercial game programs and resources are not included.
native ref PreviewShip();
object Characters[2];
int Masks[2] = {548,274};
string Nodes[2] = {"rey_a1","rey_b1"};
string Groups[2] = {"1","2"};
int TotalHoles;
ref VisibleShip;

int Main() {
    VisibleShip = PreviewShip();
    SetEventHandler("Damage", "OnDamage", 0);
    SetEventHandler("Commit", "OnCommit", 0);
    int i = 0;
    while (i < 2) {
        Event("Damage", "ll", i, Masks[i]);
        i++;
    }
    PostEvent("Commit", 0);
    return TotalHoles;
}

void OnDamage() {
    int i = GetEventData();
    int mask = GetEventData();
    ref character;
    makeref(character, Characters[i]);
    string node = Nodes[i];
    string group = Groups[i];
    character.ship.sails.(node).(group).hd = mask;
    int bits = mask;
    int holes = 0;
    while (bits > 0) {
        holes += bits % 2;
        bits = bits / 2;
    }
    character.ship.holes = holes;
    TotalHoles += holes;
}

void OnCommit() {
    // A separate frame must read the same globals after Main and handlers return.
    int i = 0;
    while (i < 2) {
        string node = Nodes[i];
        string group = Groups[i];
        VisibleShip.sails.(node).(group).hd = Characters[i].ship.sails.(node).(group).hd;
        i++;
    }
    VisibleShip.holes = TotalHoles;
}
