package main

import (
	"embed"
	"fmt"
	"html/template"
	"net/http"
	"os"
	"strconv"
	"time"
)

//go:embed index.html
var templatesFs embed.FS

type data struct {
	HostName,
	HostIpv4,
	PodName,
	PodNamespace,
	PodIpv4,
	Result string
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
	_ = cpuCostDuration

	http.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		// exercise(cpuCostDuration)

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

func isPrime(n int) bool {
	if n < 2 {
		return false
	}
	for i := 2; i*i <= n; i++ {
		if n%i == 0 {
			return false
		}
	}
	return true
}

func exercise(duration time.Duration) {
	start := time.Now()
	n := 2
	for time.Since(start) < duration {
		isPrime(n)
		n++
	}
}
