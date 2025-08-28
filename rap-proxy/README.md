# rap-proxy

Node-local HTTP service used in the RAP (Resource Adaptive Proxy) demo.  
It increments request counters (for traffic/RPS) and exposes Prometheus metrics.

## Overview
- Receives HTTP requests (e.g., `GET /`) on each node.
- Increments a monotonic counter per request.
- Exposes request metrics at `/metrics`.
- Health check at `/health`.

## Environment
- `PORT` (default `8080`)
- `NODE_NAME`, `POD_NAME` – injected via Downward API in the DaemonSet.

