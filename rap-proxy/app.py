from http.server import BaseHTTPRequestHandler, HTTPServer
import os, time, threading
from collections import defaultdict

NODE = os.getenv("NODE_NAME", "unknown")
POD  = os.getenv("POD_NAME", "unknown")
PORT = int(os.getenv("PORT", "8080"))

# counters (naik terus; aggregator hitung delta → RPS)
REQ_TOTAL = 0
REQ_BY_CLIENT = defaultdict(int)
LOCK = threading.Lock()

class H(BaseHTTPRequestHandler):
    def log_message(self, *args):  # quiet
        return

    def _inc(self):
        global REQ_TOTAL
        ip = self.client_address[0]
        with LOCK:
            REQ_TOTAL += 1
            REQ_BY_CLIENT[ip] += 1

    def do_GET(self):
        if self.path.startswith("/health"):
            self.send_response(200); self.end_headers(); self.wfile.write(b"ok"); return

        if self.path.startswith("/metrics"):
            # Prometheus exposition (sederhana)
            with LOCK:
                total = REQ_TOTAL
            body = []
            body.append('# HELP rap_requests_total Total requests served')
            body.append('# TYPE rap_requests_total counter')
            body.append(f'rap_requests_total{{node="{NODE}",pod="{POD}"}} {total}')
            # (opsional) metrik by client
            with LOCK:
                for ip, cnt in list(REQ_BY_CLIENT.items()):
                    body.append(f'rap_requests_by_client_total{{node="{NODE}",pod="{POD}",client="{ip}"}} {cnt}')
            data = ("\n".join(body) + "\n").encode()
            self.send_response(200)
            self.send_header("Content-Type","text/plain; version=0.0.4")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            return

        # default: echo JSON (untuk test TAR / siapa yang melayani)
        self._inc()
        payload = (f'{{"hello":"rap-proxy","node":"{NODE}","pod":"{POD}"}}').encode()
        self.send_response(200)
        self.send_header("Content-Type","application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

if __name__ == "__main__":
    HTTPServer(("0.0.0.0", PORT), H).serve_forever()
