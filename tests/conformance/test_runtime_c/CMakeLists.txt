# SPDX-License-Identifier: GPL-3.0-or-later
add_executable(selfcheck-c selfcheck.c)
target_compile_options(selfcheck-c PRIVATE -Wall -Wextra -Werror)
target_link_options(selfcheck-c PRIVATE -Wl,--no-insert-timestamp)
target_link_libraries(selfcheck-c PRIVATE kernel32)
