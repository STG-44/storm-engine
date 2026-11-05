// Diagnostic handlers for original payloads. These are not commercial game scripts.
native ref PreviewCharacters();
native void PreviewSailDamage(int character, float x, float y, float z, int owner);

void InitSailEvents()
{
    SetEventHandler("DoSailHole", "OnSailHole", 0);
    SetEventHandler("ShpSailDamage", "OnSailDamage", 0);
}

void OnSailHole()
{
    int owner = GetEventData();
    int character = GetEventData();
    string model = GetEventData();
    string node = GetEventData();
    int group = GetEventData();
    int count = GetEventData();
    int mask = GetEventData();
    int maximum = GetEventData();
    float fraction = GetEventData();

    ref characters = PreviewCharacters();
    aref ship;
    makearef(ship, characters.characters.(character));
    ship.ship.sails.(node).(group).hd = mask;
}

void OnSailDamage()
{
    int character = GetEventData();
    float x = GetEventData();
    float y = GetEventData();
    float z = GetEventData();
    int owner = GetEventData();
    PreviewSailDamage(character, x, y, z, owner);
}
