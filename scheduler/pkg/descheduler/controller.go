package descheduler

import (
	"context"
	"time"

	"github.com/rs/zerolog/log"
	"k8s.io/apimachinery/pkg/apis/meta/v1/unstructured"
	"k8s.io/apimachinery/pkg/runtime/schema"
	"k8s.io/client-go/dynamic"
	"k8s.io/client-go/dynamic/dynamicinformer"
	"k8s.io/client-go/rest"
	"k8s.io/client-go/tools/cache"
)

// WatchLatencyPolicy watches the LatencyDeschedulerPolicy CRD
func (d *AdaptiveDescheduler) WatchLatencyPolicy(ctx context.Context) error {
	cfg, err := rest.InClusterConfig()
	if err != nil {
		return err
	}

	client, err := dynamic.NewForConfig(cfg)
	if err != nil {
		return err
	}

	gvr := schema.GroupVersionResource{
		Group:    "riset.ub",
		Version:  "v1",
		Resource: "latencydeschedulerpolicies",
	}

	factory := dynamicinformer.NewFilteredDynamicSharedInformerFactory(
		client,
		5*time.Minute, // resync period
		d.namespace,
		nil, // no tweak list options
	)

	informer := factory.ForResource(gvr).Informer()

	informer.AddEventHandler(cache.ResourceEventHandlerFuncs{
		AddFunc: func(obj interface{}) {
			log.Info().Msg("[DESCHEDULER] CRD created event")
			d.handlePolicyEvent(ctx, obj, "ADDED")
		},
		UpdateFunc: func(oldObj, newObj interface{}) {
			log.Info().Msg("[DESCHEDULER] CRD updated event")
			d.handlePolicyEvent(ctx, newObj, "MODIFIED")
		},
		DeleteFunc: func(obj interface{}) {
			log.Info().Msg("[DESCHEDULER] CRD deleted event")
			d.handlePolicyEvent(ctx, obj, "DELETED")
		},
	})
	
	log.Info().Msg("[DESCHEDULER] Starting shared informer for LatencyDeschedulerPolicy...")
	go informer.Run(ctx.Done())

		//wait until cache ready
	if !cache.WaitForCacheSync(ctx.Done(), informer.HasSynced) {
		log.Error().Msg("[DESCHEDULER] CRD informer sync failed")
		return nil
	}

		log.Info().Msg("[DESCHEDULER] Shared informer started successfully")

		<-ctx.Done()
		return nil
	}

func (d *AdaptiveDescheduler) handlePolicyEvent(ctx context.Context, obj interface{}, eventType string) {
	u, ok := obj.(*unstructured.Unstructured)
	if !ok {
		log.Warn().Msg("[DESCHEDULER] Invalid CRD object type")
		return
	}

	spec, found, _ := unstructured.NestedMap(u.Object, "spec")
	if !found {
		log.Warn().Msg("[DESCHEDULER] No spec found in CRD event")
		return
	}

	enabled, _, _ := unstructured.NestedBool(spec, "enabled")
	interval, _, _ := unstructured.NestedInt64(spec, "intervalSeconds")
	idleCPU, _, _ := unstructured.NestedFloat64(spec, "idleCPUThreshold")

	d.policy.IntervalSeconds = int(interval)
	d.policy.IdleCPUThreshold = idleCPU

	log.Info().Msgf("[DESCHEDULER] Policy %s event — enabled=%v interval=%ds idleCPU=%.2f",
		eventType, enabled, interval, idleCPU)

	if enabled {
		if !d.isRunning {
			log.Info().Msgf("[DESCHEDULER] Starting adaptive loop",)
			loopCtx, cancel := context.WithCancel(ctx)
			d.cancelFunc = cancel
			d.isRunning = true
			go d.Run(loopCtx)
		}
	} else {
		if d.isRunning && d.cancelFunc != nil {
			log.Info().Msg("[DESCHEDULER] Stopping adaptive loop via CRD disable")
			d.cancelFunc()
			d.isRunning = false
		}
	}
}
