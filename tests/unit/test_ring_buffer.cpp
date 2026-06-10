/**
 * @file test_ring_buffer.cpp
 * @brief ZzRingBuffer 单元测试。
 */

#include <catch2/catch_test_macros.hpp>

#include "core/log/ZzRingBuffer.h"

using ZzCore::Log::ZzRingBuffer;

TEST_CASE("环形缓冲区追加与容量", "[ringbuffer]")
{
    ZzRingBuffer rb(3);
    REQUIRE(rb.capacity() == 3);
    REQUIRE(rb.size() == 0);

    REQUIRE_FALSE(rb.append(QStringLiteral("a")).has_value());
    REQUIRE_FALSE(rb.append(QStringLiteral("b")).has_value());
    REQUIRE_FALSE(rb.append(QStringLiteral("c")).has_value());
    REQUIRE(rb.size() == 3);

    SECTION("满后追加挤出最老元素")
    {
        const auto evicted = rb.append(QStringLiteral("d"));
        REQUIRE(evicted.has_value());
        REQUIRE(evicted.value() == QStringLiteral("a"));
        REQUIRE(rb.size() == 3);
        REQUIRE(rb.at(0) == QStringLiteral("b"));
        REQUIRE(rb.at(2) == QStringLiteral("d"));
    }

    SECTION("越界访问返回空")
    {
        REQUIRE(rb.at(99).isEmpty());
    }
}

TEST_CASE("环形缓冲区重设容量清空内容", "[ringbuffer]")
{
    ZzRingBuffer rb(2);
    rb.append(QStringLiteral("x"));
    rb.setCapacity(5);
    REQUIRE(rb.capacity() == 5);
    REQUIRE(rb.size() == 0);
}
