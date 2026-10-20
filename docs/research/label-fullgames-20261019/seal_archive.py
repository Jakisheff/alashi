#!/usr/bin/env python3
"""Check for the used credential, then hash the portable research archive."""
import hashlib,json,os,re,zipfile
from pathlib import Path
import run_experiment as c
ROOT=c.ROOT

def files():
 return sorted(p for p in ROOT.rglob('*') if p.is_file() and not any(x in ('bin','target','__pycache__') for x in p.relative_to(ROOT).parts) and not p.name.endswith('.tmp') and p.name!='artifact-manifest.json')
def main():
 key=os.environ.get('ALASHI_ZAI_CODING_KEY') or json.loads((Path.home()/'.local/share/opencode/auth.json').read_text())['zai-coding-plan']['key']
 secret=key.encode();assert len(secret)>15
 paths=files();checked=0
 def check(data,path):
  assert secret not in data,'Credential detected in '+path
  assert not re.search(rb'\bsk-(?:proj-)?[A-Za-z0-9_-]{30,}',data),'Key-like literal in '+path
 for path in paths:
  check(path.read_bytes(),str(path.relative_to(ROOT)));checked+=1
  if path.suffix=='.zip':
   with zipfile.ZipFile(path) as z:
    for name in z.namelist():check(z.read(name),path.name+':'+name);checked+=1
 manifest={'created_utc':c.utc(),'credential_scan_passed':True,'files_or_zip_entries_scanned':checked,'runtime_binary_excluded':True,'runtime_binary_sha256':json.loads((ROOT/'protocol.json').read_text())['runtime_binary_sha256'],'files':{str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}}
 c.write_json(ROOT/'artifact-manifest.json',manifest);print('Portable archive sealed:',len(paths),'files;',checked,'files/zip entries checked; no credentials found')
if __name__=='__main__':main()
