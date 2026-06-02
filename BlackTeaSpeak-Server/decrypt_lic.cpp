#include <iostream>
#include <fstream>
#include <vector>
#include <string>
#include <random>
#include <cstring>

std::vector<unsigned char> base64_decode(const std::string& in) {
    std::vector<unsigned char> out;
    std::vector<int> T(256, -1);
    for (int i = 0; i < 64; i++) T["ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"[i]] = i;
    int val = 0, valb = -8;
    for (unsigned char c : in) {
        if (T[c] == -1) continue;
        val = (val << 6) + T[c];
        valb += 6;
        if (valb >= 0) {
            out.push_back((val >> valb) & 0xFF);
            valb -= 8;
        }
    }
    return out;
}

int main() {
    // Read licensekey.dat
    std::ifstream file("../licensekey.dat");
    if (!file.is_open()) {
        file.open("licensekey.dat");
    }
    if (!file.is_open()) {
        std::cerr << "Failed to open licensekey.dat\n";
        return 1;
    }
    std::string line;
    std::string key2_b64;
    bool is_key2 = false;
    while (std::getline(file, line)) {
        // Strip carriage returns or spaces
        while (!line.empty() && (line.back() == '\r' || line.back() == ' ')) {
            line.pop_back();
        }
        if (line == "==key==") {
            is_key2 = !is_key2;
            continue;
        }
        if (is_key2) {
            key2_b64 += line;
        }
    }
    file.close();

    std::vector<unsigned char> key2_bytes = base64_decode(key2_b64);
    std::cout << "Decoded key2 bytes: " << key2_bytes.size() << "\n";

    // Skip protobuf headers (usually starts with 0x0a)
    size_t pos = 0;
    while (pos < key2_bytes.size() && key2_bytes[pos] == 0x0a) {
        pos += 1;
        size_t len = 0;
        size_t shift = 0;
        while (pos < key2_bytes.size()) {
            unsigned char b = key2_bytes[pos++];
            len |= (static_cast<size_t>(b & 0x7f) << shift);
            shift += 7;
            if ((b & 0x80) == 0) break;
        }
        std::cout << "Skipped protobuf field length: " << len << "\n";
    }

    if (pos >= key2_bytes.size()) {
        std::cerr << "Invalid license block position\n";
        return 1;
    }

    size_t license_len = key2_bytes.size() - pos;
    unsigned char* license_block = &key2_bytes[pos];
    std::cout << "License block offset: " << pos << ", length: " << license_len << "\n";

    // LicenseHeader structure:
    // uint16_t version;
    // uint64_t crypt_key_seed;
    // uint8_t crypt_key_verify_offset;
    // uint8_t crypt_key_verify[5];
    if (license_len < 16) {
        std::cerr << "License block too small\n";
        return 1;
    }

    uint16_t version;
    std::memcpy(&version, &license_block[0], 2);
    uint64_t seed;
    std::memcpy(&seed, &license_block[2], 8);
    uint8_t verify_offset = license_block[10];
    uint8_t verify[5];
    std::memcpy(verify, &license_block[11], 5);

    std::cout << "LicenseHeader:\n";
    std::cout << "  version: " << version << "\n";
    std::cout << "  seed: " << seed << "\n";
    std::cout << "  verify_offset: " << (int)verify_offset << "\n";
    std::cout << "  verify bytes: ";
    for(int i=0; i<5; ++i) std::cout << (int)verify[i] << " ";
    std::cout << "\n";

    // MT19937-64 verification
    std::mt19937_64 crypt_key_gen(seed);
    crypt_key_gen.discard(verify_offset);
    uint64_t expected = 0;
    std::memcpy(&expected, verify, 5);

    uint64_t received = crypt_key_gen();
    received = received ^ (received >> 40);
    received &= 0xFFFFFFFFFFULL;

    std::cout << "Verification Check:\n";
    std::cout << "  Expected: " << expected << "\n";
    std::cout << "  Received: " << received << "\n";

    if (expected != received) {
        std::cerr << "Verification failed!\n";
        return 1;
    }
    std::cout << "Verification success!\n";

    // Reseed to start decrypting from the beginning of the body
    crypt_key_gen.seed(seed);

    // Decrypt the body (starting after the 16-byte LicenseHeader)
    std::vector<unsigned char> decoded_body(license_len - 16);
    size_t index = 16;
    size_t index_decoded = 0;

    // Standard decrypt logic:
    while (index + 4 <= license_len) {
        uint32_t val;
        std::memcpy(&val, &license_block[index], 4);
        val ^= static_cast<uint32_t>(crypt_key_gen());
        std::memcpy(&decoded_body[index_decoded], &val, 4);
        index += 4;
        index_decoded += 4;
    }
    while (index < license_len) {
        unsigned char val = license_block[index];
        val ^= static_cast<unsigned char>(crypt_key_gen());
        decoded_body[index_decoded] = val;
        index++;
        index_decoded++;
    }

    std::cout << "Decrypted body size: " << decoded_body.size() << "\n";

    // Save decoded body to file
    std::ofstream out("decrypted_license.bin", std::ios::binary);
    if (!out.is_open()) {
        std::cerr << "Failed to open output file decrypted_license.bin\n";
        return 1;
    }
    out.write(reinterpret_cast<char*>(decoded_body.data()), decoded_body.size());
    out.close();
    std::cout << "Saved decrypted body to decrypted_license.bin\n";

    return 0;
}
