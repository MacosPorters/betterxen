#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Independent interval-union validation; does not use the C++ free-list implementation."""
import argparse
import json
from pathlib import Path


def verify(path):
    live, pending = {}, {}
    capacity = None
    checked = 0
    failed_requests = 0
    completion = 0
    labels = {}
    def holes():
        ranges = sorted((r['offset'], r['offset'] + r['size']) for r in [*live.values(), *pending.values()])
        result = []
        end = 0
        for start, stop in ranges:
            assert start >= end and stop <= capacity, 'overlap or bounds'
            if start > end:
                result.append((end, start))
            end = stop
        if end < capacity:
            result.append((end, capacity))
        return result
    for line in path.read_text().splitlines():
        row = json.loads(line)
        op = row['op']
        if op == 'begin':
            assert capacity is None
            capacity = row['capacity']
        elif op in ['allocate', 'failed_allocate']:
            assert row['size'] == ((row['payload'] + 65535)//65536)*65536
            if op == 'failed_allocate':
                failed_requests += 1
                assert not any(((start+row['alignment']-1)//row['alignment'])*row['alignment']+row['size'] <= end for start,end in holes()), 'false exhaustion'
            else:
                assert row['id'] not in live and row['id'] not in pending
                assert row['offset'] % row['alignment'] == 0
                assert any(start <= row['offset'] and row['offset']+row['size'] <= end for start,end in holes())
                live[row['id']] = row
        elif op == 'request_free':
            assert row['id'] in live
            pending[row['id']] = dict(live.pop(row['id']), fence=row['fence'])
        elif op == 'signal':
            assert row['completed'] >= completion
            completion = row['completed']
            pending = {k:v for k,v in pending.items() if v['fence'] > completion}
        elif op == 'stats':
            gaps = holes()
            payload = sum(v['payload'] for v in live.values())
            reserved = sum(v['size'] for v in live.values())
            held = sum(v['size'] for v in pending.values())
            free = sum(end-start for start,end in gaps)
            largest = max((end-start for start,end in gaps),default=0)
            usable = max((max(0,end-((start+2097151)//2097152)*2097152) for start,end in gaps),default=0)
            expected = dict(capacity=capacity,live_payload=payload,live_reserved=reserved,pending_reserved=held,
                            internal_waste=reserved-payload,total_free=free,largest_free=largest,
                            largest_2MiB_aligned_span=usable,free_spans=len(gaps))
            for k,v in expected.items():
                assert row[k] == v, (k,row[k],v)
            assert reserved + held + free == capacity
            if row['label'] != 'varied_trace':
                labels[row['label']] = row
            checked += 1
        elif op == 'end':
            assert row['success'] and not live and not pending
            assert holes() == [(0,capacity)]
        else:
            raise AssertionError('unexpected record '+op)
    assert checked == 2004
    assert labels['alternating_holes']['total_free'] == capacity//2
    assert labels['alternating_holes']['largest_free'] == 4*1024**2
    assert labels['pending_blocks_reuse']['pending_reserved'] == capacity//2
    assert labels['fully_coalesced']['total_free'] == capacity
    assert labels['final_empty']['total_free'] == capacity
    return dict(file=path.name,stats_checked=checked,failed_allocation_requests=failed_requests,labels=labels,
                scope='logical interval model; not GPU, physical allocation or independent contributor acceptance')


def main():
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('stdout',type=Path);ap.add_argument('--output',required=True,type=Path);args=ap.parse_args()
    result=verify(args.stdout);args.output.write_text(json.dumps(result,indent=2)+'\n');print('verified',result['stats_checked'],'logical snapshots')

if __name__=='__main__':main()
