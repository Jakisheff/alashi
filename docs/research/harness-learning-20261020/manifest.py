#!/usr/bin/env python3
"""Hash portable research artifacts; exclude generated binaries and this manifest."""
import json,sys
from pathlib import Path
import common as c
ROOT=Path(__file__).resolve().parent

def files():
 return sorted(p for p in ROOT.rglob('*') if p.is_file() and p.name!='artifact-manifest.json' and not p.name.endswith('.tmp') and not any(x in ['bin','__pycache__','target'] for x in p.relative_to(ROOT).parts))
def main():
 current={str(p.relative_to(ROOT)):c.sha(p) for p in files()}
 if '--check' in sys.argv:
  saved=json.loads((ROOT/'artifact-manifest.json').read_text());assert saved['files']==current;print('Manifest matches',len(current),'portable files')
 else:c.write_json(ROOT/'artifact-manifest.json',{'utc':c.utc(),'algorithm':'SHA-256','excluded':['bin/','target/','__pycache__/','*.tmp','artifact-manifest.json'],'count':len(current),'files':current});print('Manifest saved:',len(current),'portable files')
if __name__=='__main__':main()
