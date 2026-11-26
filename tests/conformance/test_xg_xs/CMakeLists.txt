add_executable(test_xg_xs main.cpp)
target_include_directories(test_xg_xs PRIVATE
    "${BX_ABI_INCLUDE_DIR}"
    "${CMAKE_CURRENT_SOURCE_DIR}/../../../shims/xg_xs")
target_link_libraries(test_xg_xs PRIVATE xg_xs kernel32)
