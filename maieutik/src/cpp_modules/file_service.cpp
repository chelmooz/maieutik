#include <filesystem>
#include <vector>
#include <string>
#include <cstring>
#include <fstream>
#include <mutex>

namespace fs = std::filesystem;

constexpr int MAX_FILE_RESULTS = 1024;
static char* file_result_ptrs[MAX_FILE_RESULTS];
static std::mutex file_result_mutex;

extern "C" {
    int cpp_list_mcp_files(const char* path, char*** out_files) {
        std::lock_guard<std::mutex> lock(file_result_mutex);
        fs::path folder(path);
        if (!fs::exists(folder)) { return 0; }
        int count = 0;
        for (const auto& e : fs::directory_iterator(folder)) {
            if (count >= MAX_FILE_RESULTS) break;
            if (e.is_regular_file() && 
                (e.path().extension() == ".md" || e.path().extension() == ".txt")) {
                // Zero-copy: store pointer to path c_str, note lifetime limited to this call
                // For safety we copy into static buffer
                std::string p = e.path().string();
                file_result_ptrs[count] = new char[p.size() + 1];
                strcpy(file_result_ptrs[count], p.c_str());
                count++;
            }
        }
        *out_files = file_result_ptrs;
        return count;
    }
    char* cpp_read_mcp_file(const char* path) {
        std::ifstream f(path); if (!f) return nullptr;
        std::string content((std::istreambuf_iterator<char>(f)), std::istreambuf_iterator<char>());
        char* res = new char[content.size()+1];
        strcpy(res, content.c_str());
        return res;
    }
    void cpp_free_strings(char** p, int n) { for(int i=0;i<n;i++) delete[] p[i]; delete[] p; }
    void cpp_free_string(char* p) { if(p) delete[] p; }
}
