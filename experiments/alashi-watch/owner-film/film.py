#!/usr/bin/env python3
"""Deterministic, timestamp-driven Alashi film compositor. No live FPS claim."""
import argparse
from functools import lru_cache
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
from PIL import Image, ImageDraw, ImageFont
from arena_art import arena, LAYOUT, STATIONS

HERE=Path(__file__).resolve().parent
RTS=HERE.parent/'rts-overview'
W,H,FPS,DURATION=1920,1080,30,120
MAP=(40,160,1420,799)
INK='#edf1fc'; MUTED='#9ca9c3'; GOLD='#e7c78e'; PURPLE='#bca0ff'; CYAN='#76e0dc'
TEAMS=[PURPLE,CYAN]
AGENT_COLORS=['#70e6ef','#bfa4ff','#ffce61','#78e69b','#ff9cc6','#68b9ff']
FONT_DIR=Path('/System/Library/Fonts/Supplemental')


def clamp(x): return max(0,min(1,x))
def ease(x):
    x=clamp(x)
    return x*x*(3-2*x)
def mix(a,b,p): return a+(b-a)*p
def rgb(color): return tuple(int(color[i:i+2],16) for i in (1,3,5))


@lru_cache(maxsize=60)
def font(size,bold=False):
    configured=os.environ.get('ALASHI_FONT_BOLD' if bold else 'ALASHI_FONT')
    choices=[configured, str(FONT_DIR/('Arial Bold.ttf' if bold else 'Arial.ttf')),
             '/usr/share/fonts/truetype/dejavu/DejaVuSans'+('-Bold' if bold else '')+'.ttf']
    for p in choices:
        if p and Path(p).exists(): return ImageFont.truetype(p,size)
    raise RuntimeError('Set ALASHI_FONT and ALASHI_FONT_BOLD to TrueType files')


def text(draw,xy,value,size=24,color=INK,bold=False,anchor=None):
    draw.text(xy,str(value),font=font(size,bold),fill=color,anchor=anchor)


def pill(draw,xy,label,color=CYAN,size=16,fill='#162333'):
    x,y=xy; fw=draw.textlength(label,font=font(size,True))
    draw.rounded_rectangle((x,y,x+fw+24,y+size+16),radius=8,fill=fill,outline=color,width=1)
    text(draw,(x+12,y+6),label,size,color,True)
    return fw+24


def wrap(draw,label,size,maxwidth):
    lines=[]; line=''
    for word in label.split():
        candidate=(line+' '+word).strip()
        if line and draw.textlength(candidate,font=font(size))>maxwidth:
            lines.append(line);line=word
        else:line=candidate
    if line:lines.append(line)
    return lines


CHAPTERS=[
 (0, '01 / YOUR TEAM', 'Give your agents a goal.', 'Keep the whole operation in view.'),
 (7, '02 / YOUR DECISION', 'You choose how the team works.', 'One goal: maximize cash in the final market.'),
 (15, '03 / THE EXPERIMENT', 'Same task. Same model.', 'Change only what each worker receives.'),
 (23, '04 / PARALLEL WORK', 'Each team works in parallel.', 'Three specialists check the market, legality and timing.'),
 (32, '05 / REPORTS RETURN', 'Results come back to the Chief.', 'A returned report is still waiting for verification.'),
 (36, '06 / VERIFY', 'Check the decision before acting.', 'Every team must pass the same quality checks.'),
 (47, '07 / EXECUTE', 'The verified decision changes the game.', 'Cash changes only after the rules accept the sale.'),
 (57, '08 / REPEAT', 'One success is not enough.', 'Repeat on different inventory, prices and tax.'),
 (66, '09 / SECOND CASE', 'New state. The same workflow.', 'Both teams receive identical starting conditions.'),
 (82, '10 / THIRD CASE', 'Keep the comparison fair.', 'Include the Chief, verifier and every worker.'),
 (99, '11 / MEASURE', 'Count the cost of a correct result.', 'Provider token receipts. Independent rules checks.'),
 (111,'12 / KEEP WHAT WORKS','Keep the tested setup.', 'Your goal. Your agents. A cost you can inspect.'),
]


