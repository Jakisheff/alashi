"""Create the Full HD demo delivery copy with conservative H.264 settings."""
import argparse, os, subprocess
from pathlib import Path
p=argparse.ArgumentParser()
p.add_argument('source',type=Path)
p.add_argument('output',type=Path)
a=p.parse_args()
if os.environ.get('ALASHI_FFMPEG'):
    binary=os.environ['ALASHI_FFMPEG']
else:
    import imageio_ffmpeg
    binary=imageio_ffmpeg.get_ffmpeg_exe()
subprocess.run([binary,'-hide_banner','-y','-i',str(a.source),'-vf',
    'scale=1920:1080:in_range=pc:out_range=tv,format=yuv420p',
    '-c:v','libx264','-preset','veryfast','-crf','19','-profile:v','main',
    '-level:v','4.1','-pix_fmt','yuv420p','-color_range','tv',
    '-color_primaries','bt709','-color_trc','bt709','-colorspace','bt709',
    '-tag:v','avc1','-c:a','aac','-ar','48000','-ac','2','-b:a','128k',
    '-map_metadata','-1','-movflags','+faststart',str(a.output)],check=True)
