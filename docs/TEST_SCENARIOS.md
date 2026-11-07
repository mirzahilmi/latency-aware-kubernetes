# Test Scenario: Request Distribution Ratios
 
## Overview
 
Load distribution across the 4-node cluster is described using a ratio in the format `a:b:c:d`, where each value is the **requests per second (RPS)** directed to a specific worker node (Worker 1 through Worker 4 respectively).
---
 
## Distribution Patterns
 
### Concentrated
All traffic goes to a single worker node; the other three nodes receive zero requests. Used to simulate a fully single-node load scenario.
 
Examples: `200:0:0:0`, `1600:0:0:0`
 
### Distributed — Balanced
Traffic is spread **evenly** across all four worker nodes. Each node receives the same RPS.
 
Examples: `200:200:200:200`, `800:800:800:800`
 
### Distributed — Imbalanced
Traffic is spread across all four nodes, but **unevenly** — one node carries a heavier load than the rest. Total RPS varies across scenarios.
 
Examples: `800:400:400:400`, `3200:400:400:400`
 
### Distributed — Equal Total Throughput
Traffic is distributed unevenly across nodes, but the **total cluster throughput is fixed at 3600 RPS** for every scenario in this group. Used to isolate the effect of distribution shape while keeping aggregate load constant.
 
Examples: `1600:800:800:400`, `1200:800:800:800`
 
---
 
## Quick Reference
 
| Pattern | Load Shape | Total RPS varies? |
|---|---|---|
| Concentrated | One node takes all | Yes |
| Distributed — Balanced | All nodes equal share | Yes |
| Distributed — Imbalanced | All nodes, unequal share | Yes |
| Distributed — Equal Total Throughput | All nodes, unequal share | **No (fixed at 3600)** |
