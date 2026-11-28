// Duplicate and cyclic includes must not load a file twice.
#include "MAIN.C"
#include "data/CHARACTERS.c"
void OnDamage() {
    int i = GetEventData();
    int mask = GetEventData();
    ref character;
    makeref(character, Characters[i]);
    string node = Nodes[i];
    string group = Groups[i];
    character.ship.sails.(node).(group).hd = mask;
    int holes = CountHoles(mask);
    character.ship.holes = holes;
    TotalHoles += holes;
}

void OnCommit() {
    int i = 0;
    while (i < SHIP_COUNT) {
        string node = Nodes[i];
        string group = Groups[i];
        VisibleShip.sails.(node).(group).hd = Characters[i].ship.sails.(node).(group).hd;
        i++;
    }
    VisibleShip.holes = TotalHoles;
}
