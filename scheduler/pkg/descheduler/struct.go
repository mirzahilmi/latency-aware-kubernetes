package descheduler

import (
	"context"

	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/extender"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/influx"
	"github.com/rs/zerolog/log"
	"k8s.io/client-go/kubernetes"
	"k8s.io/client-go/rest"
	metricsclient "k8s.io/metrics/pkg/client/clientset/versioned"
)

type AdaptiveDescheduler struct {
	clientset     *kubernetes.Clientset
	metricsClient *metricsclient.Clientset
	influxService *influx.Service
	bucket        string
	namespace     string

	config        extender.ScoringConfig
	policy 		  LatencyDeschedulerPolicySpec

	prevTopNode   string
	isRunning bool
	cancelFunc context.CancelFunc
}

func NewAdaptiveDescheduler(
	clientset *kubernetes.Clientset,
	influxSvc *influx.Service,
	bucket string,
	namespace string,
	cfg extender.ScoringConfig,
) *AdaptiveDescheduler {

	restCfg, err := rest.InClusterConfig()
	if err != nil {
		log.Fatal().Err(err).Msg("[DESCHEDULER] Failed to load in-cluster config for metrics client")
	}
	metricsClient, err := metricsclient.NewForConfig(restCfg)
	if err != nil {
		log.Fatal().Err(err).Msg("[DESCHEDULER] Failed to initialize metrics client")
	}

	return &AdaptiveDescheduler{
		clientset:     clientset,
		metricsClient: metricsClient,
		influxService: influxSvc,
		bucket:        bucket,
		namespace:     namespace,
		config:        cfg, 
		policy: 	   LatencyDeschedulerPolicySpec{},
		prevTopNode:   "",
	}
}

type LatencyDeschedulerPolicySpec struct {
    IntervalSeconds  int     `json:"intervalSeconds"`
    IdleCPUThreshold int 	 `json:"idleCPUThreshold"`
}

