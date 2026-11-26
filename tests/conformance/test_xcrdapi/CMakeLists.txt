add_executable(test_xcrdapi main.cpp)
target_include_directories(test_xcrdapi PRIVATE "${BX_ROOT_DIR}/shims/xcrdapi")
target_link_libraries(test_xcrdapi PRIVATE xcrdapi kernel32 uuid)
