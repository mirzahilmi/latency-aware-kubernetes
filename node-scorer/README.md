# node-scorer

Count the acumulative **node score** (0..1000) for every node using the CPU, memory, and latency metrics

## What it does
- Pulls node usage from **metrics-server** (CPU, memory).
- (Optional) Pulls latency from a **prober**.
- Normalizes and blends into a score:  
  `score = SCALE * ( W_CPU*S_cpu + W_MEM*S_mem + W_LAT*S_lat )`  
  where `S_cpu = 1 - CPU_ratio`, `S_mem = 1 - MEM_ratio`, and `S_lat` depends on the latency mode.
- Recomputes periodically and exposes HTTP.

## Endpoint
- `GET /score` : Minimal JSON: `{"host":"<node>","score":<int>}` that consumed by score aggregator.
- `GET /health`: Returns `ok` for liveness check.
- `GET /score?verbose=1`: Adds `breakdown` (cpu/mem/lat), `raw` (alloc/usage), `timestamp`, `error`. Useful for debugging.

**Verbose example**
```json
{
  "host": "belajarkube-m02",
  "score": 835,
  "breakdown": {"cpu": 0.99, "mem": 0.96, "lat": 0.5},
  "raw": {
    "alloc_cpu": 8.0,
    "alloc_mem": 16554160128.0,
    "usage_cpu": 0.061,
    "usage_mem": 647598080.0,
    "median_latency_ms": null
  },
  "timestamp": "2025-08-28T01:34:07Z",
  "error": null
}