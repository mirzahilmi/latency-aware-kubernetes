<h1> Latency-Aware Kubernetes Scheduler Extender</h1>

<h2> Overview</h2>
<p>
This project is a <b>custom Kubernetes scheduler extender</b> written in <b>Rust (Axum)</b>.<br>
It introduces <b>filtering</b> and <b>prioritization</b> logic for pod scheduling based on:
</p>
<ul>
  <li> <b>EWMA CPU usage</b> (per node)</li>
  <li> <b>EWMA latency</b> between nodes</li>
  <li> <b>Traffic load metrics</b> from Prometheus/Traefik</li>
</ul>
<p>
👉 The goal is to optimize <b>pod placement</b> for low-latency workloads and high-traffic scenarios in <b>edge/distributed Kubernetes clusters</b>.
</p>

<hr/>

<h2> Project Structure</h2>
<pre>
extender/src
├── handlers
│   ├── filter.rs        # /filter handler → eliminate overloaded nodes
│   ├── prioritize.rs    # /prioritize handler → rank nodes
│   └── mod.rs
├── kube_client.rs       # Build node → prober pod mapping (via K8s API)
├── main.rs              # Entry point (Axum HTTP server)
├── models.rs            # Data models for Extender API, Probe, Prometheus
└── state.rs             # Global state (probes cache, pod counts, etc.)
</pre>

<hr/>

<h2> Components</h2>

<h3>1️ Extender (Rust, Axum)</h3>
<ul>
  <li>HTTP endpoints:
    <ul>
      <li><code>/healthz</code> → health check</li>
      <li><code>/filter</code> → node filtering</li>
      <li><code>/prioritize</code> → node prioritization</li>
    </ul>
  </li>
  <li>Manages global <code>AppState</code>:
    <ul>
      <li>Probe results (CPU + latency per node)</li>
      <li>Pod counts</li>
      <li>Last filtered nodes</li>
      <li>Node with highest traffic (from Prometheus)</li>
      <li>Node → Prober pod mapping</li>
    </ul>
  </li>
</ul>

<h3> Custom Scheduler</h3>
<p>
Defined in <b>Extender-scheduler.yaml</b>.<br>
Runs as a separate kube-scheduler instance with:
</p>
<ul>
  <li>All default scoring plugins <b>disabled</b></li>
  <li>Extender as the only filter & prioritize logic</li>
</ul>
<p>Pods must specify:</p>
<pre>
schedulerName: latency-aware-scheduler
</pre>

<h3> Prober (StatefulSet per Node)</h3>
<ul>
  <li>Deployed per worker node (anti-affinity ensures one per node).</li>
  <li>Measures:
    <ul>
      <li>EWMA CPU usage</li>
      <li>EWMA latency between nodes</li>
    </ul>
  </li>
  <li>Exposes <code>/scores</code> on port 3000.</li>
</ul>

<h3>Traefik (Ingress Controller)</h3>
<ul>
  <li>Handles <b>external user traffic</b>.</li>
  <li>Exposes <b>traffic metrics</b> per node (<code>:9100</code>).</li>
  <li>Metrics are scraped by Prometheus and labeled with the hosting node.</li>
</ul>

<h3>Prometheus</h3>
<ul>
  <li>Scrapes Traefik metrics (<code>traefik_entrypoint_requests_total</code>).</li>
  <li>Extender queries Prometheus using PromQL:</li>
</ul>
<pre>
topk(1, sum by (node) (rate(traefik_entrypoint_requests_total{entrypoint="web"}[1m])))
</pre>
<p>→ returns the <b>busiest node</b>.</p>

<h3>Hellopod (Dummy Workload)</h3>
<ul>
  <li>Test deployment with multiple replicas (e.g. 9).</li>
  <li>Uses custom scheduler:</li>
</ul>
<pre>
schedulerName: latency-aware-scheduler
</pre>

<hr/>

<h2>Workflow</h2>

<h3>1. Traffic Flow</h3>
<p>User → <b>Traefik Ingress</b> → forwarded to <b>hellopod</b> pods</p>

<h3>2. Metrics Collection</h3>
<ul>
  <li>Traefik pods expose metrics at <code>:9100/metrics</code></li>
  <li>Prometheus scrapes them via <code>traefik-metrics</code> headless service</li>
</ul>

<h3>3. Extender Periodic Update</h3>
<ul>
  <li>Query Prometheus → find busiest node</li>
  <li>Fetch probe data (<code>/scores</code>) from prober pod</li>
  <li>Update EWMA CPU & latency cache</li>
</ul>

<h3>4. Filter Phase</h3>
<p>Scheduler calls <code>/filter</code> → node eliminated if:</p>
<ul>
  <li>CPU usage > 85%</li>
  <li>Latency > 50%</li>
  <li>No probe data available</li>
</ul>

<h3>5. Prioritize Phase</h3>
<p>Scheduler calls <code>/prioritize</code> → extender computes score:</p>
<pre>
score = (1 - CPU) * 0.3 + (1 - Latency) * 0.7
</pre>
<ul>
  <li>CPU 70–85% → penalty -15 points</li>
  <li>Nodes ranked → top node chosen</li>
</ul>

<h3>6. Scheduler Decision</h3>
<ul>
  <li>Pod placed on best-ranked node</li>
  <li>Pod count incremented → fairness in next cycle</li>
</ul>

<hr/>

<h2> Deployment Order</h2>
<ol>
  <li>Prometheus → <code>prometheus.yaml</code></li>
  <li>Traefik → <code>traefik.yaml</code></li>
  <li>Prober → <code>prober.yaml</code></li>
  <li>Extender → <code>extender.yaml</code></li>
  <li>Custom Scheduler → <code>extender-scheduler.yaml</code></li>
  <li>Test Workload (hellopod) → <code>hellopod.yaml</code></li>
</ol>

<hr/>

<h2> Current Status</h2>
<ul>
  <li>Node filtering  (CPU & latency thresholds)</li>
  <li>Node ranking  (weighted CPU + latency score)</li>
  <li>CPU penalty  (warning zone handling)</li>
  <li>Verified with <code>hellopod</code> workload under Traefik traffic</li>
</ul>
