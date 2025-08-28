import os, threading, time, json, requests
from http.server import BaseHTTPRequestHandler, HTTPServer
from kubernetes import client, config
from urllib.parse import urlparse, parse_qs
from datetime import datetime

PORT = int(os.getenv("PORT","8080"))
SCRAPER_PERIOD = int(os.getenv("SCRAPER_PERIOD","20"))
NAMESPACE = os.getenv("NAMESPACE","default")

# scorer
SCORER_LABEL = os.getenv("SCORER_LABEL","app=node-scorer")
SCORER_PORT  = int(os.getenv("SCORER_PORT","8081"))

# rap-proxy (traffic)
RAP_LABEL = os.getenv("RAP_LABEL","app=rap-proxy")
RAP_PORT  = int(os.getenv("RAP_PORT","8080"))

# caches
scores_cache = {"items": [], "ts": None}          # [{"host","score"}]
traffic_cache = {"items": [], "ts": None}         # [{"host","rps","total"}]
_prev_totals = {}                                 # (node -> total) for delta
_prev_ts = None

def k8s():
    try: config.load_incluster_config()
    except Exception: config.load_kube_config()
    return client.CoreV1Api()

def list_pods(label_selector: str):
    v1 = k8s()
    return v1.list_namespaced_pod(NAMESPACE, label_selector=label_selector).items

def list_scorer_eps():
    eps = []
    for p in list_pods(SCORER_LABEL):
        if p.status.pod_ip:
            eps.append((p.spec.node_name, p.status.pod_ip))
    return eps

def list_rap_eps():
    eps = []
    for p in list_pods(RAP_LABEL):
        if p.status.pod_ip:
            eps.append((p.spec.node_name, p.status.pod_ip))
    return eps

def scrape_scores():
    global scores_cache
    items = []
    for node, ip in list_scorer_eps():
        try:
            r = requests.get(f"http://{ip}:{SCORER_PORT}/score", timeout=2)
            r.raise_for_status()
            j = r.json()
            items.append({"host": j.get("host", node), "score": int(j.get("score",0))})
        except Exception:
            items.append({"host": node, "score": -1})
    items.sort(key=lambda x: x["score"], reverse=True)
    scores_cache = {"items": items, "ts": datetime.utcnow().isoformat()+"Z"}

def parse_prom_kv(line: str) -> tuple:
    # rap_requests_total{node="x",pod="y"} 123
    try:
        metric, val = line.strip().split(" ")
        labels = metric[metric.find("{")+1:metric.find("}")]
        kv = {}
        for part in labels.split(","):
            if not part: continue
            k,v = part.split("=")
            kv[k] = v.strip('"')
        return kv, float(val)
    except Exception:
        return {}, 0.0

def scrape_traffic():
    global traffic_cache, _prev_totals, _prev_ts
    now = time.time()
    per_node_total = {}   # sum of totals across pods on node

    for node, ip in list_rap_eps():
        try:
            r = requests.get(f"http://{ip}:{RAP_PORT}/metrics", timeout=2)
            r.raise_for_status()
            total = 0.0
            for line in r.text.splitlines():
                if line.startswith("rap_requests_total"):
                    kv, val = parse_prom_kv(line)
                    total += val
            per_node_total[node] = per_node_total.get(node, 0.0) + total
        except Exception:
            # keep missing -> no add
            pass

    items = []
    if _prev_ts is not None and per_node_total:
        dt = max(now - _prev_ts, 0.0001)
        for node, tot in per_node_total.items():
            prev = _prev_totals.get(node, tot)
            d = max(tot - prev, 0.0)
            rps = d / dt
            items.append({"host": node, "rps": round(rps,3), "total": int(tot)})
    else:
        # first pass -> rps unknown, set 0
        for node, tot in per_node_total.items():
            items.append({"host": node, "rps": 0.0, "total": int(tot)})

    items.sort(key=lambda x: x["rps"], reverse=True)
    traffic_cache = {"items": items, "ts": datetime.utcnow().isoformat()+"Z"}
    _prev_totals = per_node_total
    _prev_ts = now

def choose_decision(local_node: str, threshold: int = 500, k: int = 2, excludes=None):
    items = scores_cache["items"] or []
    excludes = set(excludes or [])

    def filt(arr):
        return [i for i in arr if i["host"] not in excludes]

    local = next((i for i in items if i["host"] == local_node), None)
    cand = filt(items)

    if local and local["host"] not in excludes and int(local["score"]) >= int(threshold):
        rest = [i for i in cand if i["host"] != local["host"]]
        return {"primary": local, "fallback": rest[:k], "reason": "local_ok"}

    rest = [i for i in cand if not local or i["host"] != local["host"]]
    primary = rest[0] if rest else None
    fallback = rest[1:1+k] if len(rest) > 1 else []
    return {"primary": primary, "fallback": fallback, "reason": "local_overloaded" if local else "local_missing"}

def busiest_node():
    items = traffic_cache["items"] or []
    return items[0]["host"] if items else None

def loop():
    while True:
        try: scrape_scores()
        except Exception: pass
        try: scrape_traffic()
        except Exception: pass
        time.sleep(SCRAPER_PERIOD)

class H(BaseHTTPRequestHandler):
    def _w(self, code, obj):
        body = json.dumps(obj).encode()
        self.send_response(code); self.send_header("Content-Type","application/json")
        self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)

    def do_GET(self):
        p = self.path
        if p.startswith("/health"): self._w(200, {"ok": True}); return
        if p.startswith("/scores"): self._w(200, scores_cache["items"]); return
        if p.startswith("/traffic"): self._w(200, traffic_cache); return
        if p.startswith("/busiest"):
            b = busiest_node()
            self._w(200, {"host": b} if b else {}); return
        if p.startswith("/best"):
            arr = scores_cache["items"]
            self._w(200, (arr[0] if arr else {})); return
        if p.startswith("/score"):
            qs = parse_qs(urlparse(p).query); node = qs.get("node", [None])[0]
            if not node: self._w(400, {"error":"node required"}); return
            for i in scores_cache["items"]:
                if i["host"] == node: self._w(200, i); return
            self._w(404, {"error":"not found"}); return
        if p.startswith("/decision"):
            qs = parse_qs(urlparse(p).query)
            local = qs.get("local", [None])[0]
            if not local: self._w(400, {"error":"local required"}); return
            threshold = int(qs.get("threshold", ["500"])[0])
            k = int(qs.get("k", ["2"])[0])
            exclude = qs.get("exclude", [""])[0]
            excludes = [x for x in exclude.split(",") if x]
            self._w(200, choose_decision(local, threshold, k, excludes)); return
        if p.startswith("/decision_auto"):
            qs = parse_qs(urlparse(p).query)
            local = busiest_node()
            if not local: self._w(503, {"error":"no traffic"}); return
            threshold = int(qs.get("threshold", ["500"])[0])
            k = int(qs.get("k", ["2"])[0])
            self._w(200, choose_decision(local, threshold, k, [])); return

        self._w(404, {"error":"not found"})

if __name__ == "__main__":
    t=threading.Thread(target=loop, daemon=True); t.start()
    HTTPServer(("0.0.0.0", PORT), H).serve_forever()
