//! `Prometheus HTTP API` data structures
//! The data structures parse structures like:
//!  {
//!   "data": {
//!     "result": [
//!       {
//!         "metric": {
//!           "__name__": "up",
//!           "instance": "localhost:9090",
//!           "job": "prometheus"
//!         },
//!         "value": [
//!           1557052757.816,
//!           "1"
//!         ]
//!       },{...}
//!     ],
//!     "resultType": "vector"
//!   },
//!   "status": "success"
//! }

#![warn(rust_2018_idioms)]
#[macro_use]
extern crate serde_derive;
use hyper::client;
use hyper::client::connect::HttpConnector;
use hyper_tls::HttpsConnector;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// `PrometheusMatrixResult` contains Range Vectors, data is stored like this
/// [[Epoch1, Metric1], [Epoch2, Metric2], ...]
#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Clone)]
pub struct PrometheusMatrixResult {
    #[serde(rename = "metric")]
    pub labels: HashMap<String, String>,
    pub values: Vec<Vec<serde_json::Value>>,
}

/// `PrometheusVectorResult` contains Instant Vectors, data is stored like this
/// [Epoch1, Metric1, Epoch2, Metric2, ...]
#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Clone)]
pub struct PrometheusVectorResult {
    #[serde(rename = "metric")]
    pub labels: HashMap<String, String>,
    pub value: Vec<serde_json::Value>,
}

/// `PrometheusResponseData` may be one of these types:
/// https://prometheus.io/docs/prometheus/latest/querying/api/#expression-query-result-formats
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(tag = "resultType")]
pub enum PrometheusResponseData {
    #[serde(rename = "vector")]
    Vector { result: Vec<PrometheusVectorResult> },
    #[serde(rename = "matrix")]
    Matrix { result: Vec<PrometheusMatrixResult> },
    #[serde(rename = "scalar")]
    Scalar { result: Vec<serde_json::Value> },
    #[serde(rename = "string")]
    String { result: Vec<serde_json::Value> },
}

impl Default for PrometheusResponseData {
    fn default() -> Self {
        Self::Vector {
            result: vec![PrometheusVectorResult::default()],
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Clone)]
pub struct PrometheusResponse {
    pub data: PrometheusResponseData,
    pub status: String,
}

#[derive(Debug)]
pub struct PrometheusInstantQuery {
    /// Prometheus expression query string.
    query: String,
    /// Evaluation timestamp. Optional.
    time: Option<u64>,
    /// Evaluation timeout. Optional. Defaults to and is capped by the value of the -query.timeout flag.
    timeout: Option<u32>,
}

#[derive(Debug)]
pub struct PrometheusRangeQuery {
    /// Prometheus expression query string.
    query: String,
    /// Start timestamp, inclusive.
    start: u64,
    /// End timestamp, inclusive.
    end: u64,
    /// Query resolution step width in duration format or float number of seconds.
    step: f64,
    /// Evaluation timeout. Optional. Defaults to and is capped by the value of the -query.timeout flag.1
    timeout: Option<u32>,
}

#[derive(Debug)]
pub enum PrometheusQuery {
    /// Evaluates an instant query at a single point in time
    Instant(PrometheusInstantQuery),
    /// Evaluates an expression query over a range of time
    Range(PrometheusRangeQuery),
}

#[derive(Error, Debug)]
pub enum PrometheusDataSourceError {
    #[error("Only HTTP and HTTPS protocols are supported")]
    UnsupportedProtocol(String),
    #[error("Unable to parse url: {0}")]
    InvalidUrl(String),
    #[error("Missing query type")]
    MissingQueryParam,
}

/// Represents a prometheus data source
#[derive(Debug)]
pub struct PrometheusDataSource {
    /// The host:port to target prometheus.
    pub host_port: String,

    /// The prefix to reach prometheus on the host_port, for example, prometheus may share a
    /// host:port with grafana, etc, and prometheus would be reached by host_port/prom/
    pub prefix: Option<String>,

    /// The query to send to prometheus
    pub query: PrometheusQuery,
}

#[derive(Debug)]
pub struct PrometheusDataSourceBuilder {
    pub host_port: String,
    pub prefix: Option<String>,
    pub query: Option<PrometheusQuery>,
}

impl PrometheusDataSourceBuilder {
    pub fn new(host_port: String) -> Self {
        Self {
            host_port,
            prefix: None,
            query: None,
        }
    }

    pub fn with_prefix(mut self, prefix: String) -> Self {
        self.prefix = Some(prefix);
        self
    }

    pub fn with_query(mut self, query: PrometheusQuery) -> Self {
        self.query = Some(query);
        self
    }

    pub fn build(self) -> Result<PrometheusDataSource, PrometheusDataSourceError> {
        let query = match self.query {
            Some(query) => query,
            None => {
                tracing::error!("Missing query field in builder");
                return Err(PrometheusDataSourceError::MissingQueryParam);
            }
        };
        Ok(PrometheusDataSource {
            host_port: self.host_port,
            prefix: self.prefix,
            query,
        })
    }

