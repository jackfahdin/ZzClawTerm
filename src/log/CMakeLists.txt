# ZzLogEngine：纯 Qt Core 日志引擎库（热层环形缓冲 + 温层 mmap/LZ4），不依赖 Widgets。
add_library(ZzLogEngine STATIC
    ZzLogLine.h
    ZzLineIndex.h
    ZzLineIndex.cpp
)
target_include_directories(ZzLogEngine PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(ZzLogEngine PUBLIC Qt6::Core PRIVATE lz4_static)
target_compile_features(ZzLogEngine PUBLIC cxx_std_20)
set_target_properties(ZzLogEngine PROPERTIES AUTOMOC ON)
