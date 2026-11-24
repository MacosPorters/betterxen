# SPDX-License-Identifier: GPL-3.0-or-later
"""Generate the PE32+ trusted runner fixtures without an x86 toolchain."""

import argparse
from pathlib import Path
from struct import pack, pack_into


def fixture(runtime_services=False):
    base = 0x140000000
    entry, callback = 0x1000, 0x1040
    imports, marker, reloc = 0x2000, 0x3000, 0x4000
    iat, lookup, module, symbol = (imports + x for x in (0x40, 0x50, 0x60, 0x80))
    tls, callbacks, marker_ptr = (imports + x for x in (0xA0, 0xD0, 0xE0))

    data = bytearray(0xC00)
    data[:2] = b"MZ"
    pack_into("<I", data, 0x3C, 0x80)
    data[0x80:0x84] = b"PE\0\0"
    pack_into("<HHIIIHH", data, 0x84, 0x8664, 4, 0, 0, 0, 240, 0x22)
    opt = 0x98
    pack_into("<H", data, opt, 0x20B)
    pack_into("<III", data, opt + 4, 0x200, 0x600, 0)
    pack_into("<IIQII", data, opt + 16, entry, entry, base, 0x1000, 0x200)
    pack_into("<II", data, opt + 56, 0x5000, 0x400)
    pack_into("<HH", data, opt + 68, 3, 0x160)
    pack_into("<QQQQII", data, opt + 72, 0x10000, 0x1000, 0x10000, 0x1000, 0, 16)
    for index, rva, size in ((1, imports, 40), (5, reloc, 16), (9, tls, 40), (12, iat, 16)):
        pack_into("<II", data, opt + 112 + index * 8, rva, size)

    sections = (
        (b".text", entry, 0x60000020),
        (b".rdata", imports, 0x40000040),
        (b".data", marker, 0xC0000040),
        (b".reloc", reloc, 0x42000040),
    )
    for index, (name, rva, permissions) in enumerate(sections):
        pack_into(
            "<8sIIIIIIHHI", data, opt + 240 + 40 * index, name, 0x200, rva,
            0x200, 0x400 + 0x200 * index, 0, 0, 0, 0, permissions,
        )

    # Entry reserves the Win64 home area, loads the relocated marker pointer,
    # calls the single synthetic import through its IAT, and halts.
    code = b"\x48\x83\xec\x28"
    code += b"\x48\x8b\x0d" + pack("<i", marker_ptr - (entry + 11))
    code += b"\xff\x15" + pack("<i", iat - (entry + 17))
    code += b"\x48\x83\xc4\x28\xf4"
    data[0x400:0x400 + len(code)] = code

    # TLS callback writes the draw ticket into .data, then returns.
    code = b"\x48\x8b\x05" + pack("<i", marker_ptr - (callback + 7))
    code += b"\x48\xc7\x00" + pack("<I", 17) + b"\xc3"
    data[0x440:0x440 + len(code)] = code

    pack_into("<IIIII", data, 0x600, lookup, 0, 0, module, iat)
    pack_into("<Q", data, 0x640, symbol)
    pack_into("<Q", data, 0x650, symbol)
    data[0x660:0x66B] = b"xenora.dll\0"
    data[0x682:0x68E] = b"fixture_draw"
    pack_into("<QQQQII", data, 0x6A0, 0, 0, base + marker + 8, base + callbacks, 0, 0)
    pack_into("<Q", data, 0x6D0, base + callback)
    pack_into("<Q", data, 0x6E0, base + marker)
    # DIR64 fixups for the TLS index/array/callback pointers and the draw ticket pointer.
    pack_into("<IIHHHH", data, 0xA00, imports, 16, 0xA0B0, 0xA0B8, 0xA0D0, 0xA0E0)
    if runtime_services:
        iat = imports + 0xF0
        service_iat, service_lookup, kernel, set_error, get_error = (
            imports + offset for offset in (0x100, 0x120, 0x140, 0x150, 0x160)
        )
        pack_into("<I", data, opt + 124, 60)
        pack_into("<II", data, opt + 112 + 12 * 8, iat, 40)
        pack_into("<I", data, 0x610, iat)
        pack_into("<Q", data, 0x6F0, symbol)
        pack_into("<IIIII", data, 0x614, service_lookup, 0, 0, kernel, service_iat)
        pack_into("<QQ", data, 0x700, set_error, get_error)
        pack_into("<QQ", data, 0x720, set_error, get_error)
        data[0x740:0x74D] = b"kernel32.dll\0"
        data[0x752:0x75E] = b"SetLastError"
        data[0x762:0x76E] = b"GetLastError"

        code = bytearray(b"\x48\x83\xec\x28")
        code += b"\xb9" + pack("<I", 23)
        code += b"\xff\x15" + pack("<i", service_iat - (entry + len(code) + 6))
        code += b"\xff\x15" + pack("<i", service_iat + 8 - (entry + len(code) + 6))
        code += b"\x48\x8b\x0d" + pack("<i", marker_ptr - (entry + len(code) + 7))
        code += b"\x48\x89\x01"  # Store the retrieved value for the draw handler.
        code += b"\xff\x15" + pack("<i", iat - (entry + len(code) + 6))
        code += b"\x48\x83\xc4\x28\xf4"
        data[0x400:0x400 + len(code)] = code
    return data


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    for name, runtime_services in (("trusted-runner", False), ("runtime-services", True)):
        path = Path(__file__).with_name(f"{name}.exe")
        if args.check:
            if path.read_bytes() != fixture(runtime_services):
                parser.exit(1, f"fixture bytes differ from generator: {path}\n")
        else:
            path.write_bytes(fixture(runtime_services))
