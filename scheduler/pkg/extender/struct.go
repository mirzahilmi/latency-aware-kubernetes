package extender

import (
	"log"
	"os"
	"strconv"
	"sync"

	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/influx"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/prober"
	"k8s.io/apimachinery/pkg/types"
	"k8s.io/client-go/kubernetes"
)

type Extender struct {
	influxService *influx.Service
	bucket        string
	topNode       string

	mu            sync.RWMutex
	proberScores  map[string]prober.ScoreData
	cachedTraffic map[string]float64 
	cachedTrafficNorm map[string]float64

	clientset     *kubernetes.Clientset
	namespace     string

	Config ScoringConfig
	IsColdStart bool
}

type ScoringConfig struct {
    WeightLatency      float64
    WeightCPU          float64
	WeightTraffic       float64
    ScaleFactor        float64
	LatencyThreshold   float64
	cpuThreshold	float64
	WarmupThreshold float64
}


func parseEnvFloat(key string) float64 {
	val := os.Getenv(key)
	if val == "" {
		log.Fatalf("missing required environment variable: %s", key)
	}
	f, err := strconv.ParseFloat(val, 64)
	if err != nil {
		log.Fatalf("invalid value for %s=%s (must be numeric)", key, val)
	}
	return f
}

func LoadScoringConfig() ScoringConfig {
	cfg := ScoringConfig{

		WeightLatency:      parseEnvFloat("WEIGHT_LATENCY"),
		WeightCPU:          parseEnvFloat("WEIGHT_CPU"),
		WeightTraffic:      parseEnvFloat("WEIGHT_TRAFFIC"),
		ScaleFactor:        parseEnvFloat("SCALE_FACTOR"),
		LatencyThreshold:	parseEnvFloat("LATENCY_THRESHOLD"),
		cpuThreshold:		parseEnvFloat("CPU_THRESHOLD"),
		WarmupThreshold: 	parseEnvFloat("WARMUP_THRESHOLD"),
	}
	return cfg
}

// SafeSetProberScores safely replaces prober score data (used in cold-start)
func (e *Extender) SafeSetProberScores(scores []prober.ScoreData) {
	e.mu.Lock()
	defer e.mu.Unlock()

	for _, s := range scores {
		e.proberScores[s.Hostname] = s
	}
}

// SafeEnableColdStart safely toggles the cold-start mode
func (e *Extender) SafeEnableColdStart() {
	e.mu.Lock()
	defer e.mu.Unlock()
	e.IsColdStart = true
}

// ================== KUBE STRUCT ==================

type SchedulerRequest struct {
	Pod   PodInfo  `json:"pod"`
	Nodes NodeList `json:"nodes"`
}

type PodInfo struct {
	Metadata struct {
		Name      string `json:"name"`
		Namespace string `json:"namespace"`
	} `json:"metadata"`
}

type NodeList struct {
	Items []Node `json:"items"`
}

type Node struct {
	Metadata NodeMetadata `json:"metadata"`
}

type NodeMetadata struct {
	Name string `json:"name"`
}

type ExtenderFilterResult struct {
	Nodes       *NodeList         `json:"nodes,omitempty"`
	FailedNodes map[string]string `json:"failedNodes,omitempty"`
	Error       string            `json:"error,omitempty"`
}

type HostPriority struct {
	Host  string `json:"host"`
	Score int64    `json:"score"`
}

type HostPriorityList []HostPriority

type ExtenderBindingArgs struct {
    PodName      string    `json:"podName"`
    PodNamespace string    `json:"podNamespace"`
    PodUID       types.UID `json:"podUID"`
    Node         string    `json:"node"`
}

type ExtenderBindingResult struct {
    Error string `json:"error,omitempty"`
}




