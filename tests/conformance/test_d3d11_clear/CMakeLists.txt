# SPDX-License-Identifier: GPL-3.0-or-later
add_executable(d3d11-clear d3d11_clear.c)
target_compile_options(d3d11-clear PRIVATE -Wall -Wextra -Werror)
target_link_options(d3d11-clear PRIVATE -Wl,--no-insert-timestamp)
target_link_libraries(d3d11-clear PRIVATE d3d11 dxgi)
