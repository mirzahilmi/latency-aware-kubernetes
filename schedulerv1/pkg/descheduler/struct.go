package descheduler

import (
	"crypto/tls"
	"log"
	"os"
	"strconv"
	"sync"

	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/influx"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/scheduler"

	"k8s.io/client-go/kubernetes"
)

type AdaptiveDescheduler struct {
	kubeClient kubernetes.Interface
	influxService *influx.Service

	bucket string
	namespace string

	scoringCfg scheduler.ScoringConfig
	deschedCfg DeschedulerConfig

	kubeTLSConfig *tls.Config
	kubeToken     string
	
	mu sync.RWMutex
	prevTopNode string
}

type DeschedulerConfig struct {
	interval float64 

	idleCpuThres float64
	idleMemThres float64
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
		idleMemThres: parse("IDLEMEM_THRESHOLD"),
	}
}

type Controller struct {
    descheduler *AdaptiveDescheduler
    config DeschedulerConfig
}


