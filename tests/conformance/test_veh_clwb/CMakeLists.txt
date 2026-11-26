add_executable(test_veh_clwb main.cpp)
target_include_directories(test_veh_clwb PRIVATE "${BX_ABI_INCLUDE_DIR}")
target_compile_options(test_veh_clwb PRIVATE -mavx2 -mfma)
target_link_libraries(test_veh_clwb PRIVATE bx_runtime_veh xmem kernel32)

add_executable(test_veh_unhandled main_unhandled.cpp)
target_link_libraries(test_veh_unhandled PRIVATE bx_runtime_veh kernel32)
