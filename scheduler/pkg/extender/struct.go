package extender

import (
	"sync"

	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/influx"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/prober"
	"k8s.io/client-go/kubernetes"
)

type Extender struct {
	influxService *influx.Service
	bucket        string
	topNode       string
	mu            sync.RWMutex
	proberScores  map[string]prober.ScoreData
	cachedTraffic map[string]float64 
	clientset     *kubernetes.Clientset
	namespace     string
	distribution  *PodDistribution // ✅ tambah field ini
}


// ================== STRUCT LAIN ==================

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
	Score int    `json:"score"`
}

type HostPriorityList []HostPriority

var (
	LatencyThreshold = 0.17
	CPUThreshold     = 0.20
	WeightLatency = 0.4
	WeightCPU     = 0.3
	WeightTraffic = 0.3
)

const SCALE_FACTOR = 1000000
