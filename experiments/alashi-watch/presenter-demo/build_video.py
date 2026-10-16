"""Rebuild the founder video from an explicit, reviewable edit decision list."""
import argparse,json,os,subprocess
from pathlib import Path
HERE=Path(__file__).resolve().parent
FF=os.environ.get('ALASHI_FFMPEG','/private/tmp/alashi-film-tools/imageio_ffmpeg/binaries/ffmpeg-macos-x86_64-v7.1')
def run(args):
 subprocess.run([FF,'-hide_banner','-loglevel','error','-y',*args],check=True)
def stamp(t):
 h=int(t//3600);m=int(t%3600//60);s=t%60;return f'{h}:{m:02}:{s:05.2f}'
def write_ass(edit,words):
 head='''[Script Info]
ScriptType: v4.00+
PlayResX: 1280
PlayResY: 720
WrapStyle: 0
[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Speech,Arial,26,&H00FFFFFF,&H00FFFFFF,&H00100B18,&HA0100B18,0,0,0,0,100,100,0,0,1,1,0,2,42,42,16,1
Style: Title,Arial,25,&H00FFFFFF,&H00FFFFFF,&H00140F1C,&H00140F1C,-1,0,0,0,100,100,0,0,3,9,0,8,280,430,20,1
Style: Note,Arial,18,&H0099D9FF,&H00FFFFFF,&H00140F1C,&H00140F1C,0,0,0,0,100,100,0,0,3,8,0,8,280,430,60,1
Style: Intro,Arial,28,&H00FFFFFF,&H00FFFFFF,&H00201623,&H00201623,-1,0,0,0,100,100,0,0,3,12,0,7,42,42,42,1
[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
'''
 lines=[]
 def cue(a,b,s,style='Speech'):
  s=s.replace('{','').replace('}','');lines.append(f'Dialogue: 0,{stamp(a)},{stamp(b)},{style},,0,0,0,,{s}')
 offset=0
 for cut_index,cut in enumerate(edit['cuts']):
  a,b=cut['in'],cut['out']
  if cut_index==0:
   for ca,cb,line in [(0.11,1.30,'Hi, my name is Amir.'),(1.69,3.60,"I'm a Tomorrow School student,"),(3.60,5.85,'and I used to be an economist.'),(6.15,8.55,'I had no experience in coding at all.')]:cue(ca,cb,line)
   offset+=b-a
   continue
  selected=[w for w in words if w['start']>=a-.03 and w['end']<=b+.03]
  group=[]
  for i,w in enumerate(selected):
   group.append(w)
   joined=''.join(x['word'] for x in group).strip()
   if len(joined)>=48 or (w['word'].endswith(('.', '?', '!')) and len(joined)>8) or i==len(selected)-1:
    joined=joined.replace('tomorrow school','Tomorrow School').replace('alash.network','ALASHI.network').replace('Alashin Network','ALASHI Network').replace('Starcraft','StarCraft')
    cue(offset+max(0,group[0]['start']-a),min(offset+b-a,offset+group[-1]['end']-a+.10),joined);group=[]
  offset+=b-a
 cue(.15,edit['intro_seconds']-.15,'AMIR\\NTomorrow School student','Intro')
 for c in edit['chapters']:
  cue(c['start'],c['end'],c['title'],'Title')
  if c.get('note'):cue(c['start'],c['end'],c['note'],'Note')
 # Adjacent captions must not overlap and jump to a second baseline.
 speech=[i for i,line in enumerate(lines) if ',Speech,' in line]
 def seconds(value):
  h,m,s=value.split(':');return int(h)*3600+int(m)*60+float(s)
 for previous,current in zip(speech,speech[1:]):
  parts=lines[previous].split(',',9);following=lines[current].split(',',9)
  if seconds(parts[2])>seconds(following[1]):parts[2]=following[1];lines[previous]=','.join(parts)
 (HERE/'captions.ass').write_text(head+'\n'.join(lines)+'\n')
 (HERE/'screen-copy.txt').write_text('\n'.join(c['title']+'\n'+c.get('note','') for c in edit['chapters'])+'\nAMIR\nTomorrow School student\n')
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--source',required=True);p.add_argument('--work',default='/private/tmp/alashi-presenter');p.add_argument('--captions-only',action='store_true');p.add_argument('--start-index',type=int,default=0);p.add_argument('--hardware',action='store_true');a=p.parse_args();work=Path(a.work);work.mkdir(exist_ok=True)
 edit=json.loads((HERE/'edit.json').read_text());segments=json.loads((HERE/'source-alignment.json').read_text());words=[w for s in segments for w in s['words']]
 assert 110<=sum(c['out']-c['in'] for c in edit['cuts'])<=120
 assert edit['cuts'][0]['in']<=.66 and edit['cuts'][0]['out']>=9.06
 write_ass(edit,words)
 if a.captions_only:raise SystemExit
 clips=[]
 for i,c in enumerate(edit['cuts']):
  path=work/f'cut-{i:02}.mp4';clips.append(path)
  if i<a.start_index:
   assert path.exists(),path
   continue
  run((['-hwaccel','videotoolbox'] if a.hardware else [])+['-ss',str(c['in']),'-i',a.source,'-t',str(c['out']-c['in']),'-map','0:v:0','-map','0:a:0','-vf','scale=1280:720,fps=30,format=yuv420p','-af','aresample=48000,asetpts=PTS-STARTPTS,afade=t=in:d=0.015,afade=t=out:st='+str(max(0,c['out']-c['in']-.015))+':d=0.015','-c:v','libx264','-preset','veryfast','-crf','21','-threads','2','-c:a','aac','-b:a','160k',str(path)])
  print(f'Cut {i+1}/{len(edit["cuts"])} ready',flush=True)
 (work/'concat.txt').write_text('\n'.join("file '"+str(x)+"'" for x in clips)+'\n')
 run(['-f','concat','-safe','0','-i',str(work/'concat.txt'),'-c','copy',str(work/'presenter-cut.mp4')])
 # The camera inset sits beside the routes, above the subtitle band.
 graph="[0:v]setpts=PTS-STARTPTS,tpad=stop_mode=clone:stop_duration=2,crop=506:506:413:107,scale=224:224,format=rgb24[face];[face][3:v]alphamerge[circle];[1:v][circle]overlay=1028:429[mapcam];[mapcam][2:v]overlay=0:0:eof_action=pass:repeatlast=0:enable='lt(t,"+str(edit['intro_seconds'])+")'[v];[v]ass='"+str(HERE/'captions.ass')+"'[out];[0:a]loudnorm=I=-16:TP=-1.5:LRA=9,aresample=48000,apad=whole_dur=120[a]"
 run(['-i',str(work/'presenter-cut.mp4'),'-i',str(work/'map.mp4'),'-i',str(work/'cut-00.mp4'),'-loop','1','-framerate','30','-i',str(HERE/'circle-mask.png'),'-filter_complex_threads','1','-filter_complex',graph,'-map','[out]','-map','[a]','-map_metadata','-1','-t','120','-r','30','-c:v','libx264','-preset','veryfast','-crf','20','-threads','2','-pix_fmt','yuv420p','-c:a','aac','-b:a','160k','-ar','48000','-movflags','+faststart',str(HERE/'ALASHI-demo-english.mp4')])
 print('Final video ready',flush=True)
