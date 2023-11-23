use lazy_static::lazy_static;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::io::{prelude::*, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;

use prometheus_client::encoding::text::encode;
use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::registry::Registry;

const SERVICE_PORT: i32 = 8443;

const UNSUPPORTED_RESPONSE: &str = "HTTP/1.1 405 Method Not Allowed\r\n\r\n";
const NOT_FOUND_RESPONSE: &str = "HTTP/1.1 404 Not Found\r\n\r\n";
const BAD_REQUEST_RESPONSE: &str = "HTTP/1.1 400 Bad Request\r\n\r\n";
const OK_RESPONSE_LINE: &str = "HTTP/1.1 200 Ok";

const TOTAL_BYTES: u64 = 4294967296; // 4GB
const CORE_COUNT: u32 = 8;

const PROM_NAMESPACE: &str = "my_server_instr";

#[derive(Serialize, Deserialize)]
struct MetricsRoot {
    cpu: MetricsCpu,
    memory: MetricsMem,
}

#[derive(Serialize, Deserialize)]
struct MetricsCpu {
    load_1m: f64,
    load_5m: f64,
    load_15m: f64,
    thread_count: u32,
}

#[derive(Serialize, Deserialize)]
struct MetricsMem {
    used_bytes: u64,
    total_bytes: u64,
}

// struct has to be pub to be used in lazy_static
#[derive(Clone, Eq, Hash, PartialEq, EncodeLabelSet, Debug)]
pub struct CpuLabels {
    bucket: String,
}

// use lazy_static to create lazy init globals
lazy_static! {
    // Mutex for safe mutable access
    pub static ref PROM_REGISTRY: Mutex<Registry> = Mutex::new(<Registry>::default());
    pub static ref METRIC_HEALTH: Gauge = Gauge::default();
    // AtomicU64 for floating points, default is i64 for some reason
    pub static ref METRIC_CPU: Family<CpuLabels, Gauge::<f64, AtomicU64>> = Family::<CpuLabels, Gauge::<f64, AtomicU64>>::default();
    pub static ref METRIC_MEM_TOTAL: Gauge::<f64, AtomicU64> = Gauge::<f64, AtomicU64>::default();
    pub static ref METRIC_MEM_USED: Gauge::<f64, AtomicU64> = Gauge::<f64, AtomicU64>::default();
}

fn handle_connection(mut stream: TcpStream) {
    let buf_reader = BufReader::new(&mut stream);
    let http_request: Vec<_> = buf_reader
        .lines()
        .map(|result| result.unwrap())
        .take_while(|line| !line.is_empty())
        .collect();

    if http_request.len() == 0 {
        println!("empty request received");
        stream.write_all(BAD_REQUEST_RESPONSE.as_bytes()).unwrap();
    } else {
        let req_line = &http_request[0];
        let req_split: Vec<&str> = req_line.split(' ').collect();
        match req_split[0] {
            "GET" => match req_split[1] {
                "/healthz" => handle_healthz(stream),
                "/stats" => handle_stats(stream),
                "/metrics" => handle_metrics(stream),
                _ => stream.write_all(NOT_FOUND_RESPONSE.as_bytes()).unwrap(),
            },
            _ => stream.write_all(UNSUPPORTED_RESPONSE.as_bytes()).unwrap(),
        }
        println!("Request: {:#?}", http_request);
    }
}

fn handle_stats(mut stream: TcpStream) {
    let payload = MetricsRoot {
        cpu: gen_metrics_cpu(CORE_COUNT),
        memory: gen_metrics_mem(TOTAL_BYTES),
    };

    let payload_content = serde_json::to_string(&payload).unwrap();
    let payload_length = payload_content.len();
    let response =
        format!("{OK_RESPONSE_LINE}\r\nContent-Length: {payload_length}\r\n\r\n{payload_content}");

    stream.write_all(response.as_bytes()).unwrap();
}

fn handle_healthz(mut stream: TcpStream) {
    if gen_health_status() {
        stream
            .write_all("HTTP/1.1 200 Ok\r\n\r\n".as_bytes())
            .unwrap();
    } else {
        stream.write_all("".as_bytes()).unwrap();
    }
}

fn handle_metrics(mut stream: TcpStream) {
    populate_metrics();

    // generate openmetrics response
    let mut buffer = String::new();
    encode(&mut buffer, &PROM_REGISTRY.lock().unwrap()).unwrap();

    let payload_length = buffer.len();
    stream
        .write_all(
            format!("{OK_RESPONSE_LINE}\r\nContent-Length: {payload_length}\r\n\r\n{buffer}")
                .as_bytes(),
        )
        .unwrap();
}

fn gen_health_status() -> bool {
    // 10% chance of being unhealthy
    let mut rng = rand::thread_rng();
    rng.gen_range(0..99) >= 10
}

fn gen_metrics_mem(total_bytes: u64) -> MetricsMem {
    let mut rng = rand::thread_rng();
    // used memory stayes between mid point and full usage
    let used_bytes = rng.gen_range(total_bytes / 2..total_bytes);

    MetricsMem {
        used_bytes,
        total_bytes: TOTAL_BYTES,
    }
}

fn gen_metrics_cpu(core_count: u32) -> MetricsCpu {
    let mut rng = rand::thread_rng();
    let mut counts: Vec<f64> = Vec::new();

    // generate 15 data points for believability
    for _ in 0..15 {
        // 10% chance of load avg spiking beyond core count
        if rng.gen_range(0..99) >= 10 {
            counts.push(rng.gen_range(0.0..core_count as f64));
        } else {
            counts.push(rng.gen_range(core_count as f64..(core_count * 2) as f64));
        }
    }

    let load_1m = counts[14];
    let load_5m = counts[9..14].iter().sum();
    let load_15m = counts.iter().sum();

    MetricsCpu {
        load_1m,
        load_5m,
        load_15m,
        thread_count: core_count * 2,
    }
}

// gether values and populate registered metrics
fn populate_metrics() {
    // gather values
    if gen_health_status() {
        METRIC_HEALTH.set(1);
    } else {
        METRIC_HEALTH.set(0);
    }

    let cpu_metrics: MetricsCpu = gen_metrics_cpu(CORE_COUNT);
    METRIC_CPU
        .get_or_create(&CpuLabels {
            bucket: "1m".to_string(),
        })
        .set(cpu_metrics.load_1m);

    METRIC_CPU
        .get_or_create(&CpuLabels {
            bucket: "5m".to_string(),
        })
        .set(cpu_metrics.load_5m);

    METRIC_CPU
        .get_or_create(&CpuLabels {
            bucket: "15m".to_string(),
        })
        .set(cpu_metrics.load_15m);

    let mem_metrics: MetricsMem = gen_metrics_mem(TOTAL_BYTES);
    METRIC_MEM_USED.set(mem_metrics.used_bytes as f64);
    METRIC_MEM_TOTAL.set(mem_metrics.total_bytes as f64);
}

// register the metrics in the register to be collected when the scraping happens
fn register_prom_metrics() {
    PROM_REGISTRY.lock().unwrap().register(
        format!("{PROM_NAMESPACE}_health"),
        "server health",
        METRIC_HEALTH.clone(),
    );

    PROM_REGISTRY.lock().unwrap().register(
        format!("{PROM_NAMESPACE}_cpu_load"),
        "CPU load average",
        METRIC_CPU.clone(),
    );

    PROM_REGISTRY.lock().unwrap().register(
        format!("{PROM_NAMESPACE}_memory_bytes_total"),
        "total memory in bytes",
        METRIC_MEM_TOTAL.clone(),
    );

    PROM_REGISTRY.lock().unwrap().register(
        format!("{PROM_NAMESPACE}_memory_bytes_used"),
        "used memory in bytes",
        METRIC_MEM_USED.clone(),
    );
}

fn main() {
    register_prom_metrics();

    let listener = TcpListener::bind(format!("127.0.0.1:{SERVICE_PORT}")).unwrap();
    println!("waiting for requests on {SERVICE_PORT}");
    for stream in listener.incoming() {
        let stream = stream.unwrap();
        println!("connection established");
        handle_connection(stream);
    }
}
