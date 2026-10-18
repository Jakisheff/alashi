"""Inspect and decode the delivered MP4 with PyAV, independent of rendering."""
import av,hashlib,json
from pathlib import Path
HERE=Path(__file__).resolve().parent
path=HERE/'ALASHI-demo-english.mp4'
with av.open(str(path)) as c:
 v=c.streams.video[0];a=c.streams.audio[0]
 result={'file':path.name,'bytes':path.stat().st_size,'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'duration_seconds':c.duration/1000000,'width':v.width,'height':v.height,'fps':float(v.average_rate),'video_codec':v.codec_context.name,'audio_codec':a.codec_context.name,'sample_rate':a.codec_context.sample_rate}
with av.open(str(path)) as c:
 frames=0;previous=-1
 for f in c.decode(video=0):
  assert f.time>previous,(previous,f.time)
  previous=f.time;frames+=1
result['decoded_video_frames']=frames
with av.open(str(path)) as c:
 samples=sum(f.samples for f in c.decode(audio=0))
result['decoded_audio_samples']=samples
assert frames==3600,result
assert abs(result['duration_seconds']-120)<.1,result
assert result['fps']==30 and result['width']==1280 and result['height']==720
assert result['video_codec']=='h264' and result['audio_codec']=='aac'
assert 119.9<samples/result['sample_rate']<120.1
result['passed']=True
(HERE/'media-check.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
