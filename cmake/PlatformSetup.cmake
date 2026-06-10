# 平台特定配置
# 根据目标操作系统设置宏定义、平台库等。

if(WIN32)
    set(ZZ_PLATFORM_WINDOWS ON)
    add_compile_definitions(
        ZZ_PLATFORM_WINDOWS=1
        UNICODE
        _UNICODE
        NOMINMAX
        WIN32_LEAN_AND_MEAN
        # ConPTY (CreatePseudoConsole 等) 需要 Windows 10 RS5+ 的 API 级别。
        WINVER=0x0A00
        _WIN32_WINNT=0x0A00
        NTDDI_VERSION=0x0A000006
    )
elseif(APPLE)
    set(ZZ_PLATFORM_MACOS ON)
    add_compile_definitions(ZZ_PLATFORM_MACOS=1)
elseif(UNIX)
    set(ZZ_PLATFORM_LINUX ON)
    add_compile_definitions(ZZ_PLATFORM_LINUX=1)
endif()

# 统一区分 Unix 类平台 (Linux + macOS 共用 PTY 实现)
if(UNIX)
    add_compile_definitions(ZZ_PLATFORM_UNIX=1)
endif()
