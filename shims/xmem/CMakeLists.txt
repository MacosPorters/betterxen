bx_add_shim_dll(xmem xmem xmem.def
    xmem.cpp
    physical_pages.cpp
)
target_link_libraries(xmem PRIVATE bx_runtime_veh kernel32)
