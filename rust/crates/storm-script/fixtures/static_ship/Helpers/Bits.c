#define ZERO_BITS 0;
int CountHoles(int mask) {
    int result = ZERO_BITS;
    while (mask > 0) {
        result += mask % 2;
        mask = mask / 2;
    }
    return result;
}
