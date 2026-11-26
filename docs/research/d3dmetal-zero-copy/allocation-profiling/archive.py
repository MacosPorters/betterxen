#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Append a redacted diagnostic copy and hash manifest; original bytes stay untouched."""
import argparse
import hashlib
import json
from pathlib import Path


def main():
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('scratch',type=Path);ap.add_argument('destination',type=Path);args=ap.parse_args()
    src=args.scratch.resolve();dst=args.destination.resolve()
    if dst.exists():ap.error('append-only destination must be new')
    dst.mkdir(parents=True)
    manifest=dict(originals_unchanged=True,redaction='JSON diagnostics replace local home-directory prefix with <HOME>; raw stdout/stderr unchanged',files=[])
    for p in sorted(src.rglob('*')):
        if not p.is_file() or p.is_symlink():continue
        rel=p.relative_to(src)
        if p.suffix=='.exe' or p.name=='observe' or any(v.startswith('.wine') or v=='bottle' for v in rel.parts):continue
        original=p.read_bytes()
        if original[:2]==b'MZ' or original[:4] in [b'\xcf\xfa\xed\xfe',b'\xfe\xed\xfa\xcf',b'\xca\xfe\xba\xbe',b'\xbe\xba\xfe\xca']:
            continue  # Executables remain in ignored artifacts, not public research records.
        data=original
        if p.suffix=='.json':data=data.replace(str(Path.home()).encode(),b'<HOME>')
        target=dst/rel;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data)
        manifest['files'].append(dict(path=rel.as_posix(),bytes=len(data),original_sha256=hashlib.sha256(original).hexdigest(),archived_sha256=hashlib.sha256(data).hexdigest(),redacted=original!=data))
    (dst/'archive-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    print('Archived',len(manifest['files']),'files to',dst)

if __name__=='__main__':main()
