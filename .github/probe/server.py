import http.server,sys,os
LOG=open(os.environ.get("PROBE_LOG","probe-log.jsonl"),"a",buffering=1)
class H(http.server.SimpleHTTPRequestHandler):
    def do_POST(self):
        n=int(self.headers.get("content-length",0));LOG.write(self.rfile.read(n).decode()+"\n")
        self.send_response(204);self.end_headers()
    def log_message(self,*a):pass
http.server.ThreadingHTTPServer(("127.0.0.1",8765),H).serve_forever()
