// New diagnostic fixture in Storm source syntax; no commercial game scripts.
native void PreviewSetAttribute(string path, string value);
native string PreviewGetAttribute(string path);

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

float LostArea(float area, int holes, int maximum)
{
    return area * holes / maximum;
}

int main()
{
    int mask = 548;
    int holes = CountHoles(mask);
    if (holes != 3) return -1;
    if (LostArea(12.0, holes, 12) != 3.0) return -2;
    PreviewSetAttribute("ship.sails.rey_a1.1.hd", "" + mask);
    PreviewSetAttribute("ship.sails.rey_a1.1.holes", "" + holes);
    PreviewSetAttribute("ship.sails.rey_b1.2.hd", "" + 274);
    if (PreviewGetAttribute("SHIP.SAILS.rey_a1.1.hd") != "548") return -3;
    return holes + CountHoles(274);
}
