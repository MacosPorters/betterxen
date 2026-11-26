add_executable(test_xmem main.cpp)
target_include_directories(test_xmem PRIVATE "${BX_ABI_INCLUDE_DIR}")
target_link_libraries(test_xmem PRIVATE xmem bx_runtime_veh kernel32)
