# SPDX-License-Identifier: GPL-3.0-or-later
add_executable(d3d12-lifetime lifetime.c)
target_compile_options(d3d12-lifetime PRIVATE -Wall -Wextra -Werror)
target_link_options(d3d12-lifetime PRIVATE -Wl,--no-insert-timestamp)
target_link_libraries(d3d12-lifetime PRIVATE d3d12 dxgi dxguid user32)
