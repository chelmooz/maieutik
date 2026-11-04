#include <string>
#include <vector>
#include <cstring>
#include <sstream>

extern "C" {
    char* cpp_create_table(int rows, int cols, const char* headers_cstr) {
        (void)cols;
        std::string headers(headers_cstr ? headers_cstr : "");
        std::stringstream ss;
        std::vector<std::string> header_vec;
        if (!headers.empty()) {
            std::string h = headers;
            if (!h.empty() && h.back() == '|') {
                h.pop_back();
            }
            size_t pos = 0;
            while ((pos = h.find('|')) != std::string::npos) {
                header_vec.push_back(h.substr(0, pos));
                h.erase(0, pos + 1);
            }
            header_vec.push_back(h);
        }
        // If no headers provided, create default columns
        if (header_vec.empty()) {
            for (int i = 0; i < cols; ++i) {
                header_vec.push_back("Col" + std::to_string(i + 1));
            }
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
        std::strcpy(res, result.c_str());
        return res;
    }
    void cpp_free_table(char* ptr) { if(ptr) delete[] ptr; }
}
