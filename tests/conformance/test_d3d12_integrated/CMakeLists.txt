# SPDX-License-Identifier: GPL-3.0-or-later
add_executable(d3d12-integrated integrated12.c)
target_compile_options(d3d12-integrated PRIVATE -Wall -Wextra -Werror)
target_link_options(d3d12-integrated PRIVATE -Wl,--no-insert-timestamp)
target_link_libraries(d3d12-integrated PRIVATE d3d12 dxgi dxguid d3dcompiler psapi user32)