def chapter_at(t,measured):
    chapter=next(c for c in reversed(CHAPTERS) if t>=c[0])
    replacements={15:('The comparison to run.','Same task and model; change only worker context.'),
                  99:('No measured token saving yet.','Provider receipts are required before showing a percentage.'),
                  111:('Keep control of your agents.','Save the workflow. Validate its cost before reuse.')}
    if not measured and chapter[0] in replacements:
        chapter=(*chapter[:2],*replacements[chapter[0]])
    return chapter


class Film:
    def __init__(self):
        self.layout=LAYOUT
        self.atlas=json.loads((RTS/'assets/degenie-atlas.json').read_text())
        self.sheet=Image.open(RTS/'assets/degenie-atlas.png').convert('RGBA')
        self.data=HERE/'evidence/benchmark'
        self.runs=json.loads((self.data/'runs.json').read_text()) if (self.data/'runs.json').exists() else []
        self.summary=json.loads((self.data/'summary.json').read_text()) if (self.data/'summary.json').exists() else None
        self.measured=bool(self.summary and self.summary['all_quality_checks_passed'])
        self.cases=json.loads((self.data/'cases.json').read_text()) if (self.data/'cases.json').exists() else []
        if not self.measured:
            demo=json.loads((HERE/'evidence/demo.json').read_text())
            self.runs=demo['runs'];self.cases=demo['cases']
        self.background=self.base()
        self.rows=[]
        if (self.data/'responses.jsonl').exists():
            self.rows=[json.loads(x) for x in (self.data/'responses.jsonl').read_text().splitlines()]

    def base(self):
        out=Image.new('RGB',(W,H));d=ImageDraw.Draw(out)
        for y in range(H):
            p=y/H;d.line((0,y,W,y),fill=(int(9+4*p),int(15+4*p),int(29+6*p)))
        # The designer's written prompts take precedence over ornate concept art.
        terrain=arena(*MAP[2:])
        mask=Image.new('L',MAP[2:]);ImageDraw.Draw(mask).rounded_rectangle((0,0,MAP[2]-1,MAP[3]-1),radius=18,fill=255)
        out.paste(terrain,MAP[:2],mask)
        d.rounded_rectangle((MAP[0],MAP[1],MAP[0]+MAP[2],MAP[1]+MAP[3]),radius=18,outline='#445068',width=2)
        d.rounded_rectangle((1488,160,1880,959),radius=18,fill='#101b2d',outline='#2b3b55',width=2)
        text(d,(42,33),'ALASHI',43,INK,True)
        d.line((236,37,236,79),fill='#3c4b63',width=2)
        text(d,(262,38),'THE AGENT COMMAND ARENA',20,MUTED,True)
        text(d,(262,65),'6 DEGENIE  /  ONE SHARED ARENA',18,MUTED)
        d.line((40,108,1880,108),fill='#26364b',width=1)
        return out

    def point(self,p):
        return (MAP[0]+p[0]*MAP[2]/self.layout['width'],MAP[1]+p[1]*MAP[3]/self.layout['height'])

    @lru_cache(maxsize=2400)
    def sprite(self,clip,frame,direction,height):
        tw,th=self.atlas['tileWidth'],self.atlas['tileHeight']
        x=frame*tw;y=(clip*8+direction)*th
        return self.sheet.crop((x,y,x+tw,y+th)).resize((round(height*tw/th),height),Image.Resampling.LANCZOS)

    def robot(self,image,position,height,clip,t,direction=0,once=False):
        duration=self.atlas['durations'][clip]
        phase=clamp(t/duration)*.9999 if once else (t%duration)/duration
        frame=min(23,int(phase*24));sprite=self.sprite(clip,frame,direction,height)
        x,y=position
        image.paste(sprite,(round(x-sprite.width*.5),round(y-height*.87)),sprite)

    def route(self,unit,p):
        points=self.layout['routes'][unit];q=1-p;weights=[q*q*q,3*q*q*p,3*q*p*p,p*p*p]
        return self.point([sum(point[a]*w for point,w in zip(points,weights)) for a in (0,1)])

    def schedule(self,t):
        # Editorial time, deliberately separate from real provider latency.
        if t<60:return 0,23,1.0
        if t<82:return 1,66,.63
        return 2,82,.63

    def unit_state(self,unit,t):
        case,start,speed=self.schedule(t);role=unit%4;side=unit//4
        start+=side*.18
        if role<3:
            start+=role*.24
            marks=[0,.24,1.12,6.4+role*.7,7.35+role*.7,7.7+role*.7]
        else:
            start+=13*speed
            marks=[0,.24,1.12,4.4,5.35,5.7]
        elapsed=(t-start)/speed
        if elapsed<0:return 'idle',0,0,case
        if elapsed<marks[1]:return 'order',0,elapsed,case
        if elapsed<marks[2]:return 'out',ease((elapsed-marks[1])/(marks[2]-marks[1])),elapsed,case
        if elapsed<marks[3]:return 'work',1,elapsed-marks[2],case
        if elapsed<marks[4]:return 'back',1-ease((elapsed-marks[3])/(marks[4]-marks[3])),elapsed-marks[3],case
        if elapsed<marks[5]:return 'deliver',0,elapsed-marks[4],case
        # A market courier carries the verified order after all reports are accepted.
        dispatch=22 if case==0 else 20
        if role==0 and dispatch<=(t-(23 if case==0 else 66 if case==1 else 82))/speed<dispatch+2.8:
            e=(t-(23 if case==0 else 66 if case==1 else 82))/speed-dispatch
            return ('execute',ease(e/.9) if e<.9 else 1 if e<1.7 else 1-ease((e-1.7)/1.1),e,case)
        return 'done',0,elapsed-marks[5],case

    def cycle_phase(self,t):
        case,start,speed=self.schedule(t)
        e=(t-start)/speed
        return case, ('ready' if e<0 else 'workers' if e<10 else 'chief' if e<13 else 'verify' if e<19 else 'accepted' if e<23.1 else 'executed')

    def run(self,case,side):
        mode=['full','scoped'][side]
        return next((r for r in self.runs if r['case_id']==f'market-{case+1}' and r['mode']==mode),None)

    def local_label(self,d,position,label,color=INK,size=15):
        x,y=position;length=d.textlength(label,font=font(size,True))
        d.rounded_rectangle((x-length/2-9,y-4,x+length/2+9,y+size+7),radius=5,fill='#101827',outline='#36425b')
        text(d,(x,y),label,size,color,True,'mt')

    def draw_world(self,image,t):
        d=ImageDraw.Draw(image)
        labels=[(name,(point[0],point[1]+(-145 if name.startswith('CHIEF') else 103))) for name,point,_,_ in STATIONS]
        for label,p in labels:self.local_label(d,self.point(p),label,INK,20)
        # A presentation flag prevents the shared artwork being mistaken for shared test state.
        pill(d,(60,177),'PAIRED REPLAY / EDITED TIMING' if self.measured else 'SCRIPTED WORKFLOW / REAL LOCAL RULES',GOLD,14,'#17202a')
        units=[]
        for actor in [0,1,2,4,5,6]:
            unit=actor
            if actor%4==2 and self.unit_state(actor+1,t)[0]!='idle':unit=actor+1
            phase,p,elapsed,case=self.unit_state(unit,t);side=unit//4;color=AGENT_COLORS[side*3+min(unit%4,2)]
            x,y=self.route(unit,p);units.append((y,unit,phase,p,elapsed,(x,y)))
            if phase not in ('idle','done'):
                path=[self.route(unit,i/48) for i in range(49)]
                # Thin routes are beneath the characters and terminate at the actual workstation.
                d.line(path,fill=tuple(int(v*.75) for v in rgb(color)),width=2)
                move=(t*.63+unit*.11)%1
                if phase in ('back','deliver'):move=1-move
                pulse=self.route(unit,move)
                d.ellipse((pulse[0]-3,pulse[1]-3,pulse[0]+3,pulse[1]+3),fill=color)
            if phase=='order':
                chief=self.point(self.layout['chiefs'][side]);d.line((chief[0],chief[1]-55,x,y-30),fill=color,width=3)
        entities=[(y,'worker',item) for item in units for y in [item[0]]]
        # Chiefs are fixed command platforms. Six Degenie remain the visible actors.
        for _,kind,data in sorted(entities,key=lambda e:e[0]):
            _,unit,phase,p,elapsed,(x,y)=data;side=unit//4;color=AGENT_COLORS[side*3+min(unit%4,2)]
            d.ellipse((x-19,y-5,x+19,y+10),fill='#272d37')
            d.ellipse((x-24,y-9,x+24,y+11),outline=color,width=2)
            moving=phase in ('out','back','execute')
            pa=self.route(unit,max(0,p-.01));pb=self.route(unit,min(1,p+.01))
            angle=math.atan2(pb[0]-pa[0],pb[1]-pa[1])+(math.pi if phase=='back' else 0)
            direction=round(angle/(math.pi/4))%8 if moving or phase=='order' else (7 if side else 1)
            clip=1 if phase=='work' else 2 if phase=='done' and elapsed<self.atlas['durations'][2] else 0
            self.robot(image,(x,y),140,clip,elapsed if clip else t+unit*.31,direction,phase=='done')
            if side*4+min(unit%4,2)==self.selected_actor(t):
                d.polygon([(x-10,y-140),(x+10,y-140),(x,y-124)],fill=color)
                d.ellipse((x-30,y-12,x+30,y+15),outline=color,width=3)
            if phase=='work':
                progress=(elapsed/5)%1
                d.arc((x-29,y-15,x+29,y+15),-90,-90+360*progress,fill=color,width=4)
                for j in range(3):d.line((x+30,y-55-j*9,x+39+j*4,y-55-j*9),fill=color,width=3)
            if phase in ('back','deliver'):
                # A data badge, never claimed to be a physical object in the hand.
                bx,by=x+20,y-110+math.sin(t*6)*2
                d.rounded_rectangle((bx,by,bx+22,by+25),radius=4,fill=color)
                for j in range(3):d.line((bx+5,by+7+j*5,bx+16,by+7+j*5),fill='#162235',width=2)
            self.local_label(d,(x,y+15),('B' if side else 'A')+str(min(unit%4,2)+1)+(' OK' if phase=='done' else ''),color,13)
        case,phase=self.cycle_phase(t)
        if phase=='executed' and self.run(case,0):
            cash=self.run(case,0)['execution']['cash_after']/1000000
            x,y=self.point((850,365))
            self.local_label(d,(x,y),f'SALE ACCEPTED  /  CASH {cash:g}M',CYAN,21)

    def selected_actor(self,t):
        if t<15:return 0
        if t<23:return 4
        if t<27:return 0
        if t<32:return 5
        if t<45:return 2
        return 0 if t<66 else 6 if t<99 else 4

    def draw_panel(self,image,t,world_t):
        d=ImageDraw.Draw(image);case,phase=self.cycle_phase(world_t)
        text(d,(1516,190),'DEGENIE',22,INK,True)
        text(d,(1852,195),'6 / 6',18,CYAN,True,'rt')
        statuses={'idle':'Ready','order':'Task assigned','out':'On the way','work':'Working','back':'Returning report','deliver':'Report delivered','done':'Ready for next task','execute':'Executing sale'}
        for row,actor in enumerate([0,1,2,4,5,6]):
            side=actor//4;unit=actor
            if actor%4==2 and self.unit_state(actor+1,world_t)[0]!='idle':unit=actor+1
            state=self.unit_state(unit,world_t)[0];color=AGENT_COLORS[row];y=235+row*59
            selected=actor==self.selected_actor(world_t)
            d.rounded_rectangle((1506,y,1861,y+53),radius=8,fill='#1d3048' if selected else '#142238',outline=color if selected else '#2a3d57',width=2 if selected else 1)
            d.rounded_rectangle((1518,y+10,1550,y+41),radius=6,fill=color)
            text(d,(1534,y+15),('B' if side else 'A')+str(actor%4+1),15,'#102033',True,'mt')
            role='Verifier' if unit%4==3 else ['Market','Rules','Horizon'][actor%4]
            text(d,(1565,y+6),role,19,INK,True)
            text(d,(1565,y+30),statuses[state],15,color)
            if state in ('out','work','back','execute'):
                d.ellipse((1838,y+20,1846,y+28),fill=color)
        d.line((1516,610,1852,610),fill='#35465e',width=1)
        if t<15:
            text(d,(1516,636),'YOUR OBJECTIVE',16,CYAN,True)
            text(d,(1516,674),'Maximize game cash',25,INK,True)
            text(d,(1516,718),'Every decision must pass',20,MUTED)
            text(d,(1516,745),'the same rules check.',20,MUTED)
            pill(d,(1516,802),'BUDGET TARGET: 10K TOKENS',GOLD,13)
            text(d,(1516,875),'Choose how your team works.',19,INK)
        elif t<23:
            text(d,(1516,636),'OWNER CONTROL',16,CYAN,True)
            text(d,(1516,680),'A / Full context',23,PURPLE,True)
            text(d,(1516,716),'All rules to every worker.',18,MUTED)
            text(d,(1516,764),'B / Task-specific context',23,CYAN,True)
            text(d,(1516,800),'Only the inputs for this job.',18,MUTED)
            if t>=18:pill(d,(1516,865),'TASK ROUTING ENABLED',CYAN,14)
        elif t<99:
            text(d,(1516,636),'EVENT LOG / CASE 0'+str(case+1),16,CYAN,True)
            result=self.run(case,0)
            goods=result['decision']['units']
            revenue=result['execution']['amount']/1000000
            cash=result['execution']['cash_after']/1000000
            logs=[]
            if phase=='ready':logs=['Same initial state for both teams.','New inventory, prices and tax.']
            if phase=='workers':logs=['Chiefs assign independent tasks.','Three workers per team are active.']
            if phase=='chief':logs=[f'Market: {goods} goods / {revenue:g}M.','Sale is legal. Final market.']
            if phase=='verify':logs=[f'Proposal: sell {goods} goods.',f'Check expected cash: {cash:g}M.']
            if phase=='accepted':logs=['Verification completed.','Approved sale is next.']
            if phase=='executed':logs=['Sale accepted by the rules.','Game state updated.']
            for i,line in enumerate(logs):
                d.ellipse((1518,684+i*52,1524,690+i*52),fill=CYAN)
                for j,part in enumerate(wrap(d,line,18,304)):
                    text(d,(1536,676+i*52+j*23),part,18,INK if i==len(logs)-1 else MUTED)
            if phase=='executed' and result:
                text(d,(1516,815),f'100M > {cash:g}M',33,CYAN,True)
                text(d,(1516,864),'GAME CASH / BOTH TEAMS',15,MUTED)
            else:
                label='Results are not cash.' if phase in ('workers','chief','verify') else 'Identical quality gate.'
                text(d,(1516,823),label,22,GOLD,True)
            text(d,(1516,915),'Local rules / edited demonstration',14,MUTED)
        else:
            text(d,(1516,636),'TOKEN RECEIPTS',16,CYAN,True)
            if self.measured:
                totals=self.summary['totals'];maximum=totals['full']['total_tokens']
                for side,mode in enumerate(['full','scoped']):
                    y=675+side*70;value=totals[mode]['total_tokens']
                    text(d,(1516,y),'Full context' if not side else 'Task-specific',18,TEAMS[side],True)
                    text(d,(1850,y),f'{value:,}',23,INK,True,'rt')
                    d.rounded_rectangle((1516,y+33,1850,y+43),radius=5,fill='#26344b')
                    d.rounded_rectangle((1516,y+33,1516+334*min(1,value/maximum),y+43),radius=5,fill=TEAMS[side])
                text(d,(1516,827),f'{self.summary["total_token_reduction_percent"]:g}% fewer tokens',30,CYAN,True)
                text(d,(1516,869),'3 / 3 correct in both groups',18,INK)
                text(d,(1516,900),'All 30 calls counted. Small test.',15,MUTED)
            else:
                for i,line in enumerate(wrap(d,'Token savings require provider measurements.',25,330)):
                    text(d,(1516,684+i*35),line,25,INK,True)
                text(d,(1516,802),'No percentage is claimed.',20,GOLD)
                text(d,(1516,853),'Count workers, Chief and checks.',17,MUTED)
                text(d,(1516,888),'Compare at the same quality.',17,MUTED)

    def frame(self,t):
        t=max(0,min(119.999,t));image=self.background.copy()
        # Opening teaser, followed by a visible chapter change into the owner setup.
        world_t=23+t*1.45 if t<7 else t
        self.draw_world(image,world_t)
        self.draw_panel(image,t,world_t);d=ImageDraw.Draw(image)
        chapter=chapter_at(t,self.measured)
        if t<1.2:
            # A brief start signal, never a random game-outcome wheel.
            cx,cy=760,536
            d.ellipse((cx-46,cy-46,cx+46,cy+46),fill='#14273a',outline=CYAN,width=3)
            d.arc((cx-40,cy-40,cx+40,cy+40),int(t*360),int(t*360)+115,fill=GOLD,width=6)
            text(d,(cx,cy-16),'01' if int(t*8)%2 else '10',29,INK,True,'mt')
            self.local_label(d,(cx,cy+60),'INITIALIZING TEAMS',CYAN,18)
        text(d,(42,123),chapter[1],18,CYAN,True)
        flag='MEASURED LLM REPLAY / EDITED TIMING' if self.measured else 'WORKFLOW CONCEPT / METRICS UNVERIFIED'
        text(d,(1878,58),flag,16,GOLD,True,'rt')
        # Keep the entire arena visible; narration stays below it.
        text(d,(42,979),chapter[2],35,INK,True)
        text(d,(43,1023),chapter[3],23,MUTED)
        text(d,(1878,1014),f'{int(t)//60:02d}:{int(t)%60:02d} / 02:00',20,MUTED,False,'rt')
        d.rectangle((40,1067,1880,1070),fill='#28384c')
        d.rectangle((40,1067,40+1840*t/120,1070),fill=CYAN)
        return image


