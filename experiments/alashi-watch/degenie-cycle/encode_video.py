from pathlib import Path
import json,sys,subprocess,shutil
ROOT=Path(__file__).resolve().parent
sys.path.insert(0,str(ROOT/'tools'))
import imageio_ffmpeg
exe=imageio_ffmpeg.get_ffmpeg_exe()
frames=[ROOT/'frames'/f'{i:04d}.jpg' for i in range(420)]
missing=[str(p.name) for p in frames if not p.exists()]
if missing:raise SystemExit(f'Incomplete render: {len(missing)} missing frames, first {missing[:5]}')
subprocess.run([exe,'-y','-framerate','15','-i',str(ROOT/'frames'/'%04d.jpg'),'-frames:v','420','-c:v','libx264','-preset','medium','-crf','18','-pix_fmt','yuv420p','-movflags','+faststart',str(ROOT/'degenie-cycle.mp4')],check=True)
shutil.copyfile(frames[0],ROOT/'poster.jpg')
for frame,name in [(22,'order'),(90,'outbound'),(157,'terminal'),(224,'report'),(280,'return'),(366,'handoff'),(395,'accepted')]:shutil.copyfile(frames[frame],ROOT/'evidence'/f'final-{name}.jpg')
print('VIDEO_READY',ROOT/'degenie-cycle.mp4')
