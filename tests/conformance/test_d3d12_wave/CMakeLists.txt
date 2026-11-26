# SPDX-License-Identifier: GPL-3.0-or-later
add_executable(d3d12-wave-probe d3d12_run.c)
target_compile_options(d3d12-wave-probe PRIVATE -Wall -Wextra -Werror)
target_link_options(d3d12-wave-probe PRIVATE -Wl,--no-insert-timestamp)
target_link_libraries(d3d12-wave-probe PRIVATE d3d12 dxgi dxguid)