def ffmpeg_path():
    value=os.environ.get('ALASHI_FFMPEG') or shutil.which('ffmpeg')
    if value:return value
    sys.path.insert(0,'/private/tmp/alashi-film-tools')
    try:
        import imageio_ffmpeg
        return imageio_ffmpeg.get_ffmpeg_exe()
    except ImportError as e:raise RuntimeError('Install FFmpeg or set ALASHI_FFMPEG') from e


def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--frames',action='store_true')
    ap.add_argument('--preview',action='store_true',help='Render the central 20-second scene')
    ap.add_argument('--out',type=Path,default=HERE/'alashi-owner-film.mp4')
    args=ap.parse_args();film=Film();evidence=HERE/'evidence';evidence.mkdir(exist_ok=True)
    if args.frames:
        for t in [3,18,25,30,37,42,49,71,90,105,115]:
            frame=film.frame(t);frame.save(evidence/f'frame-{t:03}.jpg',quality=94)
        film.frame(105).resize((390,219),Image.Resampling.LANCZOS).save(evidence/'phone-390.jpg',quality=94)
        film.frame(25).save(HERE/'poster.jpg',quality=94)
        subtitles=[]
        for i,c in enumerate(CHAPTERS):
            start=c[0];end=CHAPTERS[i+1][0] if i+1<len(CHAPTERS) else DURATION
            c=chapter_at(start,film.measured)
            stamp=lambda value:f'00:{value//60:02d}:{value%60:02d},000'
            subtitles.append(f'{i+1}\n{stamp(start)} --> {stamp(end)}\n{c[2]}\n{c[3]}\n')
        (HERE/'captions.en.srt').write_text('\n'.join(subtitles))
        return
    first,count=(23*FPS,20*FPS) if args.preview else (0,DURATION*FPS)
    output=args.out
    log=(evidence/('preview-encode.log' if args.preview else 'encode.log')).open('w')
    command=[ffmpeg_path(),'-hide_banner','-y','-f','rawvideo','-pix_fmt','rgb24','-s',f'{W}x{H}','-r',str(FPS),'-i','-',
             '-an','-c:v','libx264','-preset','fast','-crf','19','-pix_fmt','yuv420p','-movflags','+faststart','-frames:v',str(count),str(output)]
    start=time.monotonic()
    with subprocess.Popen(command,stdin=subprocess.PIPE,stderr=log) as encoder:
        try:
            for i in range(count):
                encoder.stdin.write(film.frame((first+i)/FPS).tobytes())
                if i%150==0:print(f'FRAME {i}/{count} elapsed {time.monotonic()-start:.1f}s',flush=True)
        finally:encoder.stdin.close()
        if encoder.wait():raise RuntimeError('FFmpeg failed; inspect encode.log')
    log.close()
    metadata={'frames':count,'fps':FPS,'duration_s':count/FPS,'width':W,'height':H,'measured_llm':film.measured,
              'timing':'Edited event presentation, not measured live latency','render_seconds':round(time.monotonic()-start,2),
              'sha256':hashlib.sha256(output.read_bytes()).hexdigest(),'bytes':output.stat().st_size}
    (evidence/('preview.json' if args.preview else 'video.json')).write_text(json.dumps(metadata,indent=2)+'\n')
    print(json.dumps(metadata),flush=True)


if __name__=='__main__':main()
