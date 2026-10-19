def run():
    protocol=json.loads((ROOT/'protocol.json').read_text())
    for name,h in protocol['hashes'].items():assert digest((ROOT/name).read_bytes())==h, 'Frozen input changed: '+name
    assert (ROOT/'resume-amendment.json').exists(), 'Missing explicit amendment'
    spec=importlib.util.spec_from_file_location('alashi_debrief',REPO/'tools/debrief.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
    key=m.llm_key();assert key,'Missing alashi API credential'
    prompts=json.loads((ROOT/'prompts.json').read_text());rows={r['id']:r for r in json.loads((ROOT/'fixtures.json').read_text())}
    jobs=json.loads((ROOT/'schedule.json').read_text())[126:]
    def request(job):
      p=prompts[job['key']];started=time.monotonic();result={**job,'scenario':p['scenario'],'condition':p['condition'],'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
      body={**protocol['settings'],'messages':p['messages']}
      req=urllib.request.Request('https://api.z.ai/api/paas/v4/chat/completions',data=json.dumps(body,ensure_ascii=False).encode(),headers={'Authorization':'Bearer '+key,'Content-Type':'application/json'},method='POST')
      try:
        with urllib.request.urlopen(req,timeout=50) as r:raw=json.load(r)
        result['raw']=raw
        try:
          result['parsed']=parse(raw['choices'][0]['message']['content'],[o['id'] for o in rows[p['scenario']]['options']]);result['status']='ok'
        except Exception as e:result.update(status='invalid',error=f'{type(e).__name__}: {e}')
      except Exception as e:result.update(status='request_error',error=f'{type(e).__name__}: {e}')
      result['elapsed_s']=round(time.monotonic()-started,3)
      return result
    # Bounded windows preserve randomized submission order and cap in-flight requests at two.
    with (ROOT/'responses.jsonl').open('a') as output, concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
      consecutive=0;done=126
      for offset in range(0,len(jobs),2):
        futures=[pool.submit(request,j) for j in jobs[offset:offset+2]]
        for f in futures:
          result=f.result();output.write(json.dumps(result,ensure_ascii=False)+'\n');output.flush();os.fsync(output.fileno());done+=1
          consecutive=consecutive+1 if result['status']=='request_error' else 0
          print(f'{done}/{protocol['planned_requests']} {result["id"]} {result["status"]} {result.get("parsed",{}).get("action","")} {result["elapsed_s"]}s',flush=True)
        if consecutive>=5:raise SystemExit('Stopped: consecutive API errors; partial data preserved')
