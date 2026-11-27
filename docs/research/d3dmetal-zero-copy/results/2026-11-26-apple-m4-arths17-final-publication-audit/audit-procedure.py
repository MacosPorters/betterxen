from pathlib import Path
import subprocess,json,hashlib,re
from datetime import datetime,timezone
repo=Path.cwd();root=repo/'docs/research/d3dmetal-zero-copy';out=repo/'artifacts/workstream-a/final-publication';out.mkdir(exist_ok=True)
files=sorted(set(x for x in subprocess.check_output(['git','ls-files','--modified','--others','--exclude-standard','-z']).decode().split('\0') if x))
allowed=['CHANGELOG.md','docs/README.md','docs/research/component-reuse-matrix.md','docs/research/upstream-registry.md']
checks={'utc':datetime.now(timezone.utc).isoformat(),'candidate_files':len(files),'candidate_bytes':0,'safety_findings':[],'archive_files':0,'source_references':0,'source_matches':0,'missing_source_references':[],'candidate_manifest':[]}
patterns={'personal_path':rb'(?:/Users/[^/\s"<>]+|[A-Za-z]:\\(?:Users|home)\\[^\\\s"<>]+)','credential':rb'(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,}|-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----|AKIA[0-9A-Z]{16})'}
source_hashes={}
for p in root.rglob('*'):
 if p.is_file() and p.suffix in ['.cpp','.py','.h','.hlsl']:
  source_hashes.setdefault(hashlib.sha256(p.read_bytes()).hexdigest(),[]).append(str(p.relative_to(repo)))
for f in files:
 if f not in allowed and not f.startswith('docs/research/d3dmetal-zero-copy/'):raise RuntimeError('scope: '+f)
 p=repo/f;b=p.read_bytes();checks['candidate_bytes']+=len(b);checks['candidate_manifest'].append({'path':f,'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()})
 if p.is_symlink() or p.suffix in ['.exe','.dll','.lock','.pyc'] or b[:2]==b'MZ' or b[:4] in [bytes.fromhex(x) for x in ['cffaedfe','feedfacf','cafebabe','bebafeca']]:raise RuntimeError('forbidden payload: '+f)
 for name,pat in patterns.items():
  if re.search(pat,b):checks['safety_findings'].append({'path':f,'kind':name})
for manifest in (root/'results').glob('*/archive-manifest.json'):
 for row in json.loads(manifest.read_text())['files']:
  p=manifest.parent/row['path'];b=p.read_bytes()
  if hashlib.sha256(b).hexdigest()!=row['archived_sha256']:raise RuntimeError('archive hash: '+str(p))
  checks['archive_files']+=1
for receipt in (root/'results').glob('*/receipt.json'):
 d=json.loads(receipt.read_text())
 for label in ['sources','source_hashes']:
  for ref,digest in d.get(label,{}).items():
   if not isinstance(digest,str) or not re.fullmatch('[a-f0-9]{64}',digest):continue
   checks['source_references']+=1
   if digest in source_hashes:checks['source_matches']+=1
   else:checks['missing_source_references'].append({'receipt':str(receipt.relative_to(repo)),'source':ref,'sha256':digest})
checks['baseline_sha256']=hashlib.sha256((root/'probe.cpp').read_bytes()).hexdigest()
if checks['baseline_sha256']!='33e90bfee89e812f48990613cd74ced56473e5c3fc5d973f2ba1116090646a0c':raise RuntimeError('baseline changed')
for base,paths in [('9ae6d1ee7092b6021aad075531f50ba60de4fd53',['docs/research/d3dmetal-zero-copy/probe.cpp','docs/research/d3dmetal-zero-copy/run.py']),('HEAD',['abi','shims','launcher','shader-transpiler'])]:
 if subprocess.check_output(['git','diff',base,'--',*paths]):raise RuntimeError('protected files changed')
checks['external_files']=[]
for f,expected in [('LICENSE.txt','3d5a69a6f5695507ecc9c412c783ca60572bf878'),('D3D12MemAlloc.h','b685a5cd6188519606efaf039cd88c2faffa261a'),('D3D12MemAlloc.cpp','b7ba60a3a4deb0ed466b235b7c5298f91ba5c976')]:
 p=repo/'artifacts/workstream-a/allocator-reference-01/upstream'/f;b=p.read_bytes();actual=hashlib.sha1(b'blob '+str(len(b)).encode()+b'\0'+b).hexdigest()
 if actual!=expected:raise RuntimeError('external source mismatch '+f)
 checks['external_files'].append({'file':f,'git_blob':actual,'sha256':hashlib.sha256(b).hexdigest(),'public_payload':False})
checks['kernel_pressure']=subprocess.check_output(['sysctl','-n','kern.memorystatus_vm_pressure_level'],text=True).strip()
checks['memory_heavy_checks']='blocked by urgent pressure / 23 GB reserve; earlier exact-source compile receipts retained'
(out/'audit.json').write_text(json.dumps(checks,indent=2)+'\n')
if checks['safety_findings']:raise RuntimeError(checks['safety_findings'])
print(json.dumps({k:v for k,v in checks.items() if k not in ['candidate_manifest','external_files']},indent=2))
