package scheduler

import (
    "context"
    "encoding/json"
    "net/http"

    corev1 "k8s.io/api/core/v1"
    metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
    extenderv1 "k8s.io/kube-scheduler/extender/v1"

    "github.com/rs/zerolog/log"
)

// handles /bind request from the scheduler (pkg/scheduler/routes.go)
func (e *Extender) Bind(w http.ResponseWriter, r *http.Request) {
    var args extenderv1.ExtenderBindingArgs

    if err := json.NewDecoder(r.Body).Decode(&args); err != nil {
        http.Error(w, err.Error(), http.StatusBadRequest)
        return
    }

    pods := args.PodName
	podNs := args.PodNamespace
	targetNode := args.Node
    log.Info().
        Str("pod", podNs+"/"+pods).
        Str("node", targetNode).
        Msg("[BIND] Received request")

    binding := &corev1.Binding{
        ObjectMeta: metav1.ObjectMeta{
            Name:      args.PodName,
            Namespace: args.PodNamespace,
            UID:       args.PodUID,
        },
        Target: corev1.ObjectReference{
            Kind: "Node",
            Name: args.Node,
        },
    }

    err := e.kubeClient.CoreV1().Pods(podNs).Bind(context.TODO(), binding, metav1.CreateOptions{})
	
	result := extenderv1.ExtenderBindingResult{
		Error: "",
	}
	
	if err != nil {
		log.Error().Err(err).Msg("[BIND] Failed to bind pod")
		result.Error = err.Error()
	} else {
		log.Info().Msg("[BIND] Success")
	}

	// Return success response
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(result)
}
