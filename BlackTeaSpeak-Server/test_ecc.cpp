#include <iostream>
#include <vector>
#include <iomanip>
extern "C" {
#include <tomcrypt.h>
}

int main() {
    ltc_mp = ltm_desc;
    
    ecc_key key;
    prng_state prng;
    int err;
    
    if (sprng_start(&prng) != CRYPT_OK) {
        std::cerr << "sprng_start failed" << std::endl;
        return 1;
    }
    
    if ((err = ecc_make_key(&prng, find_prng("sprng"), 32, &key)) != CRYPT_OK) {
        std::cerr << "ecc_make_key failed: " << error_to_string(err) << std::endl;
        return 1;
    }
    
    unsigned char out[256];
    unsigned long outlen = sizeof(out);
    if ((err = ecc_export(out, &outlen, PK_PUBLIC, &key)) != CRYPT_OK) {
        std::cerr << "ecc_export failed: " << error_to_string(err) << std::endl;
        return 1;
    }
    
    for (unsigned long i = 0; i < outlen; i++) {
        std::cout << std::hex << std::setw(2) << std::setfill('0') << (int)out[i];
    }
    std::cout << std::endl;
    return 0;
}
