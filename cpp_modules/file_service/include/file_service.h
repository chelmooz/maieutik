#pragma once
#include <string>
#include <vector>

struct FileInfo {
    std::string path;
    size_t size;
};

class FileService {
public:
    static std::vector<FileInfo> listFiles(const std::string& folder);
    static std::string readFile(const std::string& path);
};
