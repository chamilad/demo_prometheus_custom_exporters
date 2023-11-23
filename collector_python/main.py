
import time
import requests
from prometheus_client import start_http_server
from prometheus_client.core import GaugeMetricFamily, REGISTRY
from prometheus_client.registry import Collector

EXP_PORT = 9000
SERV_URL = "http://127.0.0.1:8443"


class CustomCollector(Collector):
    def __init__(self, namespace="my_server_py"):
        self.namespace = namespace

    def collect(self):
        # define the metrics
        health = GaugeMetricFamily(f"{self.namespace}_health", "server health")
        cpu_load = GaugeMetricFamily(
            f"{self.namespace}_cpu_load", "CPU load average",
            labels=["bucket"])
        memory_total = GaugeMetricFamily(
            f"{self.namespace}_memory_bytes_total",
            "total memory in bytes")
        memory_used = GaugeMetricFamily(
            f"{self.namespace}_memory_bytes_used",
            "used memory in bytes")

        # talk to the server and populate the metrics
        resp = requests.get(url=f"{SERV_URL}/healthz")
        if resp.status_code != 200:
            health.add_metric(labels=[], value=0)
            yield health

            cpu_load.add_metric(labels=["1m"], value=0)
            cpu_load.add_metric(labels=["5m"], value=0)
            cpu_load.add_metric(labels=["15m"], value=0)
            yield cpu_load

            memory_total.add_metric(labels=[], value=0)
            yield memory_total

            memory_used.add_metric(labels=[], value=0)
            yield memory_used
        else:
            health.add_metric(labels=[], value=1)
            yield health

            metrics_resp = requests.get(url=f"{SERV_URL}/stats")
            if metrics_resp.status_code != 200:
                cpu_load.add_metric(labels=["1m"], value=0)
                cpu_load.add_metric(labels=["5m"], value=0)
                cpu_load.add_metric(labels=["15m"], value=0)
                yield cpu_load

                memory_total.add_metric(labels=[], value=0)
                yield memory_total

                memory_used.add_metric(labels=[], value=0)
                yield memory_used
            else:
                metrics = metrics_resp.json()

                cpu_load.add_metric(labels=["1m"], value=metrics["cpu"]["load_1m"])
                cpu_load.add_metric(labels=["5m"], value=metrics["cpu"]["load_5m"])
                cpu_load.add_metric(labels=["15m"], value=metrics["cpu"]["load_15m"])
                yield cpu_load

                memory_total.add_metric(labels=[], value=metrics["memory"]["total_bytes"])
                yield memory_total

                memory_used.add_metric(labels=[], value=metrics["memory"]["used_bytes"])
                yield memory_used


if __name__ == "__main__":
    start_http_server(EXP_PORT)
    REGISTRY.register(CustomCollector())
    while True:
        time.sleep(1)
