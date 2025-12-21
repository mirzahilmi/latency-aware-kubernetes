package main

import (
	"crypto/rand"
	"crypto/sha256"
	"embed"
	"fmt"
	"html/template"
	"net/http"
	"os"
	"strconv"
	"time"
)

const (
	ITERS = 10_000
)

//go:embed index.html
var templatesFs embed.FS

type data struct {
	HostName,
	HostIpv4,
	PodName,
	PodNamespace,
	PodIpv4,
	OpIters string
}

func main() {
	hostName := os.Getenv("NODE_NAME")
	if hostName == "" {
		hostName = "N/A"
	}
	hostIpv4 := os.Getenv("NODE_IP")
	if hostIpv4 == "" {
		hostIpv4 = "N/A"
	}
	podName := os.Getenv("POD_NAME")
	if podName == "" {
		podName = "N/A"
	}
	podNamespace := os.Getenv("POD_NAMESPACE")
	if podNamespace == "" {
		podNamespace = "N/A"
	}
	podIpv4 := os.Getenv("POD_IP")
	if podIpv4 == "" {
		podIpv4 = "N/A"
	}

	envCpuCost := os.Getenv("TARGET_CPU_COST_IN_MILLISECONDS")
	if envCpuCost == "" {
		envCpuCost = "5"
	}
	cpuCost, err := strconv.Atoi(envCpuCost)
	if err != nil {
		panic(err)
	}
	cpuCostDuration := time.Duration(cpuCost) * time.Millisecond
	iters := calculateIterations(cpuCostDuration)

	http.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		h := sha256.New()
		for range iters {
			b := make([]byte, 1024) // 1 KiB
			rand.Read(b)
			h.Write(b)
		}

		tmpl, err := template.ParseFS(templatesFs, "index.html")
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}

		data := data{
			HostName:     hostName,
			HostIpv4:     hostIpv4,
			PodName:      podName,
			PodNamespace: podNamespace,
			PodIpv4:      podIpv4,
			OpIters:      fmt.Sprintf("%x", h.Sum(nil)),
		}
		tmpl.Execute(w, data)
	})

	envPort := os.Getenv("PORT")
	port, err := strconv.Atoi(envPort)
	if err != nil {
		panic(err)
	}
	addr := fmt.Sprintf(":%d", port)

	fmt.Printf("Listening HTTP on address %s\n", addr)
	http.ListenAndServe(addr, nil)
}

func calculateIterations(duration time.Duration) int64 {
	start := time.Now()
	h := sha256.New()
	for range ITERS {
		b := make([]byte, 1024) // 1 KiB
		rand.Read(b)
		h.Write(b)
	}
	elapsed := time.Since(start)

	costPerIter := elapsed / ITERS
	iters := int64(duration / costPerIter)

	return iters
}
