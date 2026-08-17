# ZzClawTermApp：除 main.cpp 外的全部应用源码，供可执行与测试共同链接。
set(ZZCLAWTERM_APP_SOURCES
)

# 计划 03（会话模型/凭据）与计划 02（日志引擎）交付的子目录
foreach(sub session log)
    if(EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/${sub}/CMakeLists.txt")
        add_subdirectory(${sub})
    else()
        message(STATUS "缺少 src/${sub}（由计划 02/03 交付），相关装配目标暂不可用")
    endif()
endforeach()

add_library(ZzClawTermApp STATIC ${ZZCLAWTERM_APP_SOURCES})
target_include_directories(ZzClawTermApp PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(ZzClawTermApp
    PUBLIC
        Qt6::Core
        Qt6::Widgets
        Qt6::Network
        ZzQTermWidget::qtermwidget
        ZzPureTools
        ZzSshCore
)
# 计划 02/03 目标存在时才链接
foreach(optional_target ZzSessionCore ZzLogEngine)
    if(TARGET ${optional_target})
        target_link_libraries(ZzClawTermApp PUBLIC ${optional_target})
    endif()
endforeach()
target_compile_definitions(ZzClawTermApp PUBLIC
    ZZ_GIT_REVISION="${ZZ_GIT_REVISION}"
    ZZ_BUILD_TYPE="$<CONFIG>"
)

# Windows 下可执行不带控制台窗口；macOS 生成 .app
qt_add_executable(ZzClawTerm main.cpp)
set_target_properties(ZzClawTerm PROPERTIES
    WIN32_EXECUTABLE TRUE
    MACOSX_BUNDLE TRUE
)
target_link_libraries(ZzClawTerm PRIVATE ZzClawTermApp)

install(TARGETS ZzClawTerm
    RUNTIME DESTINATION bin
    BUNDLE DESTINATION .
)
