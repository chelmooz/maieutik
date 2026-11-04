#include <string>
#include <vector>
#include <cstring>
#include <sstream>

extern "C" {
    // Create table
    char* create_table(int rows, int cols, const char* headers_cstr) {
        std::string headers(headers_cstr ? headers_cstr : "");
        std::stringstream ss;
        std::vector<std::string> header_vec;
        if (!headers.empty()) {
            std::string h = headers.substr(0, headers.size()-1); // remove trailing |
            size_t pos = 0;
            while ((pos = h.find('|')) != std::string::npos) {
                header_vec.push_back(h.substr(0, pos));
                h.erase(0, pos + 1);
            }
            header_vec.push_back(h);
        }
        for (const auto &h : header_vec) ss << "| " << h << " ";
        ss << "|\n|";
        for (size_t i = 0; i < header_vec.size(); ++i) ss << "|---";
        ss << "|\n";
        for (int r = 0; r < rows; ++r) {
            for (size_t i = 0; i < header_vec.size(); ++i) ss << "|  ";
            ss << "|\n";
        }
        std::string result = ss.str();
        char* res = new char[result.size() + 1];
        strcpy(res, result.c_str());
        return res;
    }
    void free_table(char* ptr) { if(ptr) delete[] ptr; }
}
