set(CMAKE_AUTOMOC ON)

find_package(Qt6 6.8 REQUIRED COMPONENTS Test)

# 性能记录（规格 9.1）需要代码版本；探测失败时记为 unknown
find_package(Git QUIET)
set(ZZ_GIT_COMMIT_HASH "unknown")
if(Git_FOUND)
    execute_process(
        COMMAND ${GIT_EXECUTABLE} rev-parse --short HEAD
        WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}
        OUTPUT_VARIABLE ZZ_GIT_COMMIT_HASH_DETECTED
        OUTPUT_STRIP_TRAILING_WHITESPACE
        ERROR_QUIET)
    if(ZZ_GIT_COMMIT_HASH_DETECTED)
        set(ZZ_GIT_COMMIT_HASH "${ZZ_GIT_COMMIT_HASH_DETECTED}")
    endif()
endif()

# 注册一个 QTest 可执行目标：源码为同名 .cpp，链接 ZzSessionCore，并登记进 ctest。
function(zz_session_add_test target)
    add_executable(${target} ${target}.cpp)
    target_link_libraries(${target} PRIVATE ZzSessionCore Qt6::Core Qt6::Test)
    add_test(NAME ${target} COMMAND ${target})
endfunction()

zz_session_add_test(ZzSessionProfileTest)
zz_session_add_test(ZzSessionModelTest)
zz_session_add_test(ZzCredentialStoreTest)

# 规格 9.1：记录需包含构建类型 / 编译器 / git commit hash；Release 数字才有效
add_executable(ZzCredentialPerfTest ZzCredentialPerfTest.cpp)
target_link_libraries(ZzCredentialPerfTest PRIVATE ZzSessionCore Qt6::Core Qt6::Test)
target_compile_definitions(ZzCredentialPerfTest PRIVATE
    ZZ_BUILD_TYPE="$<CONFIG>"
    ZZ_COMPILER_ID="${CMAKE_CXX_COMPILER_ID}"
    ZZ_COMPILER_VERSION="${CMAKE_CXX_COMPILER_VERSION}"
    ZZ_GIT_COMMIT_HASH="${ZZ_GIT_COMMIT_HASH}"
    ZZ_RECORDS_DIR="${CMAKE_SOURCE_DIR}/tests/perf/records")
add_test(NAME ZzCredentialPerfTest COMMAND ZzCredentialPerfTest)
# 与 ZzLogEnginePerfTest 一致：性能门控归入 perf 标签，便于 ctest -L perf 统一筛选
set_tests_properties(ZzCredentialPerfTest PROPERTIES LABELS "perf")
