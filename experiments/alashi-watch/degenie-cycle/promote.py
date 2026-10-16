"""Make the completed Degenie episode the default local preview; preserve the old prototype."""
from pathlib import Path
import shutil,json,hashlib
root=Path(__file__).resolve().parent
parent=root.parent
assert (root/'degenie-cycle.mp4').is_file(),'Encode the full episode first'
assert (root/'poster.jpg').is_file()
source=(root/'index.html').read_text()
assert '<head>' in source
page=source.replace('<head>','<head><base href="degenie-cycle/">',1)
backups={}
for name,backup in [('alashi-watch.html','alashi-watch-dots.html'),('index.html','index-dots.html')]:
 old=parent/name;copy=parent/backup
 if old.exists() and not copy.exists():shutil.copyfile(old,copy)
 if copy.exists():backups[backup]=hashlib.sha256(copy.read_bytes()).hexdigest()
 old.write_text(page)
(root/'evidence'/'preview-promotion.json').write_text(json.dumps({'old_prototype_backups':backups,'entrypoints':['../index.html','../alashi-watch.html','index.html']},indent=2)+'\n')
print('LOCAL_PREVIEW_PROMOTED')
