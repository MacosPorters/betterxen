add_executable(test_xcrdapi main.cpp)
target_compile_definitions(test_xcrdapi PRIVATE WIDL_EXPLICIT_AGGREGATE_RETURNS)
target_include_directories(test_xcrdapi PRIVATE "${BX_ROOT_DIR}/shims/xcrdapi" "${BX_ROOT_DIR}/shims/xcrdapi/dstorage")
target_link_libraries(test_xcrdapi PRIVATE xcrdapi d3d12_xs kernel32 uuid)
