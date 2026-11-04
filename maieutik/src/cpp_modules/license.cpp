#include <string>
#include <cstring>

extern "C" {
    const char* cpp_get_license_status() {
        static const char* license = "MIT";
        return license;
    }
}
