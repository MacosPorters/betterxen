# M1-F1 (#420): synthetic mini-engine boot fixture.
#
# Links statically (PE import directory, not LoadLibrary) against the import
# library of every BetterXen shim DLL so the Windows loader must resolve all 16
# before main() runs. The ntuser forwarder libraries are listed first so that
# CreateWindowExA & co. bind to the ext-ms-win-rtcore-ntuser-* shims instead of
# the toolchain's default user32 import library.
add_executable(test_integration_boot main.cpp)
target_include_directories(test_integration_boot PRIVATE "${BX_ABI_INCLUDE_DIR}")
target_link_libraries(test_integration_boot PRIVATE
    ntuser_win_ansi_1_0
    ntuser_win_1_0
    ntuser_msg_ansi_1_0
    ntuser_msg_1_0
    psm_appnotify_1_0
    psm_appnotify_1_1
    bx_runtime_veh
    xmem
    xgameruntime
    xcrdapi
    acphal
    xapu
    chad
    gameinput
    xg_xs
    d3d12_xs
    user32
    kernel32
)
