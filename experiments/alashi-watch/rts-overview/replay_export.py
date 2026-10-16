"""Bounded loopback-only frame export; game clocks never wait for encoding."""
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import time
import uuid


class ReplayExporter:
    def __init__(self, root):
        self.root = Path(root)
        self.ffmpeg = shutil.which(os.environ.get('ALASHI_FFMPEG', 'ffmpeg'))
        self.lock = threading.Lock()
        self.job = None

    def start(self, source):
        if not self.ffmpeg:
            raise ValueError('ffmpeg_required')
        if not all(source['accepted']):
            raise ValueError('cycle_not_complete')
        duration = max(e['t'] for e in source['events'] if e['kind'] == 'accepted') + 1.5
        count = math.ceil(duration * 30)
        if not 1 <= count <= 900:
            raise ValueError('cycle_too_long')
        with self.lock:
            if self.job and not self.job.get('encoding') and time.monotonic()-self.job['created']>180:
                self.job['temp'].cleanup()
                self.job=None
            if self.job:
                raise ValueError('export_in_progress')
            self.job = dict(id=uuid.uuid4().hex, temp=tempfile.TemporaryDirectory(prefix='alashi-replay-'),
                            frames=count, next=0, bytes=0, source=source, created=time.monotonic())
            return dict(job=self.job['id'], frames=count, fps=30, source=source)

    def frame(self, job_id, index, data):
        with self.lock:
            job = self.job
            if not job or job['id'] != job_id:
                raise ValueError('unknown_export')
            if index != job['next'] or index >= job['frames']:
                raise ValueError('frame_out_of_order')
            if not 4 <= len(data) <= 512000 or not data.startswith(b'\xff\xd8') or not data.endswith(b'\xff\xd9'):
                raise ValueError('invalid_jpeg')
            if job['bytes'] + len(data) > 512000000:
                raise ValueError('export_too_large')
            (Path(job['temp'].name) / f'{index:05d}.jpg').write_bytes(data)
            job['next'] += 1
            job['bytes'] += len(data)
        return dict(ok=True)

    def finish(self, job_id):
        with self.lock:
            job = self.job
            if not job or job['id'] != job_id:
                raise ValueError('unknown_export')
            if job['next'] != job['frames'] or job.get('encoding'):
                raise ValueError('frames_incomplete')
            job['encoding'] = True
        output_dir = self.root / 'exports'
        output_dir.mkdir(exist_ok=True)
        output = output_dir / (job['id'] + '.mp4')
        try:
            subprocess.run([self.ffmpeg, '-hide_banner', '-loglevel', 'error', '-y', '-framerate', '30',
                            '-i', str(Path(job['temp'].name) / '%05d.jpg'), '-frames:v', str(job['frames']),
                            '-c:v', 'libx264', '-preset', 'veryfast', '-crf', '18', '-pix_fmt', 'yuv420p',
                            '-movflags', '+faststart', str(output)], check=True, capture_output=True, timeout=90)
            metadata = dict(source_snapshot=job['source'], frames=job['frames'], fps=30,
                            duration=job['frames']/30, bytes=output.stat().st_size, raw_bytes=job['bytes'],
                            method='event-driven replay; explicit frame timestamps; FFmpeg')
            output.with_suffix('.json').write_text(json.dumps(metadata, ensure_ascii=False, indent=2))
            return dict(url='/exports/' + output.name, **metadata)
        except subprocess.SubprocessError as error:
            output.unlink(missing_ok=True)
            raise ValueError('ffmpeg_export_failed') from error
        finally:
            job['temp'].cleanup()
            with self.lock:
                self.job = None

    def cancel(self, job_id):
        with self.lock:
            if self.job and self.job['id'] == job_id and not self.job.get('encoding'):
                self.job['temp'].cleanup()
                self.job = None
        return dict(ok=True)
