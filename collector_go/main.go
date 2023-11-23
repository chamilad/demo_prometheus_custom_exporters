package main

import (
	"fmt"
	"log"
	"net/http"

	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promhttp"
)

const (
	exporterPort      = "9001"
	exporterNamespace = "my_server_go"
	serverURL         = "http://127.0.0.1:8443"
)

func main() {
	serverCollector := NewCollector(exporterNamespace, serverURL)
	prometheus.MustRegister(serverCollector)

	// let the client handle the metrics retrieval call
	http.Handle("/", promhttp.Handler())
	log.Printf("starting metrics server at %s", exporterPort)
	log.Fatal(http.ListenAndServe(fmt.Sprintf(":%s", exporterPort), nil))
}
