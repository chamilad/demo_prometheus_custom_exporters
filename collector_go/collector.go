package main

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"

	"github.com/prometheus/client_golang/prometheus"
)

// A struct to implement prometheus.Collector
type CustomCollector struct {
	serverURL string

	health      prometheus.Gauge
	cpu         *prometheus.GaugeVec // GaugeVec for labelled metrics
	memoryTotal prometheus.Gauge
	memoryUsed  prometheus.Gauge
}

// to deserialise the response from the server
type ServerMetricsResponse struct {
	CPU struct {
		Load1m  float64 `json:"load_1m"`
		Load5m  float64 `json:"load_5m"`
		Load15m float64 `json:"load_15m"`
	} `json:"cpu"`

	Memory struct {
		BytesTotal int64 `json:"total_bytes"`
		BytesUsed  int64 `json:"used_bytes"`
	} `json:"memory"`
}

func NewCollector(namespace string, serverUrl string) *CustomCollector {
	collector := &CustomCollector{
		serverURL: serverUrl,

		health: prometheus.NewGauge(
			prometheus.GaugeOpts{
				Name:      "health",
				Help:      "health of the server",
				Namespace: namespace,
			},
		),

		cpu: prometheus.NewGaugeVec(
			prometheus.GaugeOpts{
				Name:      "cpu_load",
				Help:      "CPU load average with 1m, 5m, and 15m labels",
				Namespace: namespace,
			},
			[]string{"bucket"},
		),

		memoryTotal: prometheus.NewGauge(
			prometheus.GaugeOpts{
				Name:      "memory_bytes_total",
				Help:      "total memory in bytes",
				Namespace: namespace,
			},
		),

		memoryUsed: prometheus.NewGauge(
			prometheus.GaugeOpts{
				Name:      "memory_bytes_used",
				Help:      "memory usage in bytes",
				Namespace: namespace,
			},
		),
	}

	return collector
}

// Describe is required in client_golang apparently.
func (c *CustomCollector) Describe(ch chan<- *prometheus.Desc) {
	ch <- c.health.Desc()
	ch <- c.cpu.With(prometheus.Labels{"bucket": "1m"}).Desc()
	ch <- c.cpu.With(prometheus.Labels{"bucket": "5m"}).Desc()
	ch <- c.cpu.With(prometheus.Labels{"bucket": "15m"}).Desc()
	ch <- c.memoryTotal.Desc()
	ch <- c.memoryUsed.Desc()
}

// Collect implements the bulk of the metrics collection and processing
func (c *CustomCollector) Collect(ch chan<- prometheus.Metric) {
	// check health first
	healthReq, err := http.NewRequest(http.MethodGet, fmt.Sprintf("%s/healthz", c.serverURL), nil)
	if err != nil {
		fmt.Errorf("could not initialize a request to scrape health status: %s", err)
		return
	}

	httpClient := &http.Client{}
	resp, err := httpClient.Do(healthReq)
	if err != nil {
		fmt.Errorf("error while reading server health status: %s", err)
		return
	}

	if resp.StatusCode != http.StatusOK {
		fmt.Errorf("invalid response from server health status: %s", resp.Status)

		// set values to the metrics
		c.health.Set(float64(0))
		c.cpu.With(prometheus.Labels{"bucket": "1m"}).Set(float64(0))
		c.cpu.With(prometheus.Labels{"bucket": "5m"}).Set(float64(0))
		c.cpu.With(prometheus.Labels{"bucket": "15m"}).Set(float64(0))
		c.memoryTotal.Set(float64(0))
		c.memoryUsed.Set(float64(0))

		// share with the prometheus client to do its thing
		ch <- c.health
		ch <- c.cpu.With(prometheus.Labels{"bucket": "1m"})
		ch <- c.cpu.With(prometheus.Labels{"bucket": "5m"})
		ch <- c.cpu.With(prometheus.Labels{"bucket": "15m"})
		ch <- c.memoryTotal
		ch <- c.memoryUsed

		return
	}

	// if health is good, check for metrics
	// could check for metrics anyways, depends on server behaviour
	metricsReq, err := http.NewRequest(http.MethodGet, fmt.Sprintf("%s/stats", c.serverURL), nil)
	if err != nil {
		fmt.Errorf("could not initialize a request to scrape health status: %s", err)
		return
	}

	metricsResp, err := httpClient.Do(metricsReq)
	if err != nil {
		fmt.Errorf("error while reading server health status: %s", err)
		return
	}

	respContent, err := io.ReadAll(metricsResp.Body)
	if err != nil {
		fmt.Errorf("error while reading response from Kibana status: %s", err)
		return
	}

	metrics := &ServerMetricsResponse{}
	err = json.Unmarshal(respContent, &metrics)
	if err != nil {
		fmt.Errorf("error while unmarshalling Kibana status: %s\nProblematic content:\n%s", err, respContent)
		return
	}

	// same as above, set and share
	c.health.Set(float64(1))
	c.cpu.With(prometheus.Labels{"bucket": "1m"}).Set(float64(metrics.CPU.Load1m))
	c.cpu.With(prometheus.Labels{"bucket": "5m"}).Set(float64(metrics.CPU.Load5m))
	c.cpu.With(prometheus.Labels{"bucket": "15m"}).Set(float64(metrics.CPU.Load15m))
	c.memoryTotal.Set(float64(metrics.Memory.BytesTotal))
	c.memoryUsed.Set(float64(metrics.Memory.BytesUsed))

	ch <- c.health
	ch <- c.cpu.With(prometheus.Labels{"bucket": "1m"})
	ch <- c.cpu.With(prometheus.Labels{"bucket": "5m"})
	ch <- c.cpu.With(prometheus.Labels{"bucket": "15m"})
	ch <- c.memoryTotal
	ch <- c.memoryUsed
}
