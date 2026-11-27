#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Bounded arithmetic/source counterexamples; no allocation or production code execution."""
import argparse
import hashlib
import json
from pathlib import Path


def cpu_allows(used, request, cap):
    return used <= cap and request <= cap - used


def graphics_allows(snapshot, charged, request, cap):
    return not (request > cap or snapshot > cap or charged > cap - snapshot) and request <= cap - snapshot - charged


def main():
    ap = argparse.ArgumentParser(description=__doc__);ap.add_argument('source_root', type=Path);ap.add_argument('--output', type=Path, required=True);a=ap.parse_args()
    paths=['shims/xmem/xmem.cpp','shims/xmem/physical_pages.cpp','shims/d3d12_xs/vmem.cpp','abi/include/bx_xmem_internal.h']
    text={f:(a.source_root/f).read_text() for f in paths}
    assert 'const uint64_t used = heap_bytes + virtual_bytes + physical_bytes;' in text[paths[0]]
    assert 'return used <= limit && bytes <= limit - used;' in text[paths[0]]
    assert 'charged_private_bytes' not in text[paths[0]]
    assert 'bx_xmem::try_reserve_physical(total_bytes)' in text[paths[1]]
    assert 'const uint64_t xmem_used = stats.committed_virtual_bytes + stats.allocated_physical_bytes;' in text[paths[2]]
    assert 'g_record.charged_private_bytes += bytes;' in text[paths[2]]
    # Values are abstract GiB units, not allocated memory.
    cap=8; x=g=0
    first=graphics_allows(x,g,6,cap);assert first;g+=6
    second=cpu_allows(x,3,cap);assert second;x+=3
    sequential={'cap_units':cap,'graphics_request_units':6,'cpu_request_units':3,'graphics_allowed':first,'cpu_allowed':second,'combined_units':x+g,'combined_cap_exceeded':x+g>cap}
    assert not graphics_allows(3,0,6,cap)  # Reverse-order control sees CPU bytes.
    x=4;g=0;snapshot=x
    assert cpu_allows(x,4,cap);x+=4
    assert graphics_allows(snapshot,g,4,cap);g+=4
    interleaving={'initial_xmem_units':4,'graphics_snapshot_units':snapshot,'intervening_xmem_commit_units':4,'graphics_commit_units':4,'final_combined_units':x+g,'cap_units':cap,'combined_cap_exceeded':x+g>cap}
    result={'scope':'source-derived logical counterexample only; no host memory allocation, shim invocation or production modification','source_revision':'c13f9c76650edded41774528b50f40798244a6f3','source_sha256':{f:hashlib.sha256((a.source_root/f).read_bytes()).hexdigest() for f in paths},'assumptions':['stable common configured cap','underlying host allocations succeed','distinct live CPU and D3D12 allocations, not aliases or double counting'],'sequential':sequential,'reverse_order_control_rejects':True,'snapshot_interleaving':interleaving,'conclusion':'One-way visibility violates combined admission even sequentially; separate locks also permit a stale cross-DLL snapshot. Real shim/backend reproduction remains unrun.'}
    a.output.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))


if __name__=='__main__':main()
