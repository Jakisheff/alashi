#!/usr/bin/env python3
"""Pure-data execution for model-written analysis and context helpers."""
import ast,contextlib,io,json,math,resource,sys
FORBIDDEN={'open','eval','exec','compile','globals','locals','vars','dir','input','help','breakpoint','getattr','setattr','delattr','memoryview','super'}
METHODS={'get','items','keys','values','append','extend','sort','copy','count','index','setdefault','add','discard','remove','pop','clear','update','lower','upper','strip','split','join','replace','startswith','endswith'}
BUILTINS={'abs':abs,'all':all,'any':any,'bool':bool,'dict':dict,'enumerate':enumerate,'float':float,'int':int,'isinstance':isinstance,'len':len,'list':list,'max':max,'min':min,'range':range,'reversed':reversed,'round':round,'set':set,'sorted':sorted,'str':str,'sum':sum,'tuple':tuple,'zip':zip,'Exception':Exception,'ValueError':ValueError,'ZeroDivisionError':ZeroDivisionError,'KeyError':KeyError,'TypeError':TypeError}
def validate(code):
 if len(code)>16000:raise ValueError('Code exceeds 16000 characters')
 tree=ast.parse(code)
 for node in ast.walk(tree):
  if isinstance(node,(ast.Import,ast.ImportFrom,ast.Global,ast.Nonlocal,ast.ClassDef,ast.AsyncFunctionDef,ast.Await,ast.Yield,ast.YieldFrom,ast.With,ast.AsyncWith)):raise ValueError('Unsupported syntax: '+type(node).__name__)
  if isinstance(node,ast.Name) and (node.id.startswith('__') or node.id in FORBIDDEN):raise ValueError('Forbidden name: '+node.id)
  if isinstance(node,ast.Attribute) and node.attr not in METHODS:raise ValueError('Unsupported attribute: '+node.attr)
 return tree

def main():
 resource.setrlimit(resource.RLIMIT_CPU,(2,3))
 try:resource.setrlimit(resource.RLIMIT_AS,(384*1024*1024,384*1024*1024))
 except (OSError,ValueError):pass
 payload=json.load(sys.stdin);output=io.StringIO()
 def bounded_print(*values,**kwargs):
  if kwargs.keys()-{'sep','end'}:raise ValueError('print only supports sep/end')
  print(*values,file=output,**kwargs)
  if output.tell()>32000:raise ValueError('Output exceeds 32000 characters; aggregate it')
 scope={'__builtins__':{**BUILTINS,'print':bounded_print},'sqrt':math.sqrt,'log':math.log,'ceil':math.ceil,'floor':math.floor,'dumps':json.dumps}
 if payload['mode']=='analysis':scope['games']=payload['games']
 else:scope.update(state=payload['state'],history=payload['history'])
 try:
  exec(compile(validate(payload['code']),'<model-data-code>','exec'),scope)
  if payload['mode']=='helper':
   value=scope['augment'](scope['state'],scope['history']);assert isinstance(value,dict),'augment must return a dict';encoded=json.dumps(value,ensure_ascii=False,allow_nan=False);assert len(encoded)<=8000,'augment output exceeds 8000 characters'
   result={'ok':True,'value':value}
  else:result={'ok':True,'output':output.getvalue()}
 except Exception as error:result={'ok':False,'error':type(error).__name__+': '+str(error)[:1000]}
 print(json.dumps(result,ensure_ascii=False,allow_nan=False))
if __name__=='__main__':main()
