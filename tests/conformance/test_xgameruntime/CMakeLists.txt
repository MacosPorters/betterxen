add_executable(test_xgameruntime main.cpp)
target_link_libraries(test_xgameruntime PRIVATE xgameruntime kernel32)

# Keep the same executable name to exercise the marker check and storage isolation.
add_executable(test_xgameruntime_unentitled main.cpp)
target_compile_definitions(test_xgameruntime_unentitled PRIVATE BX_TEST_UNENTITLED)
target_link_libraries(test_xgameruntime_unentitled PRIVATE xgameruntime kernel32)
set_target_properties(test_xgameruntime_unentitled PROPERTIES
    OUTPUT_NAME test_xgameruntime
    RUNTIME_OUTPUT_DIRECTORY "${CMAKE_RUNTIME_OUTPUT_DIRECTORY}/unentitled"
)
