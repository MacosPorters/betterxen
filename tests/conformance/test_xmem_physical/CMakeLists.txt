add_executable(test_xmem_physical main.cpp)
target_include_directories(test_xmem_physical PRIVATE "${BX_ABI_INCLUDE_DIR}")
target_link_libraries(test_xmem_physical PRIVATE xmem kernel32)
