package descheduler

import (
	"log"
	"os"
	"strconv"
	"sync"

	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/influx"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/scheduler"

	"k8s.io/client-go/kubernetes"

	metricsclient "k8s.io/metrics/pkg/client/clientset/versioned"
)

type AdaptiveDescheduler struct {
	kubeClient kubernetes.Interface
	metricsClient metricsclient.Interface
	influxService *influx.Service

	bucket string
	namespace string

	scoringCfg scheduler.ScoringConfig
	deschedCfg DeschedulerConfig
	
	mu sync.RWMutex
	prevTopNode string
}

//TODO: add memory threshold config
type DeschedulerConfig struct {
	interval float64 
	idleCpuThres float64
}

func LoadDeschedulerConfig() DeschedulerConfig {
	parse := func(key string) float64 {
		val := os.Getenv(key)
		f, err := strconv.ParseFloat(val, 64)
		if err != nil {
			log.Fatalf("invalid value for %s=%s (must be numeric)", key, val)
		}
		return f
	}
	return DeschedulerConfig{
		interval: parse("DESCHED_INTERVAL"),
		idleCpuThres: parse("IDLECPU_THRESHOLD"),
	}
}

type Controller struct {
    descheduler *AdaptiveDescheduler
    cfg DeschedulerConfig
}

