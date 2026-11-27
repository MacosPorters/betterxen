bx_add_shim_dll(xcrdapi XCRDAPI XCRDAPI.def
    xcrdapi.cpp
)
target_include_directories(xcrdapi PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}/dstorage")
target_link_libraries(xcrdapi PRIVATE bx_runtime_veh kernel32 uuid)
target_compile_definitions(xcrdapi PRIVATE WIDL_EXPLICIT_AGGREGATE_RETURNS)
