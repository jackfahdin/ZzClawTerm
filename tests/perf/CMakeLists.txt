# 性能基建库：携带记录目录、编译器、构建类型等编译期定义
add_library(ZzPerfInfra STATIC
    ZzPerfRecorder.h
    ZzPerfRecorder.cpp
)
target_include_directories(ZzPerfInfra PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(ZzPerfInfra PUBLIC ZzClawTermApp Qt6::Core)
target_compile_definitions(ZzPerfInfra PUBLIC
    ZZ_PERF_RECORDS_DIR="${CMAKE_SOURCE_DIR}/tests/perf/records"
    ZZ_COMPILER_DESCRIPTION="${CMAKE_CXX_COMPILER_ID} ${CMAKE_CXX_COMPILER_VERSION}"
)

# 注册一个性能测试：链接 ZzPerfInfra，并打上 perf 标签便于筛选
function(zz_add_perf_test name)
    add_executable(${name} ${ARGN})
    target_link_libraries(${name} PRIVATE ZzClawTermApp ZzTestMocks ZzPerfInfra Qt6::Test Qt6::Widgets)
    add_test(NAME ${name} COMMAND ${name})
    set_tests_properties(${name} PROPERTIES
        LABELS "perf"
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
endfunction()

zz_add_perf_test(tst_PerfRecorder tst_PerfRecorder.cpp)
zz_add_perf_test(tst_PerfSettings tst_PerfSettings.cpp)
