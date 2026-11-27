# M2-C1 (#464): D3D12ConfigureVirtualMemoryX plus placed buffer and RGBA8 texture readback.
add_executable(test_d3d12_placed main.cpp)
target_include_directories(test_d3d12_placed PRIVATE "${BX_ABI_INCLUDE_DIR}")
target_link_libraries(test_d3d12_placed PRIVATE d3d12_xs xmem bx_runtime_veh kernel32)
