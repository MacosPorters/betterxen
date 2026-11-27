add_executable(test_gameinput main.cpp)
target_link_libraries(test_gameinput PRIVATE gameinput kernel32)
target_include_directories(test_gameinput PRIVATE "${BX_ROOT_DIR}/shims/gameinput")
target_compile_options(test_gameinput PRIVATE -fexceptions)
