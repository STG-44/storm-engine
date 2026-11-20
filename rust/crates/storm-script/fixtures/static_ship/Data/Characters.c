// Includes are rooted at the program directory, not this file's directory.
#include "Helpers\Bits.c"
object Characters[SHIP_COUNT];
int Masks[SHIP_COUNT] = {548,274};
string Nodes[SHIP_COUNT] = {"rey_a1","rey_b1"};
string Groups[SHIP_COUNT] = {"1","2"};
int TotalHoles;
ref VisibleShip;
#define COMMIT_EVENT "Commit"
