# prometheus-http-api-rs 0.2.0

A simple library to pull data from prometheus using its API

[![Rust](https://github.com/sebosp/prometheus-http-api-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/sebosp/prometheus-http-api-rs/actions/workflows/rust.yml)
[![Crates.io](https://img.shields.io/crates/v/prometheus-http-api.svg)](https://crates.io/crates/prometheus-http-api)
[![Documentation](https://docs.rs/prometheus-http-api/badge.svg)][dox]

Upcoming docs [crate documentation][dox].

[dox]: https://docs.rs/prometheus-http-api

## Usage

Add dependency in `Cargo.toml`:

```toml
[dependencies]
prometheus-http-api = "0.2.0"
```

Use `prometheus_http_api`

```rust
use prometheus_http_api::*;

#[tokio::main]
async fn main() {
    let query = Query::Instant(InstantQuery::new("up"));
    let request = DataSourceBuilder::new("localhost:9090")
        .with_query(query)
        .build()
        .unwrap();
    let res_json = request.get().await;
    tracing::info!("{:?}", res_json);
}
```

# License

- Apache License, Version 2.0 ([LICENSE](LICENSE) or http://apache.org/licenses/LICENSE-2.0)

## TODO

### To be addressed soon

- Missing fallback to POST, some queries can grow paths the GET params limits.
- Missing `Instant` support, currently we only support epochs as params.
- Querying metadata: `/api/v1/series` or `/api/v1/labels` `/api/v1/label/<label_name>/values`
- Target state `/api/v1/targets`
- AlertManagers `/api/v1/alertmanagers`
- Status Config `/api/v1/status/config`
- Status flags `/api/v1/status/flags`
- Runtime info `/api/v1/status/runtimeinfo`, available since prometheus v2.2
- Build info `/api/v1/status/buildinfo`, available since prometheus v2.14
- TSDB metrics `/api/v1/status/tsdb`, available since prometheus v2.14
- WAL Replay status `/api/v1/status/walreplay` available since prometheus v2.15

### Not stable endpoints:

- Querying exemplars `/api/v1/query_exemplars` (experimental)
- Rules `/api/v1/rules` (doesn't have stability guarantees from prometheus v1)
- Alerts `/api/v1/alerts` (doesn't have stability guarantees from prometheus v1)
- Target Metadata `/api/v1/targets/metadata` (experimental)
- Metric Metadata `/api/v1/metadata`(experimental)

### Admin endpoints

Since prometheus v2.18, require `-web.enable-admin-api`

- Snapshots `/api/v1/admin/tsdb/snapshot`
- Delete timeseries `/api/v1/admin/tsdb/delete_series`
- Clean tombstones `/api/v1/admin/tsdb/clean_tombstones`

### Write endpoint

Requires `--web.enable-remote-write-receiver`.

- Prometheus remote write protocol `/api/v1/write` since prometheus v2.33 not efficient way to push data.

## NOTES:
- The `json::Value` is not necessarily a number, it may be a string:
> JSON does not support special float values such as NaN, Inf, and -Inf, so sample values
> are transferred as quoted JSON strings rather than raw numbers.

