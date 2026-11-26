bx_add_shim_dll(xcrdapi XCRDAPI XCRDAPI.def
    xcrdapi.cpp
)
target_link_libraries(xcrdapi PRIVATE bx_runtime_veh kernel32 uuid)
