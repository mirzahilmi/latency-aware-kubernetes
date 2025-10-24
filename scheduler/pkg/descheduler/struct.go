package descheduler

import (
	"sync"
	"time"

	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/influx"
	"k8s.io/client-go/kubernetes"
	"k8s.io/client-go/rest"
)

type Descheduler struct {
	clientset     *kubernetes.Clientset
	influxService *influx.Service
	bucket        string
	config        *rest.Config
	mu            sync.Mutex

	prevTopNode  string
	lastEviction time.Time
}

func NewDescheduler(clientset *kubernetes.Clientset, service *influx.Service, bucket string) *Descheduler {
	return &Descheduler{
		clientset:     clientset,
		influxService: service,
		bucket:        bucket,
	}
}
