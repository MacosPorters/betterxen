add_executable(test_veh_amd_only main.cpp)
target_include_directories(test_veh_amd_only PRIVATE "${BX_ABI_INCLUDE_DIR}")
target_link_libraries(test_veh_amd_only PRIVATE bx_runtime_veh kernel32)
