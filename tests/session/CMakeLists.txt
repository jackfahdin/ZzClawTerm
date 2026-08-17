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
