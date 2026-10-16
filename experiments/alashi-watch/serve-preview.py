"""Loopback-only preview server with byte ranges for browser video seeking."""
import http.server, os, re
from pathlib import Path
ROOT=Path(__file__).resolve().parent
class RangeHandler(http.server.SimpleHTTPRequestHandler):
    protocol_version='HTTP/1.1'
    def __init__(self,*args,**kwargs):super().__init__(*args,directory=str(ROOT),**kwargs)
    def end_headers(self):
        self.send_header('Accept-Ranges','bytes')
        super().end_headers()
    def send_head(self):
        self.remaining=None
        header=self.headers.get('Range')
        path=Path(self.translate_path(self.path))
        if not header or not path.is_file():return super().send_head()
        size=path.stat().st_size
        match=re.fullmatch(r'bytes=(\d*)-(\d*)',header.strip())
        if not match or not any(match.groups()):return self.invalid_range(size)
        first,last=match.groups()
        if first:
            start=int(first);end=min(int(last) if last else size-1,size-1)
        else:start=max(0,size-int(last));end=size-1
        if start>end or start>=size:return self.invalid_range(size)
        try:stream=path.open('rb')
        except OSError:self.send_error(404);return None
        stream.seek(start);self.remaining=end-start+1
        self.send_response(206)
        self.send_header('Content-Type',self.guess_type(str(path)))
        self.send_header('Content-Range',f'bytes {start}-{end}/{size}')
        self.send_header('Content-Length',str(self.remaining))
        self.send_header('Last-Modified',self.date_time_string(path.stat().st_mtime))
        self.end_headers();return stream
    def invalid_range(self,size):
        self.send_response(416);self.send_header('Content-Range',f'bytes */{size}')
        self.send_header('Content-Length','0');self.end_headers();return None
    def copyfile(self,source,output):
        try:
            if self.remaining is None:return super().copyfile(source,output)
            while self.remaining>0:
                data=source.read(min(65536,self.remaining))
                if not data:break
                output.write(data);self.remaining-=len(data)
        except (BrokenPipeError,ConnectionResetError):pass
if __name__=='__main__':
    server=http.server.ThreadingHTTPServer(('127.0.0.1',8766),RangeHandler)
    print('Alashi preview: http://127.0.0.1:8766/',flush=True)
    server.serve_forever()
