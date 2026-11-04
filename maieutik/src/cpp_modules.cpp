// C++ implementation for file_service module
// Compiled directly by cc crate

#include <filesystem>
#include <vector>
#include <string>
#include <cstring>
#include <fstream>

namespace fs = std::filesystem;

extern "C" {
    int list_files(const char* path, char*** out_files) {
        fs::path folder(path);
        if (!fs::exists(folder)) { return 0; }
        std::vector<std::string> files;
        for (const auto& e : fs::directory_iterator(folder)) {
            if (e.is_regular_file()) { files.push_back(e.path().string()); }
        }
        *out_files = new char*[files.size()];
        for (size_t i=0; i<files.size(); ++i) {
            (*out_files)[i] = new char[files[i].size()+1];
            strcpy((*out_files)[i], files[i].c_str());
        }
        return static_cast<int>(files.size());
    }
    char* read_file(const char* path) {
        std::ifstream f(path); if (!f) return nullptr;
        std::string content((std::istreambuf_iterator<char>(f)), std::istreambuf_iterator<char>());
        char* res = new char[content.size()+1];
        strcpy(res, content.c_str());
        return res;
    }
    void free_strings(char** p, int n) { for(int i=0;i<n;i++) delete[] p[i]; delete[] p; }
}
