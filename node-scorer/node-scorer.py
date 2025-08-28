import os, threading, time, json, requests, statistics
from http.server import BaseHTTPRequestHandler, HTTPServer
from kubernetes import client, config
from datetime import datetime

# ---------- ENV ----------
PORT              = int(os.getenv("PORT", "8081"))
INTERVAL_SEC      = int(os.getenv("INTERVAL_SEC", "60"))
NODE_NAME         = os.getenv("NODE_NAME", "")
# Mode latensi: "score" (default, prober mengembalikan {host, score}) atau "raw"
LATENCY_MODE      = os.getenv("LATENCY_MODE", "score").lower()
# Jika mode=score, panggil endpoint ini:
LAT_SCORE_URL_TMPL = os.getenv("LAT_SCORE_URL_TMPL", "http://prober-svc.default.svc.cluster.local:8080/score?node={node}")
# Skala untuk mengubah score prober ke [0..1] bila score > 1
L_SCORE_SCALE     = float(os.getenv("L_SCORE_SCALE", "1000"))
# Jika mode=raw, fungsi lama (median ms → fungsi 1/(1+x)^alpha)
LATENCY_URL_TMPL  = os.getenv("LATENCY_URL_TMPL", "http://prober-svc.default.svc.cluster.local:8080/latency?source={node}")
L_REF_MS          = float(os.getenv("L_REF_MS", "10"))
ALPHA             = float(os.getenv("ALPHA", "1.0"))

W_CPU             = float(os.getenv("W_CPU", "0.4"))
W_MEM             = float(os.getenv("W_MEM", "0.3"))
W_LAT             = float(os.getenv("W_LAT", "0.3"))
SCALE             = int(os.getenv("SCALE", "1000"))

latest = {
    "host": NODE_NAME or "unknown",
    "score": 0,
    "breakdown": {"cpu": 0, "mem": 0, "lat": 0},
    "raw": {},
    "timestamp": None
}

def parse_cpu_to_cores(qty: str) -> float:
    # Terima "164202573n" (nano), "1234u" (micro), "250m" (milli), "2" (cores)
    q = str(qty).strip()
    if q.endswith("n"):   # nanocores
        return float(q[:-1]) / 1e9
    if q.endswith("u"):   # microcores
        return float(q[:-1]) / 1e6
    if q.endswith("m"):   # millicores
        return float(q[:-1]) / 1e3
    return float(q)       # cores

def parse_mem_to_bytes(qty: str) -> float:
    qty = str(qty).strip()
    units = [("Ki", 1024), ("Mi", 1024**2), ("Gi", 1024**3), ("Ti", 1024**4)]
    for suf, mul in units:
        if qty.endswith(suf): return float(qty[:-len(suf)]) * mul
    if qty.endswith("k"): return float(qty[:-1]) * 1000
    if qty.endswith("M"): return float(qty[:-1]) * 1_000_000
    if qty.endswith("G"): return float(qty[:-1]) * 1_000_000_000
    return float(qty)

def k8s():
    try: config.load_incluster_config()
    except Exception: config.load_kube_config()
    return client.CustomObjectsApi(), client.CoreV1Api()

def get_node_usage(node_name: str):
    metrics, core = k8s()
    nm = metrics.get_cluster_custom_object("metrics.k8s.io","v1beta1","nodes",node_name)
    usage_cpu = parse_cpu_to_cores(nm["usage"]["cpu"])
    usage_mem = parse_mem_to_bytes(nm["usage"]["memory"])
    node = core.read_node(node_name)
    alloc_cpu = parse_cpu_to_cores(node.status.allocatable["cpu"])
    alloc_mem = parse_mem_to_bytes(node.status.allocatable["memory"])
    return usage_cpu, alloc_cpu, usage_mem, alloc_mem

def get_S_lat_from_prober_score(self_node: str) -> float:
    # Expects {"host":"<node>","score": <number>}
    url = LAT_SCORE_URL_TMPL.format(node=self_node)
    try:
        r = requests.get(url, timeout=3); r.raise_for_status()
        j = r.json(); s = float(j.get("score", 0.0))
        # Map ke [0..1]: kalau sudah 0..1 biarkan; kalau >1, bagi skala
        if s > 1.0 and L_SCORE_SCALE > 0: s = s / L_SCORE_SCALE
        return max(0.0, min(s, 1.0))
    except Exception:
        return 0.5  # fallback netral

def get_S_lat_from_raw(self_node: str) -> float:
    # Expects {"source":"<node>", "targets": {"nodeX": ms, ...}}
    url = LATENCY_URL_TMPL.format(node=self_node)
    try:
        r = requests.get(url, timeout=3); r.raise_for_status()
        d = r.json(); targets = d.get("targets", {})
        vals = [float(v) for v in targets.values() if v is not None]
        if not vals: return 0.5
        med = statistics.median(vals)
        return 1.0 / (1.0 + (med / max(L_REF_MS, 0.1)) ** ALPHA)
    except Exception:
        return 0.5

def compute_once():
    global latest
    # Ambil usage
    usage_cpu, alloc_cpu, usage_mem, alloc_mem = get_node_usage(NODE_NAME)
    cpu_ratio = min(usage_cpu / max(alloc_cpu, 1e-9), 1.2)
    mem_ratio = min(usage_mem / max(alloc_mem, 1e-9), 1.2)

    # Skor CPU/Mem → makin kecil rasio, makin tinggi skor
    S_cpu = max(0.0, 1.0 - cpu_ratio)
    S_mem = max(0.0, 1.0 - mem_ratio)

    # Skor latency
    med_lat = None
    if LATENCY_MODE == "score":
        S_lat = get_S_lat_from_prober_score(NODE_NAME)
    else:
        S_lat = get_S_lat_from_raw(NODE_NAME)

    # Hitung skor total
    score_f = W_CPU * S_cpu + W_MEM * S_mem + W_LAT * S_lat
    score_i = int(round(SCALE * score_f))

    # Update payload
    latest = {
        "host": NODE_NAME,
        "score": score_i,
        "breakdown": {
            "cpu": round(S_cpu, 4),
            "mem": round(S_mem, 4),
            "lat": round(S_lat, 4),
        },
        "raw": {
            "cpu_ratio": cpu_ratio,
            "mem_ratio": mem_ratio,
            "median_latency_ms": med_lat,
            "alloc_cpu": alloc_cpu,
            "alloc_mem": alloc_mem,
            "usage_cpu": usage_cpu,
            "usage_mem": usage_mem,
        },
        "timestamp": datetime.utcnow().isoformat() + "Z",
        "error": None
    }

def loop():
    while True:
        try: compute_once()
        except Exception as e:
            latest["error"] = str(e)
        time.sleep(INTERVAL_SEC)

class H(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path.startswith("/health"):
            self.send_response(200); self.end_headers(); self.wfile.write(b"ok"); return
        if self.path.startswith("/score"):
            # default: minimalis
            minimal = {"host": latest["host"], "score": latest["score"]}
            if "verbose=1" in self.path:
                body = json.dumps(latest).encode()
            else:
                body = json.dumps(minimal).encode()
            self.send_response(200); self.send_header("Content-Type","application/json")
            self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body); return
        self.send_response(404); self.end_headers()

if __name__ == "__main__":
    t = threading.Thread(target=loop, daemon=True); t.start()
    HTTPServer(("0.0.0.0", PORT), H).serve_forever()
