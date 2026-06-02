#include <iostream>
#include <random>
#include <cstring>

int main() {
    uint64_t seed = 13332836344511906223ULL; // LE seed at 114
    std::mt19937_64 mt(seed);
    
    std::cout << "Discarding 13 values...\n";
    mt.discard(13);
    
    uint64_t val = mt();
    uint64_t received = val ^ (val >> 40);
    received &= 0xFFFFFFFFFFULL;
    
    std::cout << "Received verify value: " << received << "\n";
    std::cout << "Bytes of received: ";
    unsigned char bytes[8];
    std::memcpy(bytes, &received, 8);
    for(int i=0; i<5; ++i) std::cout << (int)bytes[i] << " ";
    std::cout << "\n";
    
    return 0;
}
