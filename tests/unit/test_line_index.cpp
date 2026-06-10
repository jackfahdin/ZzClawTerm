/**
 * @file test_line_index.cpp
 * @brief ZzLineIndex 单元测试。
 */

#include <catch2/catch_test_macros.hpp>

#include "core/log/ZzLineIndex.h"

using ZzCore::Log::ZzLineIndex;

TEST_CASE("行偏移索引分块定位", "[lineindex]")
{
    ZzLineIndex index;
    constexpr std::uint64_t kLines = 2500;
    for (std::uint64_t i = 0; i < kLines; ++i) {
        index.appendLine(i * 10);  // 每行偏移 = 行号*10
    }

    REQUIRE(index.totalLines() == kLines);

    // 块大小 1024: 第 1500 行所在块起始行为 1024。
    REQUIRE(index.blockStartLine(1500) == 1024);
    REQUIRE(index.blockStartLine(0) == 0);
    REQUIRE(index.blockStartLine(2048) == 2048);

    // 块首偏移 = 块首行号 * 10。
    REQUIRE(index.blockOffsetForLine(0) == 0);
    REQUIRE(index.blockOffsetForLine(1024) == 10240);
    REQUIRE(index.blockOffsetForLine(1500) == 10240);
    REQUIRE(index.blockOffsetForLine(2048) == 20480);
}
