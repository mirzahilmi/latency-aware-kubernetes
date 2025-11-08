package extender

import (
    "context"
    "encoding/json"
    "net/http"

    corev1 "k8s.io/api/core/v1"
    metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
    "k8s.io/client-go/kubernetes"
    "k8s.io/client-go/rest"

    "github.com/rs/zerolog/log"
)

// HandleBind — handles /bind request from the scheduler
func (e *Extender) HandleBind(w http.ResponseWriter, r *http.Request) {
    var args ExtenderBindingArgs
    if err := json.NewDecoder(r.Body).Decode(&args); err != nil {
        http.Error(w, err.Error(), http.StatusBadRequest)
        return
    }

    log.Info().
        Str("pod", args.PodNamespace+"/"+args.PodName).
        Str("node", args.Node).
        Msg("[EXTENDER] Received bind request")

    cfg, err := rest.InClusterConfig()
    if err != nil {
        log.Error().Err(err).Msg("[EXTENDER] Failed to get in-cluster config")
        http.Error(w, err.Error(), http.StatusInternalServerError)
        return
    }

    clientset, err := kubernetes.NewForConfig(cfg)
    if err != nil {
        log.Error().Err(err).Msg("[EXTENDER] Failed to create kube clientset")
        http.Error(w, err.Error(), http.StatusInternalServerError)
        return
    }
    if e.clientset == nil {
        cfg, _ := rest.InClusterConfig()
        e.clientset, _ = kubernetes.NewForConfig(cfg)
    }

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

    if err := clientset.CoreV1().Pods(args.PodNamespace).Bind(context.TODO(), binding, metav1.CreateOptions{}); err != nil {
        log.Error().
            Err(err).
            Str("pod", args.PodNamespace+"/"+args.PodName).
            Str("node", args.Node).
            Msg("[EXTENDER] Bind failed")

        w.Header().Set("Content-Type", "application/json")
        json.NewEncoder(w).Encode(&ExtenderBindingResult{Error: err.Error()})
        return
    }

    log.Info().
        Str("pod", args.PodNamespace+"/"+args.PodName).
        Str("node", args.Node).
        Msg("[EXTENDER] Bind succeeded")

    w.Header().Set("Content-Type", "application/json")
    json.NewEncoder(w).Encode(&ExtenderBindingResult{})
}
