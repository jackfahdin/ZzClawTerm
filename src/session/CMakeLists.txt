# Qt6::Core 与 OpenSSL 的 find_package 均为幂等，允许与根或其他模块重复调用
find_package(Qt6 6.8 REQUIRED COMPONENTS Core)
find_package(OpenSSL 3.0 REQUIRED COMPONENTS Crypto)

# 系统密钥环后端（ZzKeyringCredentialBackend）：
# Linux 优先 pkg-config 探测 libsecret 开发包；找不到则编译优雅降级桩
# （isAvailable()=false，门面回退 AES 文件后端），不引入新第三方依赖。
# Windows 链接 advapi32（wincred），macOS 链接 Security/CoreFoundation。
if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
    find_package(PkgConfig QUIET)
    if(PkgConfig_FOUND)
        pkg_check_modules(LIBSECRET QUIET IMPORTED_TARGET libsecret-1)
    endif()
endif()

set(CMAKE_AUTOMOC ON)

add_library(ZzSessionCore STATIC
    ZzSessionProfile.cpp
    ZzSessionProfile.h
    ZzSessionModel.cpp
    ZzSessionModel.h
    ZzCredentialBackend.h
    ZzCredentialStore.cpp
    ZzCredentialStore.h
    ZzAesFileCredentialBackend.cpp
    ZzAesFileCredentialBackend.h
    ZzKeyringCredentialBackend.cpp
    ZzKeyringCredentialBackend.h
    ZzForwardRule.cpp
    ZzForwardRule.h
)
target_compile_features(ZzSessionCore PUBLIC cxx_std_20)
target_link_libraries(ZzSessionCore PUBLIC Qt6::Core PRIVATE OpenSSL::Crypto)
target_include_directories(ZzSessionCore PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})

if(LIBSECRET_FOUND)
    target_compile_definitions(ZzSessionCore PRIVATE ZZ_HAVE_LIBSECRET=1)
    target_link_libraries(ZzSessionCore PRIVATE PkgConfig::LIBSECRET)
    # glib/gio 头文件（GDBusInterfaceInfo::signals 成员）与 Qt signals/slots
    # 关键字宏冲突；该翻译单元不使用 Qt 关键字，单独禁用
    set_source_files_properties(ZzKeyringCredentialBackend.cpp
        PROPERTIES COMPILE_DEFINITIONS QT_NO_KEYWORDS)
elseif(WIN32)
    target_link_libraries(ZzSessionCore PRIVATE advapi32)
elseif(APPLE)
    target_link_libraries(ZzSessionCore PRIVATE
        "-framework Security" "-framework CoreFoundation")
endif()
