bx_add_shim_dll(d3d12_xs d3d12_xs d3d12_xs.def
    d3d12_xs.cpp
    proxy.cpp
    proxy_generated.cpp
)
target_link_libraries(d3d12_xs PRIVATE bx_runtime_veh xmem kernel32)
