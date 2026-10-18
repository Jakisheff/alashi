"""Export boundaries without invoking a codec or changing game state."""
from pathlib import Path
import tempfile
from replay_export import ReplayExporter

with tempfile.TemporaryDirectory() as directory:
    exporter=ReplayExporter(Path(directory));exporter.ffmpeg='ffmpeg'
    source=dict(accepted=[True,True],events=[dict(kind='accepted',t=9.5)])
    def rejected(call,reason):
        try:call()
        except ValueError as error:assert str(error)==reason,(str(error),reason)
        else:raise AssertionError(reason)
    rejected(lambda:exporter.start(dict(accepted=[False,False])),'cycle_not_complete')
    job=exporter.start(source);assert job['fps']==30 and job['frames']==330
    rejected(lambda:exporter.start(source),'export_in_progress')
    rejected(lambda:exporter.frame('../escape',0,b''),'unknown_export')
    rejected(lambda:exporter.frame(job['job'],1,b'\xff\xd8\xff\xd9'),'frame_out_of_order')
    rejected(lambda:exporter.frame(job['job'],0,b'not jpeg'),'invalid_jpeg')
    rejected(lambda:exporter.finish(job['job']),'frames_incomplete')
    location=Path(exporter.job['temp'].name)
    exporter.frame(job['job'],0,b'\xff\xd8test\xff\xd9')
    assert (location/'00000.jpg').read_bytes()==b'\xff\xd8test\xff\xd9'
    rejected(lambda:exporter.frame(job['job'],0,b'\xff\xd8test\xff\xd9'),'frame_out_of_order')
    exporter.job['created']-=181
    next_job=exporter.start(source);assert not location.exists()
    exporter.cancel(next_job['job']);assert exporter.job is None
print('PASS: incomplete cycle, foreign job, frame order, JPEG bounds, duplicate frame, stale export cleanup, cancellation')
