#include <regex>
#include <string>
#include <vector>
#include <cstring>
#include <fstream>
#include <algorithm>

extern "C" {
    int cpp_extract_links(const char* content_cstr, char*** results) {
        std::string content(content_cstr);
        std::vector<std::string> links;
        std::regex wiki_regex(R"(\[\[([^\]]+)\]\])");
        auto begin = std::sregex_iterator(content.begin(), content.end(), wiki_regex);
        auto end = std::sregex_iterator();

        for (auto it = begin; it != end; ++it) {
            links.push_back((*it)[1].str());
        }

        *results = new char*[links.size()];
        for (size_t i = 0; i < links.size(); ++i) {
            (*results)[i] = new char[links[i].size() + 1];
            strcpy((*results)[i], links[i].c_str());
        }
        return static_cast<int>(links.size());
    }
    void cpp_free_links(char** ptr, int count) { for(int i=0;i<count;i++) delete[] ptr[i]; delete[] ptr; }
}
