# score-aggregator

Central scraper + decision API.  
It periodically scrapes:
- **rap-proxy** (`/metrics`) to compute **RPS** per node,
- **node-scorer** (`/score`) to rank node **health**.

Then it exposes consolidated JSON for load steering (RAP local-first, health-gated by `threshold`).

## Endpoints
- `GET /health`: Returns `ok` for liveness check.
- `GET /scores`: Sorted list of `{"host","score"}` (highest first).
- `GET /traffic`: `{"items":[{"host","rps","total"},...], "ts": "<iso>"}` – live busiest info (RPS = Δcounter/Δtime). 
- `GET /busiest`: `{"host":"<node>"}` – node with highest RPS (tie-break: impl. dependent).
- `GET /decision?local=<node>&threshold=<n>&k=<m>[&exclude=...]`: For RAP decision, **local-first** else **best non-local**. Returns:<br>`{"primary":{"host","score"}, "fallback":[...], "reason":"local_ok|local_overloaded|local_missing"}`

**Decision example**
```json
{
  "primary": { "host": "belajarkube", "score": 822 },
  "fallback": [
    { "host": "belajarkube-m03", "score": 838 },
    { "host": "belajarkube-m02", "score": 837 }
  ],
  "reason": "local_ok"
}