# 统一的编译警告配置
# 提供一个 INTERFACE 目标 zz_warnings, 各模块按需链接。

add_library(zz_warnings INTERFACE)

if(MSVC)
    target_compile_options(zz_warnings INTERFACE
        /W4
        /permissive-
        /utf-8
        /Zc:__cplusplus
    )
else()
    target_compile_options(zz_warnings INTERFACE
        -Wall
        -Wextra
        -Wpedantic
        -Wshadow
        -Wconversion
    )
endif()
