add_executable(test_win32_bridge main.cpp)
target_link_libraries(test_win32_bridge PRIVATE kernel32 user32)
add_dependencies(test_win32_bridge
    psm_appnotify_1_0 psm_appnotify_1_1
    ntuser_msg_1_0 ntuser_msg_ansi_1_0 ntuser_win_1_0 ntuser_win_ansi_1_0
)
