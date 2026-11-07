"""Resume after recorded storage interruption, without regenerating submitted requests."""
import inspect,json
import experiment as e
rows=[json.loads(line) for line in (e.ROOT/'responses.jsonl').read_text().splitlines()]
schedule=json.loads((e.ROOT/'schedule.json').read_text())
assert len(rows)==126 and [r['id'] for r in rows]==[j['id'] for j in schedule[:126]]
source=inspect.getsource(e.run)
changes={
 "assert not (ROOT/'responses.jsonl').exists(),'Refusing to overwrite or silently resume a run'":"assert (ROOT/'resume-amendment.json').exists(), 'Missing explicit amendment'",
 "jobs=json.loads((ROOT/'schedule.json').read_text())":"jobs=json.loads((ROOT/'schedule.json').read_text())[126:]",
 "(ROOT/'responses.jsonl').open('x')":"(ROOT/'responses.jsonl').open('a')",
 "consecutive=0;done=0":"consecutive=0;done=126",
 "{done}/{len(jobs)}":"{done}/{protocol['planned_requests']}"
}
for before,after in changes.items():
 assert source.count(before)==1,before
 source=source.replace(before,after)
(e.ROOT/'resume-run-function.py').write_text(source)
exec(compile(source,str(e.ROOT/'resume-run-function.py'),'exec'),e.__dict__)
e.run()
