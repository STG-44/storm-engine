#event_handler(DAMAGE_EVENT, "OnDamage")
// New static-handler diagnostic, not commercial game source.
#define SHIP_COUNT 2
#define MASK_A 548
#define MASK_B 274
#define DAMAGE_SCALE 1.0
#define DAMAGE_EVENT "Damage"
#include "Data\Characters.c"
#include "Handlers/Sails.c"
#include "handlers\SAILS.C";
native ref PreviewShip();

int Main() {
    if (DAMAGE_SCALE != 1.0) return -1;
    VisibleShip = PreviewShip();
    ref ships;
    makeref(ships, Characters);
    if (GetArraySize(ships) != SHIP_COUNT) return -2;
    Masks[0] = MASK_A;
    Masks[1] = MASK_B;
    int i = 0;
    while (i < SHIP_COUNT) {
        Event(DAMAGE_EVENT, "ll", i, Masks[i]);
        i++;
    }
    PostEvent(COMMIT_EVENT, 0);
    return TotalHoles;
}

// Registered at compile time even though this branch and function never run.
void NeverCalled() {
    if (0) { #event_handler(COMMIT_EVENT, "OnCommit"); }
}
