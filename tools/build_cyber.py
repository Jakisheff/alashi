from pathlib import Path
import re,json,base64
root=Path(__file__).resolve().parents[1]
base=(root/'app/alashi-party21-replay.html').read_text()
base=base.replace('requestAnimationFrame(tick)','')
font=base64.b64encode((root/'assets/fonts/Jura.ttf').read_bytes()).decode()
css='@font-face{font-family:Jura;src:url(data:font/ttf;base64,'+font+');font-weight:300 700}*{box-sizing:border-box}html,body{margin:0;background:#03050c;overflow:hidden}canvas{display:block;width:100vw;height:100vh;object-fit:contain}nav{display:none}'
base=re.sub(r'<style>.*?</style>','<style>'+css+'</style>',base,flags=re.S)
changes={'#76e1b5':'#35dcff','#f8be67':'#ff42c8','#8fc9ff':'#35dcff','#dcafff':'#ff42c8','#ffe584':'#35dcff','#ffaba3':'#ff42c8','#2d4738':'#08101c','#14271f':'#080e1b','#1d3329':'#0a1120','#3a5541':'#10273a','#172c22':'#08101c','#101f18':'#081020','#9dad8c44':'#35dcff22','#9db39d12':'#35dcff12','#91e5b4':'#b6ff42','#f9fff4':'#35dcff','#b9c9b4':'#548197','#a6b9aa':'#9aabc7','#c3d0c3':'#9aabc7','#eef2e8':'#e6f4ff'}
for old,new in changes.items():base=base.replace(old,new)
proof=json.loads((root/'docs/ops/CYBER_LOCAL_PROOF_20260906.json').read_text());proof['transactions']=[x for x in proof['transactions'] if x['instruction'] in ['join','settle']]
extra=(root/'tools/cyber_overlay.js').read_text().replace('__PROOF__',json.dumps(proof,ensure_ascii=False).replace('<','\\u003c'))
base=base.replace("text(money(a.cash_after)+' песо',1288,752,48,'#fff',750)","text(money(a.cash_after)+' песо',1288,752,48,'#b6ff42',750)");base=base.replace('</script>',extra+'</script>');(root/'app/alashi-cyber-demo.html').write_text(base)
