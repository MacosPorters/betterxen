# Cross-compile the Windows texture workload.
# CC_WIN may be x86_64-w64-mingw32-gcc or "clang --target=x86_64-w64-mingw32".

CC_WIN ?= x86_64-w64-mingw32-gcc
CC_HOST ?= cc
CFLAGS_WIN ?= -std=c11 -O2 -Wall -Wextra -Werror -mconsole
LIBS_WIN ?= -ld3d11 -ldxgi -ld3dcompiler -lole32

.PHONY: all check-reference clean

all: d3d11-texture.exe

sample_hlsl.h: sample.hlsl embed_hlsl.py
	python3 embed_hlsl.py sample.hlsl sample_hlsl.h

d3d11-texture.exe: main.c texture_ref.c texture_ref.h sample_hlsl.h
	$(CC_WIN) $(CFLAGS_WIN) -I. -o $@ main.c texture_ref.c $(LIBS_WIN)

check-reference: texture_ref.c texture_ref.h host/ref_selftest.c
	$(CC_HOST) -std=c11 -O2 -Wall -Wextra -Werror -I. -o ref_selftest host/ref_selftest.c texture_ref.c
	./ref_selftest

clean:
	rm -f d3d11-texture.exe ref_selftest sample_hlsl.h
