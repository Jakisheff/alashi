from pathlib import Path
import json,re,base64
root=Path(__file__).resolve().parents[1]
base=(root/'app/alashi-party21-replay.html').read_text().replace('requestAnimationFrame(tick)','')
three=re.findall(r'<script>(.*?)</script>',(root/'app/alashi-city.html').read_text(),re.S)[0]
font=base64.b64encode((root/'assets/fonts/Jura.ttf').read_bytes()).decode()
css='@font-face{font-family:Jura;src:url(data:font/ttf;base64,'+font+');font-weight:300 700}*{box-sizing:border-box}html,body{margin:0;background:#10181c;overflow:hidden}canvas{display:block;width:100vw;height:100vh;object-fit:contain}nav{display:none}'
base=re.sub(r'<style>.*?</style>','<style>'+css+'</style>',base,flags=re.S)
proof=json.loads((root/'docs/ops/STUDIO_LOCAL_PROOF_20260906.json').read_text());proof['transactions']=[t for t in proof['transactions'] if t['instruction']=='join']
victory=(root/'tools/parallel_victory.js').read_text().replace('__EARTH_TEXTURE__','data:image/jpeg;base64,'+base64.b64encode((root/'assets/textures/earth_atmos_2048.jpg').read_bytes()).decode())
extra=(root/'tools/studio_scene.js').read_text()+'\n'+victory+'\n'+(root/'tools/parallel_overlay.js').read_text().replace('__PARALLEL_PROOF__',json.dumps(proof,ensure_ascii=False).replace('<','\\u003c'))
base=base.replace('</script>',extra+'</script>').replace('<script>','<script>'+three+'</script><script>',1)
(root/'app/alashi-parallel-demo.html').write_text(base)
