# ─────────────────────────────────────────────────────────────
# 单元测试 (Catch2)。需 -DZZ_ENABLE_TESTS=ON 与 catch2 依赖。
# ─────────────────────────────────────────────────────────────

find_package(Catch2 3 REQUIRED)

set(ZZ_TEST_SOURCES
    unit/test_ring_buffer.cpp
    unit/test_line_index.cpp
    unit/test_vt_parser.cpp

    # 被测源文件 (避免引入完整可执行目标)
    ${CMAKE_SOURCE_DIR}/src/core/log/ZzRingBuffer.cpp
    ${CMAKE_SOURCE_DIR}/src/core/log/ZzRingBufferPrivate.cpp
    ${CMAKE_SOURCE_DIR}/src/core/log/ZzLineIndex.cpp
    ${CMAKE_SOURCE_DIR}/src/core/log/ZzLineIndexPrivate.cpp
    ${CMAKE_SOURCE_DIR}/src/core/terminal/ZzVTParser.cpp
    ${CMAKE_SOURCE_DIR}/src/core/terminal/ZzVTParserPrivate.cpp
    ${CMAKE_SOURCE_DIR}/src/core/terminal/ZzTextBuffer.cpp
    ${CMAKE_SOURCE_DIR}/src/core/terminal/ZzTextBufferPrivate.cpp
)

add_executable(zz_tests ${ZZ_TEST_SOURCES})

target_include_directories(zz_tests PRIVATE ${CMAKE_SOURCE_DIR}/src)
target_link_libraries(zz_tests PRIVATE Qt6::Core Catch2::Catch2WithMain)

include(CTest)
include(Catch)
catch_discover_tests(zz_tests)
