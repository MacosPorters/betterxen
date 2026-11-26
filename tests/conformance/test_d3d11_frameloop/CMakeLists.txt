# SPDX-License-Identifier: GPL-3.0-or-later
add_executable(d3d11-frameloop frameloop.c)
target_compile_options(d3d11-frameloop PRIVATE -Wall -Wextra -Werror)
target_link_options(d3d11-frameloop PRIVATE -Wl,--no-insert-timestamp)
target_link_libraries(d3d11-frameloop PRIVATE d3d11 dxgi dxguid d3dcompiler m)
