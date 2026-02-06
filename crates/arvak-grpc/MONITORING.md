# Arvak gRPC Monitoring Guide

This guide explains how to set up and use monitoring for the Arvak gRPC service.

## Overview

The Arvak gRPC service provides comprehensive observability through:

- **Prometheus Metrics**: 9 key metrics tracking job lifecycle, performance, and capacity
- **Health Endpoints**: HTTP endpoints for liveness and readiness checks
- **OpenTelemetry Tracing**: Distributed tracing for request correlation
- **Structured Logging**: JSON logs for production, console logs for development

## Quick Start

### 1. Start the Arvak gRPC Server

The service exposes two ports:
- **50051**: gRPC service
- **9090**: HTTP health/metrics endpoints

```bash
cd crates/arvak-grpc
cargo run --bin arvak-grpc-server
```

Or run the health/metrics example:

```bash
cargo run --example health_metrics
```

### 2. Start Monitoring Stack

Launch Prometheus and Grafana with Docker Compose:

```bash
docker-compose -f docker-compose.monitoring.yml up -d
```

This starts:
- **Prometheus** on http://localhost:9091
- **Grafana** on http://localhost:3000 (login: admin/admin)

### 3. Access the Dashboard

1. Open Grafana at http://localhost:3000
2. Login with username `admin` and password `admin`
3. Navigate to Dashboards → Browse
4. Open "Arvak gRPC Service" dashboard

## Available Metrics

### Job Metrics

| Metric | Type | Description | Labels |
|--------|------|-------------|--------|
| `arvak_jobs_submitted_total` | Counter | Total jobs submitted | `backend_id` |
| `arvak_jobs_completed_total` | Counter | Total jobs completed | `backend_id` |
| `arvak_jobs_failed_total` | Counter | Total jobs failed | `backend_id`, `error_type` |
| `arvak_jobs_cancelled_total` | Counter | Total jobs cancelled | `backend_id` |

### Performance Metrics

| Metric | Type | Description | Labels |
|--------|------|-------------|--------|
| `arvak_job_duration_milliseconds` | Histogram | Job execution time | `backend_id` |
| `arvak_job_queue_time_milliseconds` | Histogram | Time in queue | `backend_id` |
| `arvak_rpc_duration_milliseconds` | Histogram | RPC request duration | `method` |

### Capacity Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `arvak_active_jobs` | Gauge | Currently running jobs |
| `arvak_queued_jobs` | Gauge | Jobs waiting in queue |
| `arvak_backend_available` | Gauge | Backend availability (1=up, 0=down) |

## Health Check Endpoints

### Liveness Check

**Endpoint**: `GET http://localhost:9090/health`

Returns basic service health information:

```bash
curl http://localhost:9090/health
```

Response:
```json
{
  "status": "healthy",
  "version": "1.1.1",
  "uptime_seconds": 42
}
```

### Readiness Check

**Endpoint**: `GET http://localhost:9090/health/ready`

Checks if service is ready to accept traffic (backends available):

```bash
curl http://localhost:9090/health/ready
```

Response:
```json
{
  "ready": true,
  "backends": [
    {
      "backend_id": "simulator",
      "available": true
    }
  ],
  "active_jobs": 5,
  "queued_jobs": 10
}
```

### Metrics Endpoint

**Endpoint**: `GET http://localhost:9090/metrics`

Returns Prometheus metrics in text format:

```bash
curl http://localhost:9090/metrics
```

## Distributed Tracing

### Configuration

Set up OpenTelemetry export via environment variables:

```bash
export RUST_LOG=info
export ARVAK_LOG_FORMAT=json
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
```

### Jaeger Integration

To send traces to Jaeger:

1. Start Jaeger:
```bash
docker run -d --name jaeger \
  -p 6831:6831/udp \
  -p 16686:16686 \
  -p 4317:4317 \
  jaegertracing/all-in-one:latest
```

2. Configure Arvak to export to Jaeger:
```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
cargo run --bin arvak-grpc-server
```

3. View traces at http://localhost:16686

## Grafana Dashboard

The provided dashboard includes:

1. **Job Submission Rate**: Jobs submitted per second by backend
2. **Active & Queued Jobs**: Current capacity usage
3. **Job Completion Rate**: Jobs completed per second
4. **Job Failure Rate**: Failures per second with error types
5. **Job Success Rate**: Percentage of successful jobs
6. **Job Duration (P95)**: 95th percentile execution time
7. **Queue Time (P95)**: 95th percentile queue wait time
8. **RPC Duration by Method**: RPC performance breakdown
9. **Backend Availability**: Current backend status

## Alerting

### Recommended Alerts

Add these alerts to Prometheus `prometheus.yml`:

```yaml
rule_files:
  - 'alerts.yml'

alerting:
  alertmanagers:
    - static_configs:
        - targets: ['alertmanager:9093']
```

Create `alerts.yml`:

```yaml
groups:
  - name: arvak_grpc
    interval: 30s
    rules:
      - alert: HighJobFailureRate
        expr: rate(arvak_jobs_failed_total[5m]) > 0.1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High job failure rate"
          description: "Job failure rate is {{ $value }} failures/sec"

      - alert: BackendDown
        expr: arvak_backend_available == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Backend unavailable"
          description: "Backend {{ $labels.backend_id }} is down"

      - alert: HighQueueDepth
        expr: arvak_queued_jobs > 1000
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High queue depth"
          description: "{{ $value }} jobs queued"
```

## Production Deployment

### Kubernetes

For Kubernetes deployments, use ServiceMonitor for Prometheus Operator:

```yaml
apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
metadata:
  name: arvak-grpc
  labels:
    app: arvak-grpc
spec:
  selector:
    matchLabels:
      app: arvak-grpc
  endpoints:
    - port: metrics
      path: /metrics
      interval: 30s
```

### Log Aggregation

For production, export JSON logs to a log aggregation system:

```bash
export ARVAK_LOG_FORMAT=json
cargo run --bin arvak-grpc-server 2>&1 | your-log-shipper
```

Compatible with:
- ELK Stack (Elasticsearch, Logstash, Kibana)
- Grafana Loki
- Splunk
- Datadog

## Troubleshooting

### Metrics Not Showing

1. Verify service is running: `curl http://localhost:9090/health`
2. Check metrics endpoint: `curl http://localhost:9090/metrics`
3. Verify Prometheus can scrape: http://localhost:9091/targets

### High Memory Usage

Monitor these metrics:
- `arvak_active_jobs` - Should not grow unbounded
- `arvak_queued_jobs` - Indicates backpressure

### Slow Performance

Check these metrics:
- `arvak_job_duration_milliseconds` P95 - High execution time
- `arvak_job_queue_time_milliseconds` P95 - Long queue waits
- `arvak_rpc_duration_milliseconds` - RPC bottlenecks

## Further Reading

- [Prometheus Documentation](https://prometheus.io/docs/)
- [Grafana Dashboards](https://grafana.com/docs/grafana/latest/dashboards/)
- [OpenTelemetry](https://opentelemetry.io/docs/)
