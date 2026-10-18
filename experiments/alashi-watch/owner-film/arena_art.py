"""Code-native 2.5D arena, following the designer's PDF text requirements."""
from PIL import Image, ImageDraw

STATIONS=[
    ('CHIEF A',(240,625),'#4fdbe5','chief'),
    ('MARKET',(850,210),'#70dd89','market'),
    ('RULES',(410,300),'#55b8ff','rules'),
    ('HORIZON',(1230,660),'#ffca5c','horizon'),
    ('VERIFY',(800,730),'#b995ff','verify'),
    ('CHIEF B',(1440,220),'#f49abf','chief'),
]
LAYOUT={
    'width':1672,'height':941,'chiefs':[[240,625],[1440,220]],
    'routes':[
        [[300,710],[550,640],[660,420],[740,280]],
        [[365,675],[270,530],[270,420],[330,370]],
        [[410,745],[700,600],[950,650],[1140,735]],
        [[410,745],[500,830],[620,830],[720,800]],
        [[1300,315],[1150,310],[1040,310],[950,280]],
        [[1390,385],[1050,450],[700,480],[500,370]],
        [[1300,430],[1320,510],[1340,640],[1320,735]],
        [[1300,430],[1150,500],[950,620],[900,800]],
    ]}


def rgb(c):return tuple(int(c[i:i+2],16) for i in (1,3,5))


def arena(width,height):
    out=Image.new('RGB',(1672,941),'#0b1728');d=ImageDraw.Draw(out)
    # One shared arena; dotted free space is not a mandatory movement track.
    d.polygon([(55,350),(840,35),(1618,345),(1590,750),(840,922),(74,745)],fill='#112338')
    d.line([(55,350),(840,35),(1618,345),(1590,750),(840,922),(74,745),(55,350)],fill='#28455d',width=2)
    for y in range(170,900,46):
        for x in range(100,1600,46):
            if abs(x-836)/800+abs(y-500)/480<1.35:d.ellipse((x,y,x+2,y+2),fill='#254058')
    for _,(x,y),color,kind in STATIONS:
        c=rgb(color)
        # Clear platform top, extrusion and a single bright semantic symbol.
        d.ellipse((x-147,y-37,x+147,y+113),fill='#08111e')
        points=[(x-142,y-17),(x-92,y-59),(x+92,y-59),(x+142,y-17),(x+105,y+61),(x-105,y+61)]
        d.polygon([(a,b+22) for a,b in points],fill='#22364d')
        d.polygon(points,fill=tuple(int(36+v*.20) for v in c))
        d.line(points+[points[0]],fill=color,width=4)
        d.line([(x-104,y+79),(x+104,y+79)],fill=color,width=4)
        cy=y-24
        if kind=='market':
            d.rounded_rectangle((x-46,cy-39,x+46,cy+28),radius=6,fill='#eef9f7')
            d.rectangle((x-31,cy-2,x-7,cy+28),fill='#397b70')
            d.rectangle((x+6,cy-4,x+32,cy+12),fill='#6eb9c0')
            d.polygon([(x-56,cy-30),(x-42,cy-64),(x+42,cy-64),(x+56,cy-30)],fill=color)
            for offset in [-33,-6,21]:d.polygon([(x+offset,cy-64),(x+offset+13,cy-64),(x+offset+18,cy-30),(x+offset-4,cy-30)],fill='#e9fff0')
            d.line((x-57,cy-28,x+57,cy-28),fill='#e9fff0',width=5)
        elif kind=='rules':
            d.rounded_rectangle((x-43,cy-67,x+43,cy+31),radius=8,fill='#edf6ff')
            for row in range(3):
                yy=cy-46+row*26
                d.line([(x-28,yy+5),(x-22,yy+11),(x-11,yy-3)],fill='#197ca9',width=5)
                d.rounded_rectangle((x,yy,x+28,yy+6),radius=2,fill='#668aa8')
        elif kind=='horizon':
            for i,h in enumerate([30,55,86]):
                xx=x-55+i*38;d.rounded_rectangle((xx,cy+30-h,xx+25,cy+30),radius=3,fill=color if i!=1 else '#f5e8ba')
            d.line([(x-62,cy-34),(x-22,cy-65),(x+18,cy-70),(x+61,cy-93)],fill='#fff1bd',width=6)
            d.polygon([(x+61,cy-93),(x+39,cy-90),(x+55,cy-71)],fill='#fff1bd')
        elif kind=='verify':
            d.polygon([(x,cy-76),(x+53,cy-56),(x+44,cy+1),(x,cy+37),(x-44,cy+1),(x-53,cy-56)],fill='#d5c6ff')
            d.polygon([(x,cy-64),(x+39,cy-49),(x+32,cy-6),(x,cy+20),(x-32,cy-6),(x-39,cy-49)],fill='#795bc1')
            d.line([(x-24,cy-17),(x-6,cy+1),(x+28,cy-36)],fill='#f4f0ff',width=9)
        else:
            d.ellipse((x-58,cy-62,x+58,cy+29),fill='#183849',outline=color,width=3)
            for yy in [cy-27,cy-2]:d.line([(x-34,yy),(x,yy-20),(x+34,yy)],fill='#efffff',width=8)
            d.line((x,cy-80,x,cy-65),fill=color,width=4)
            d.ellipse((x-5,cy-90,x+5,cy-80),fill=color)
    return out.resize((width,height),Image.Resampling.LANCZOS)
