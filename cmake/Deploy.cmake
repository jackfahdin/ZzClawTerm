# 构建后部署脚本 (由 POST_BUILD 以 cmake -P 调用)。
# 设计目标: 部署失败 (例如目标 exe 正在运行被占用) 时仅警告, 不终止构建。
#
# 入参 (-D):
#   ZZ_EXE       可执行文件 (或 macOS 上的 .app 目录)
#   ZZ_DIST      部署输出目录
#   ZZ_PLATFORM  WIN / MAC / OTHER
#   ZZ_TOOL      windeployqt / macdeployqt 路径 (可空)

file(MAKE_DIRECTORY "${ZZ_DIST}")

if(ZZ_PLATFORM STREQUAL "MAC")
    get_filename_component(_name "${ZZ_EXE}" NAME)
    set(_dest "${ZZ_DIST}/${_name}")
    execute_process(
        COMMAND "${CMAKE_COMMAND}" -E copy_directory "${ZZ_EXE}" "${_dest}"
        RESULT_VARIABLE _res)
    if(NOT _res EQUAL 0)
        message(WARNING "部署: 无法更新 ${_dest} (可能正在运行), 跳过本次部署。")
        return()
    endif()
    if(ZZ_TOOL)
        execute_process(COMMAND "${ZZ_TOOL}" "${_dest}" RESULT_VARIABLE _dep)
        if(NOT _dep EQUAL 0)
            message(WARNING "部署: macdeployqt 失败, 跳过。")
        endif()
    endif()
    return()
endif()

# Windows / Linux: 拷贝可执行文件。
get_filename_component(_name "${ZZ_EXE}" NAME)
set(_dest "${ZZ_DIST}/${_name}")
execute_process(
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different "${ZZ_EXE}" "${_dest}"
    RESULT_VARIABLE _res)
if(NOT _res EQUAL 0)
    message(WARNING "部署: 无法更新 ${_dest} (可能正在运行), 跳过本次部署。")
    return()
endif()

if(ZZ_PLATFORM STREQUAL "WIN" AND ZZ_TOOL)
    execute_process(
        COMMAND "${ZZ_TOOL}" --release --no-translations --compiler-runtime "${_dest}"
        RESULT_VARIABLE _dep)
    if(NOT _dep EQUAL 0)
        message(WARNING "部署: windeployqt 失败, 跳过。")
    endif()
endif()
