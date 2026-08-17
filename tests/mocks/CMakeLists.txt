add_library(ZzTestMocks STATIC
    ZzMockTransport.h
    ZzMockTransport.cpp
)
target_include_directories(ZzTestMocks PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(ZzTestMocks PUBLIC ZzClawTermApp Qt6::Core)
