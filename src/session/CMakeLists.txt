# Qt6::Core 与 OpenSSL 的 find_package 均为幂等，允许与根或其他模块重复调用
find_package(Qt6 6.8 REQUIRED COMPONENTS Core)
find_package(OpenSSL 3.0 REQUIRED COMPONENTS Crypto)

set(CMAKE_AUTOMOC ON)

add_library(ZzSessionCore STATIC
    ZzSessionProfile.cpp
    ZzSessionProfile.h
    ZzSessionModel.cpp
    ZzSessionModel.h
    ZzCredentialStore.cpp
    ZzCredentialStore.h
    ZzForwardRule.cpp
    ZzForwardRule.h
)
target_compile_features(ZzSessionCore PUBLIC cxx_std_20)
target_link_libraries(ZzSessionCore PUBLIC Qt6::Core PRIVATE OpenSSL::Crypto)
target_include_directories(ZzSessionCore PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
