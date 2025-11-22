package extender

import (
	"log"
	"os"
	"strconv"
	"sync"
	"time"

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
	Config 		  ScoringConfig

	lastPenalized map[string]struct {
        CPU     float64
		Mem     float64
        Applied time.Time
    }
}

type ScoringConfig struct {
    WeightLatency      float64
    WeightCPU          float64
	WeightMem          float64
	WeightTraffic      float64
    ScaleFactor        float64

	LatencyThreshold   float64
	cpuThreshold	   float64
	memThreshold       float64

	vmPenaltyCPU		float64
	rpiPenaltyCPU		float64

	vmPenaltyMem		float64
	rpiPenaltyMem		float64
}

func LoadScoringConfig() ScoringConfig {
	parse := func(key string) float64 {
		val := os.Getenv(key)
		f, err := strconv.ParseFloat(val, 64)
		if err != nil {
			log.Fatalf("invalid value for %s=%s (must be numeric)", key, val)
		}
		return f
	}

	return ScoringConfig{
		WeightLatency:      parse("WEIGHT_LATENCY"),
		WeightCPU:          parse("WEIGHT_CPU"),
		WeightTraffic:      parse("WEIGHT_TRAFFIC"),
		WeightMem:     		parse("WEIGHT_MEMORY"),
		ScaleFactor:        parse("SCALE_FACTOR"),

		LatencyThreshold:	parse("LATENCY_THRESHOLD"),
		cpuThreshold:		parse("CPU_THRESHOLD"),
		memThreshold:		parse("MEM_THRESHOLD"),

		vmPenaltyCPU:		parse("VM_PENALTY_CPU"),
		rpiPenaltyCPU: 		parse("RPI_PENALTY_CPU"),
		vmPenaltyMem:		parse("VM_PENALTY_MEM"),
		rpiPenaltyMem: 		parse("RPI_PENALTY_MEM"),
	}
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
	Metadata struct {
		Name string `json:"name"`
	} `json:"metadata"`
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




