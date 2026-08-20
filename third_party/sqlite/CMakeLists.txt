# SQLite 官方 amalgamation（vendored，Public Domain）：ZzLogEngine 冷层持久存储。
# 必须定义 SQLITE_ENABLE_FTS5（全文索引）；线程模式保持默认 serialized。
add_library(sqlite3_static STATIC
    sqlite3.c
    sqlite3.h
    sqlite3ext.h
)
target_include_directories(sqlite3_static PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_compile_definitions(sqlite3_static PUBLIC SQLITE_ENABLE_FTS5)
if(UNIX)
    find_package(Threads REQUIRED)
    target_link_libraries(sqlite3_static PUBLIC Threads::Threads ${CMAKE_DL_LIBS})
endif()
# amalgamation 单文件体量大，关闭第三方告警干扰
if(MSVC)
    target_compile_options(sqlite3_static PRIVATE /W0)
else()
    target_compile_options(sqlite3_static PRIVATE -w)
endif()
