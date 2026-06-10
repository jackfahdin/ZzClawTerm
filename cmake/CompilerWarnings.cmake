# 统一的编译警告配置
# 提供一个 INTERFACE 目标 zz_warnings, 各模块按需链接。
# 自有代码要求零警告: 默认将警告视为错误 (可用 -DZZ_WERROR=OFF 临时关闭)。

option(ZZ_WERROR "将编译警告视为错误" ON)

add_library(zz_warnings INTERFACE)

if(MSVC)
    target_compile_options(zz_warnings INTERFACE
        /W4
        /permissive-
        /utf-8
        /Zc:__cplusplus
        $<$<BOOL:${ZZ_WERROR}>:/WX>
    )
else()
    target_compile_options(zz_warnings INTERFACE
        -Wall
        -Wextra
        -Wpedantic
        -Wshadow
        -Wconversion
        $<$<BOOL:${ZZ_WERROR}>:-Werror>
    )
endif()
