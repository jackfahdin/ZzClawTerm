/**
 * @file ZzVTParser.cpp
 * @brief ZzVTParser 公开接口实现。
 */

#include "core/terminal/ZzVTParser.h"

#include "core/terminal/ZzVTParserPrivate.h"

namespace ZzCore {
namespace Terminal {

ZzVTParser::ZzVTParser(ZzTextBuffer& buffer)
    : d_ptr(std::make_unique<ZzVTParserPrivate>(buffer))
{
}

ZzVTParser::~ZzVTParser() = default;

void ZzVTParser::parse(const QByteArray& data)
{
    for (char ch : data) {
        d_ptr->feed(static_cast<std::uint8_t>(ch));
    }
}

void ZzVTParser::reset()
{
    d_ptr->reset();
}

}  // namespace Terminal
}  // namespace ZzCore
