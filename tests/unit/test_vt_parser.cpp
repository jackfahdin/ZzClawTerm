/**
 * @file test_vt_parser.cpp
 * @brief ZzVTParser 与 ZzTextBuffer 协作的单元测试。
 */

#include <catch2/catch_test_macros.hpp>

#include "core/terminal/ZzCell.h"
#include "core/terminal/ZzTextBuffer.h"
#include "core/terminal/ZzVTParser.h"

using ZzCore::Terminal::ZzColor;
using ZzCore::Terminal::ZzTextBuffer;
using ZzCore::Terminal::ZzVTParser;

TEST_CASE("解析普通字符与 SGR 颜色", "[vtparser]")
{
    ZzTextBuffer buf;
    buf.resize(80, 24);
    ZzVTParser parser(buf);

    parser.parse(QByteArrayLiteral("\x1b[31mAB"));

    REQUIRE(buf.cell(0, 0).codepoint == U'A');
    REQUIRE(buf.cell(0, 1).codepoint == U'B');
    REQUIRE(buf.cell(0, 0).fg.type == ZzColor::Type::Indexed);
    REQUIRE(buf.cell(0, 0).fg.index == 1);
    REQUIRE(buf.cursorCol() == 2);
    REQUIRE(buf.cursorRow() == 0);
}

TEST_CASE("回车换行移动光标", "[vtparser]")
{
    ZzTextBuffer buf;
    buf.resize(80, 24);
    ZzVTParser parser(buf);

    parser.parse(QByteArrayLiteral("X\r\nY"));

    REQUIRE(buf.cell(0, 0).codepoint == U'X');
    REQUIRE(buf.cell(1, 0).codepoint == U'Y');
    REQUIRE(buf.cursorRow() == 1);
}

TEST_CASE("真彩色 SGR 38;2", "[vtparser]")
{
    ZzTextBuffer buf;
    buf.resize(80, 24);
    ZzVTParser parser(buf);

    parser.parse(QByteArrayLiteral("\x1b[38;2;10;20;30mZ"));

    const auto& cell = buf.cell(0, 0);
    REQUIRE(cell.codepoint == U'Z');
    REQUIRE(cell.fg.type == ZzColor::Type::Rgb);
    REQUIRE(cell.fg.r == 10);
    REQUIRE(cell.fg.g == 20);
    REQUIRE(cell.fg.b == 30);
}

TEST_CASE("光标定位 CUP", "[vtparser]")
{
    ZzTextBuffer buf;
    buf.resize(80, 24);
    ZzVTParser parser(buf);

    parser.parse(QByteArrayLiteral("\x1b[5;10H*"));

    // CUP 为 1 基; 第 5 行第 10 列 → 0 基 (4, 9)。
    REQUIRE(buf.cell(4, 9).codepoint == U'*');
}
