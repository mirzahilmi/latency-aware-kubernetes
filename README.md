> [!CAUTION]
> The project is still under active development. I am not responsible for any damage caused by this 
prototype. Please consider creating an issue if you have any questions.

## TLDR

Kubernetes scheduling algorithm based on latency metric received by measuring ping latency of a 
high traffic client within the specified time context.

```mermaid
sequenceDiagram
    participant Cr as CronJob
    participant D as NingD
    participant K as Control Plane
    participant G as Site Gateway
    participant Sc as Scheduler
    participant B as MQTT Broker
    participant SX as Site X
    participant P as Prober
    participant SN as Site N

    activate Cr
    activate K
    activate G
    activate B
    activate SX
    activate SN
    activate Sc

    Note over Cr,G: Phase 1: Traffic Discovery
    Cr->>D: Create NingD job
    activate D
    D->>K: Discover site gateways
    K-->>D: Site gateway domains + Node name
    D->>G: Pull OpenTelemetry metrics (gRPC)
    G-->>D: OpenTelemetry metrics (gRPC)
    D->>D: Sort gateway by traefik_entrypoint_requests_total
    D->>D: Select highest traefik_entrypoint_requests_total resulting Site X

    Note over SX,SN: Phase 2: Latency Probing
    D->>K: Schedule latency probing job in Site X
    deactivate D
    K->>K: Store prober job creation in etcd
    K->>SX: Provision prober job
    SX->>P: Start prober
    activate P
    P->>SN: Ping A N-times
    P->>SN: Ping B N-times
    P->>SN: Ping C N-times
    SN-->>P: PONG!
    P->>P: Parse latency metrics into protocol buffer
    P->>B: Publish latency metrics
    deactivate P

    Note over K,SN: Phase 3: Reschedule application pods
    B->>Sc: Publish latency metrics
    Sc->>Sc: Average latency for each site
    Sc->>Sc: Sort site by latency descending
    Sc->>K: Reschedule/move existing application pod into the designated sorted site
    K->>K: Store application pod reschedule task in etcd
    K->>SN: Shutting down idling application pod, but remaining atleast 1
    SN->>SN: Kill application pod
    K->>SX: Schedule more application pod until limit
    K->>SN: Schedule application pod on site sorted earlier if theres any remaining

    deactivate Cr
    deactivate K
    deactivate G
    deactivate B
    deactivate SX
    deactivate SN
    deactivate Sc
```

## How to run?

### Prerequisite

- [docker](https://www.docker.com/)
- [kind](https://kind.sigs.k8s.io/)
- kubectl
- [helm](https://helm.sh/)

### Setup

The base environment can be runned with a single make command below:
```shell
make all
```

