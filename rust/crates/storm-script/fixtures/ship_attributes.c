// New compatibility fixture, not an original commercial game handler.
native ref PreviewShip();

int CountHoles(int mask)
{
    int count = 0;
    while (mask > 0)
    {
        if (mask % 2 != 0) count++;
        mask = mask / 2;
    }
    return count;
}

void RecordHit(ref character, string node, int group, int mask)
{
    string path = "ship.sails." + node + "." + group;
    aref sail;
    makearef(sail, character.(path));
    sail.hd = mask;
    sail.holes = CountHoles(mask);
}

int main()
{
    ref character;
    character = PreviewShip();
    character.index = 19;
    RecordHit(character, "rey_a1", 1, 548);
    RecordHit(character, "rey_b1", 2, 274);
    character.ship.damage = 3.25;
    aref sails;
    makearef(sails, character.ship.sails);
    if (GetAttributesNum(sails) != 2) return -1;
    aref first;
    first = GetAttributeN(sails, 0);
    if (GetAttributeName(first) != "rey_a1") return -2;

    object copy;
    copy = character;
    // Object assignment deep-copies attributes and copies the entity ID.
    // The following callback still uses that ID, although the tree is a copy.
    string group = "1";
    copy.ship.sails.rey_a1.(group).hd = 1;
    if (sti(character.ship.sails.rey_a1.(group).hd) != 548) return -3;
    if (stf(character.ship.damage) != 3.25) return -4;

    // CopyAttributes works through ref and preserves the destination root.
    object snapshot;
    ref destination;
    makeref(destination, snapshot);
    CopyAttributes(destination, character);
    DeleteAttribute(destination, "ship.damage");
    if (CheckAttribute(character, "ship.damage") != 1) return -5;
    if (CheckAttribute(destination, "ship.damage") != 0) return -6;
    return sti(character.ship.sails.rey_a1.(group).holes) + CountHoles(274);
}
