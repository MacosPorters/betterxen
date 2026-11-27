bx_add_shim_dll(gameinput GameInput GameInput.def
    gameinput.cpp
    gameinput_allocation.cpp
)
target_link_libraries(gameinput PRIVATE bx_runtime_veh kernel32)
target_compile_options(gameinput PRIVATE -fexceptions)
