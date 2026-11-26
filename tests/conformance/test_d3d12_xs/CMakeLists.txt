# M1-C1 (#414): D3D12XboxCreateDevice + COM proxy conformance (device, queue, allocator, list, root signature).
add_executable(test_d3d12_xs main.cpp)
target_include_directories(test_d3d12_xs PRIVATE "${BX_ABI_INCLUDE_DIR}")
target_link_libraries(test_d3d12_xs PRIVATE d3d12_xs bx_runtime_veh kernel32)
