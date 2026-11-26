add_executable(test_audio_input main.cpp)
target_link_libraries(test_audio_input PRIVATE acphal xapu chad kernel32)
add_dependencies(test_audio_input gameinput)
