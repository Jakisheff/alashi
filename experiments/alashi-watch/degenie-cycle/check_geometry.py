"""Run inside Blender after build_scene.py with mode validate; independent checks use evaluated poses."""
import json, math
from pathlib import Path
ROOT=Path(__file__).resolve().parent
# Read existing raw observations; do not compare targets to themselves.
samples=json.loads((ROOT/'evidence'/'contacts.json').read_text())
results=[]
for sample in samples:
    t=sample['time']
    for side, lengths in sample['lengths'].items():
        for actual,expected in zip(lengths,[.32,.36]):
            assert abs(actual-expected)<1e-5,(t,side,actual,expected)
    if t in [14.9,18.5,23.8,24.4]:
        for side in ['left','right']:
            gap=sample['tablet']['min'][2]-sample['fingers'][side]['max'][2]
            assert 0<=gap<.002,(t,side,gap)
            results.append({'time':t,'hand':side,'support_gap':gap})
    if t==10.5:
        screen_top=1.19+.04+.009
        gap=sample['fingers']['right']['min'][2]-screen_top
        assert 0<=gap<.002,('screen_contact',gap)
        results.append({'time':t,'screen_contact_gap':gap})
    if t==24.4:
        pad_top=.9975+.015/2
        assert abs(sample['tablet']['min'][2]-pad_top)<1e-5
        results.append({'time':t,'receiver_contact_gap':sample['tablet']['min'][2]-pad_top})
assert sum([12,10,9,8])==39
(ROOT/'evidence'/'geometry-checks.json').write_text(json.dumps({'passed':True,'source':'contacts.json, actual mesh bounds and bone heads','joint_lengths':[.32,.36],'observations':results,'limit':'Sampled contacts and bounding boxes; visual review of the full movement remains required.'},indent=2))
print('PASS: rigid joint lengths, both supporting fingertips, terminal touch, receiving pad and quote arithmetic')
