"""Deterministic English RTS presentation layer, using the approved atlas."""
import json, math, subprocess, os
from pathlib import Path
from functools import lru_cache
from PIL import Image, ImageDraw, ImageFont
HERE=Path(__file__).resolve().parent
RTS=HERE.parent/'rts-overview'
W,H,FPS=1280,720,30
FF=os.environ.get('ALASHI_FFMPEG','/private/tmp/alashi-film-tools/imageio_ffmpeg/binaries/ffmpeg-macos-x86_64-v7.1')
C=[(205,167,255),(161,239,230)]
@lru_cache(None)
def font(s,b=False):return ImageFont.truetype('/System/Library/Fonts/Supplemental/Arial'+(' Bold' if b else '')+'.ttf',s)
def text(d,p,s,size=18,fill=(248,241,229),bold=False,anchor=None):d.text(p,str(s),font=font(size,bold),fill=fill,anchor=anchor)
def plate(d,p,s,size=13,color=(248,241,229)):
 x,y=p;tw=d.textlength(s,font=font(size,True));d.rounded_rectangle((x-tw/2-7,y-3,x+tw/2+7,y+size+5),4,fill=(20,15,28,220));text(d,(x,y),s,size,color,True,'mt')
def smooth(t):t=max(0,min(1,t));return t*t*(3-2*t)
class Map:
 def __init__(self):
  self.layout=json.loads((RTS/'map-layout.json').read_text());self.atlas=json.loads((RTS/'assets/degenie-atlas.json').read_text())
  self.sheet=Image.open(RTS/'assets/degenie-atlas.png').convert('RGBA');self.bg=Image.open(RTS/'assets/alashi-terrain.png').convert('RGB').resize((W,H),Image.Resampling.LANCZOS)
 def pt(self,p):return p[0]*W/1672,p[1]*H/941
 def route(self,u,p):
  q=1-p;weights=[q*q*q,3*q*q*p,3*q*p*p,p*p*p]
  return self.pt([sum(v[a]*w for v,w in zip(self.layout['routes'][u],weights)) for a in (0,1)])
 @lru_cache(1800)
 def sprite(self,clip,frame,direction,height):
  tw,th=160,192;x=frame*tw;y=(clip*8+direction)*th
  return self.sheet.crop((x,y,x+tw,y+th)).resize((round(height*tw/th),height),Image.Resampling.LANCZOS)
 def robot(self,out,p,h,clip,t,direction):
  f=int((t%self.atlas['durations'][clip])/self.atlas['durations'][clip]*24);s=self.sprite(clip,min(23,f),direction,h)
  out.paste(s,(round(p[0]-s.width/2),round(p[1]-h*.87)),s)
 def state(self,u,t):
  # Each worker completes a full task. The verifier starts after its three reports.
  z=(t-9-u//4*.55)%8.8;start=(u%4)*.20 if u%4<3 else 4.35
  a=z-start
  if a<0:return 'idle',0,0
  if a<.24:return 'order',0,a
  if a<1.09:return 'outbound',smooth((a-.24)/.85),a-.24
  if a<2.65:return 'work',1,a-1.09
  if a<3.5:return 'return',1-smooth((a-2.65)/.85),a-2.65
  if a<3.85:return 'handoff',0,a-3.5
  return 'done',0,a-3.85
 def render(self,t):
  out=self.bg.copy();d=ImageDraw.Draw(out,'RGBA');units=[]
  for u in range(8):
   phase,p,age=self.state(u,t);x,y=self.route(u,p);color=C[u//4];units.append((y,u,phase,p,age,x))
   if phase not in ('idle','done'):
    points=[self.route(u,j/30) for j in range(31)]
    for j in range(0,30,3):d.line(points[j:j+2],fill=(*color,90),width=2)
   if phase=='order':
    cx,cy=self.pt(self.layout['chiefs'][u//4]);d.line((cx,cy-55,x,y-25),fill=(*color,200),width=2)
   if phase=='work':d.arc((x-22,y-9,x+22,y+9),-90,-90+360*age/1.56,fill=(*color,255),width=3)
  entities=[(u[0],False,u) for u in units]+[(self.pt(p)[1],True,i) for i,p in enumerate(self.layout['chiefs'])]
  for _,chief,data in sorted(entities,key=lambda v:v[0]):
   if chief:
    side=data;x,y=self.pt(self.layout['chiefs'][side]);z=(t-9-side*.55)%8.8;clip=2 if z>8.25 else 0
    d.ellipse((x-52,y-22,x+52,y+22),outline=(*C[side],200),width=2);self.robot(out,(x,y),153,clip,z-8.25 if clip else t,7 if side else 1);d=ImageDraw.Draw(out,'RGBA')
    plate(d,(x,y+31),'CHIEF '+'AB'[side],16,C[side]);continue
   y,u,phase,p,age,x=data;color=C[u//4];d.ellipse((x-18,y-7,x+18,y+8),outline=(*color,210),width=2)
   a=self.route(u,max(0,p-.01));b=self.route(u,min(1,p+.01));angle=math.atan2(b[0]-a[0],b[1]-a[1])+(math.pi if phase=='return' else 0)
   direction=round(angle/(math.pi/4))%8 if phase in ('outbound','return') else 7 if u//4 else 1
   clip=1 if phase=='work' else 2 if phase=='done' and age<1.2 else 0
   self.robot(out,(x,y),83,clip,age if clip else t+u*.37,direction);d=ImageDraw.Draw(out,'RGBA')
   if phase in ('return','handoff'):
    d.rounded_rectangle((x+18,y-64,x+31,y-46),2,fill=(*color,255));d.line((x+21,y-59,x+28,y-59),fill=(25,18,35),width=2)
   if phase=='handoff':
    cx,cy=self.pt(self.layout['chiefs'][u//4]);q=age/.35;dx=x+25+(cx-x-25)*q;dy=y-55+(cy-55-y+55)*q
    d.ellipse((dx-3,dy-3,dx+3,dy+3),fill=(*color,255))
   plate(d,(x,y+13),'AB'[u//4]+str(u%4+1)+(' OK' if phase=='done' and age<1.2 else ''),11,color)
  labels=['MARKET','PARLIAMENT','ANALYTICS','ANALYTICS','VERIFY A','VERIFY B']
  for (_,x,y),label in zip(self.layout['labels'],labels):plate(d,self.pt((x,y)),label,11)
  d.rounded_rectangle((18,16,262,77),8,fill=(19,15,28,238));text(d,(33,23),'ALASHI',29,bold=True);text(d,(34,57),'Learn to lead your agents',13)
  d.rounded_rectangle((861,16,1262,66),8,fill=(19,15,28,238));text(d,(881,25),'PROTOTYPE REPLAY',14,(255,211,141),True);text(d,(881,46),'Scripted activity · no live LLM calls',12)
  # Bottom band is reserved for the exact English speech captions.
  d.rectangle((0,665,W,H),fill=(14,11,22,243))
  return out
if __name__=='__main__':
 import argparse
 p=argparse.ArgumentParser();p.add_argument('--duration',type=float,default=120);p.add_argument('--out',default='/private/tmp/alashi-presenter/map.mp4');p.add_argument('--still',type=float);a=p.parse_args();m=Map()
 if a.still is not None:m.render(a.still).save(a.out);raise SystemExit
 proc=subprocess.Popen([FF,'-y','-loglevel','error','-f','rawvideo','-vcodec','rawvideo','-pix_fmt','rgb24','-s','1280x720','-r','30','-i','-','-an','-c:v','libx264','-preset','veryfast','-crf','21','-pix_fmt','yuv420p',a.out],stdin=subprocess.PIPE)
 for n in range(round(a.duration*FPS)):
  proc.stdin.write(m.render(n/FPS).tobytes())
  if n%300==0:print(f'Rendered {n/FPS:.0f}/{a.duration}s',flush=True)
 proc.stdin.close();raise SystemExit(proc.wait())
