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
use hyper::client::connect::HttpConnector;
use hyper::client::Client;
use hyper_tls::HttpsConnector;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
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
    timeout: Option<u64>,
}

impl PrometheusInstantQuery {
    /// Initializes an Instant query with optional fields set to None
    pub fn new(query: String) -> Self {
        Self {
            query,
            time: None,
            timeout: None,
        }
    }

    /// Builder method to set the query timeout
    pub fn with_epoch(mut self, time: u64) -> Self {
        self.time = Some(time);
        self
    }

    /// Builder method to set the query timeout
    pub fn with_timeout(mut self, timeout: u64) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Transforms the typed query into HTTP GET query params
    pub fn as_query_params(&self) -> String {
        let mut res = String::from(format!("query={}", self.query));
        if let Some(time) = self.time {
            res.push_str(&format!("&time={}", time));
        }
        if let Some(timeout) = self.timeout {
            res.push_str(&format!("&timeout={}", timeout));
        }
        res
    }
}

#[derive(Debug)]
pub struct PrometheusRangeQuery {
    /// Prometheus expression query string.
    pub query: String,
    /// Start timestamp, inclusive.
    pub start: u64,
    /// End timestamp, inclusive.
    pub end: u64,
    /// Query resolution step width in duration format or float number of seconds.
    pub step: f64,
    /// Evaluation timeout. Optional. Defaults to and is capped by the value of the -query.timeout flag.1
    pub timeout: Option<u64>,
}

impl PrometheusRangeQuery {
    /// Initializes a Range query with optional fields set to None
    pub fn new(query: String, start: u64, end: u64, step: f64) -> Self {
        Self {
            query,
            start,
            end,
            step,
            timeout: None,
        }
    }

    /// Builder method to set the query timeout
    pub fn with_timeout(mut self, timeout: u64) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Transforms the typed query into HTTP GET query params
    pub fn as_query_params(&self) -> String {
        let mut res = String::from(format!(
            "query={}&start={}&end={}&step={}",
            self.query, self.start, self.end, self.step
        ));
        if let Some(timeout) = self.timeout {
            res.push_str(&format!("&timeout={}", timeout));
        }
        res
    }
}

#[derive(Debug)]
pub enum PrometheusQuery {
    /// Evaluates an instant query at a single point in time
    Instant(PrometheusInstantQuery),
    /// Evaluates an expression query over a range of time
    Range(PrometheusRangeQuery),
}

impl PrometheusQuery {
    ///  Builds a query of type `Self::Instant`
    pub fn instant(query: PrometheusInstantQuery) -> Self {
        PrometheusQuery::Instant(query)
    }

    ///  Builds a query of type `Self::Range`
    pub fn range(query: PrometheusRangeQuery) -> Self {
        PrometheusQuery::Range(query)
    }

    /// Transforms the typed query into HTTP GET query params
    pub fn as_query_params(&self) -> String {
        match self {
            Self::Instant(query) => query.as_query_params(),
            Self::Range(query) => query.as_query_params(),
        }
    }

    /// Returns the timeout of the prometheus query
    pub fn get_timeout(&self) -> Option<u64> {
        match self {
            Self::Instant(query) => query.timeout,
            Self::Range(query) => query.timeout,
        }
    }
}

#[derive(Error, Debug)]
pub enum PrometheusDataSourceError {
    #[error("http error: {0}")]
    Http(#[from] http::Error),
    #[error("hyper error: {0}")]
    Hyper(#[from] hyper::Error),
    #[error("Serde Error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Missing query type")]
    MissingQueryParam,
}

/// Represents a prometheus data source
#[derive(Debug)]
pub struct PrometheusDataSource {
    /// This should contain the scheme://<authority>/ portion of the URL, the params would be
    /// appended later.
    pub authority: String,

    /// Optionally specify if http/https is used. By default 'http'
    pub scheme: String,

    /// The prefix to reach prometheus on the authority, for example, prometheus may share a
    /// host:port with grafana, etc, and prometheus would be reached by <authority>/prom/
    pub prefix: Option<String>,

    /// The query to send to prometheus
    pub query: PrometheusQuery,

    /// Sets the timeout for the HTTP connection to the prometheus server
    pub http_timeout: Option<Duration>,
}

#[derive(Debug)]
pub struct PrometheusDataSourceBuilder {
    /// Allows setting the http://<authority>/ portion of the URL, the query param may be a
    /// host:port or user:password@host:port or dns/fqdn
    pub authority: String,

    /// Allows setting the <scheme>://authority/ portion of the URL, currently tested with http and
    /// https by using hyper_tls
    pub scheme: Option<String>,

    /// Allows setting the scheme://authority/<prefix>/api/v1/ portion of the URL, useful when
    /// prometheus shares the same `authority` as other components and the api/v1/query should be
    /// prefixed with a specific route.
    pub prefix: Option<String>,

    /// Sets the query parameter
    pub query: Option<PrometheusQuery>,

    /// Sets the timeout for the HTTP connection to the prometheus server
    pub http_timeout: Option<Duration>,
}

impl PrometheusDataSourceBuilder {
    pub fn new(authority: String) -> Self {
        Self {
            authority,
            scheme: None,
            prefix: None,
            query: None,
            http_timeout: None,
        }
    }

    /// Sets the prefix that hosts prometheus, useful when prometheus is behind a shared reverse
    /// proxy
    pub fn with_prefix(mut self, prefix: String) -> Self {
        self.prefix = Some(prefix);
        self
    }

    /// Sets the prometheus query param.
    pub fn with_query(mut self, query: PrometheusQuery) -> Self {
        self.query = Some(query);
        self
    }

    /// Sets the URL scheme, be it http or https
    pub fn with_scheme(mut self, scheme: String) -> Self {
        self.scheme = Some(scheme);
        self
    }

    /// Builds into PrometheusDataSource after checking and merging fields
    pub fn build(self) -> Result<PrometheusDataSource, PrometheusDataSourceError> {
        let query = match self.query {
            Some(query) => query,
            None => {
                tracing::error!("Missing query field in builder");
                return Err(PrometheusDataSourceError::MissingQueryParam);
            }
        };
        if let Some(http_timeout) = self.http_timeout {
            if let Some(query_timeout) = query.get_timeout() {
                if query_timeout > http_timeout.as_secs() {
                    tracing::warn!("Configured query_timeout is longer than http_timeout. Prometheus query will be dropped by the http client if the query exceeds http_timeout");
                }
            }
        }
        let scheme = match self.scheme {
            Some(val) => val,
            None => String::from("http"),
        };
        Ok(PrometheusDataSource {
            authority: self.authority,
            scheme,
            prefix: self.prefix,
            query,
            http_timeout: self.http_timeout,
        })
    }
}

impl PrometheusDataSource {
    /// `prepare_url` builds a hyper::Uri with the query and time boundaries
    pub fn build_url(&self) -> Result<hyper::Uri, PrometheusDataSourceError> {
        tracing::trace!("build_url()");
        let url_builder = http::uri::Builder::new()
            .scheme(self.scheme.as_str())
            .authority(self.authority.clone());
        let query_params = self.query.as_query_params();
        tracing::trace!("build_url: raw query_params: {}", query_params);
        let encoded_query_params = utf8_percent_encode(&query_params, NON_ALPHANUMERIC).to_string();
        tracing::trace!("build_url: encoded query_params: {}", encoded_query_params);
        Ok(url_builder.path_and_query(encoded_query_params).build()?)
    }

    /// `get` is an async operation that returns potentially a PrometheusResponse
    pub async fn get(&self) -> Result<PrometheusResponse, PrometheusDataSourceError> {
        let url = self.build_url()?;
        tracing::debug!("get() init Prometheus URL: {}", url);
        let mut client = Client::builder();
        if let Some(timeout) = self.http_timeout {
            client.pool_idle_timeout(timeout);
        }
        let request = if url.scheme() == Some(&hyper::http::uri::Scheme::HTTP) {
            tracing::info!("get: Prometheus URL: {}", url);
            client
                .build::<_, hyper::Body>(HttpConnector::new())
                .get(url.clone())
        } else {
            client
                .build::<_, hyper::Body>(HttpsConnector::new())
                .get(url.clone())
        };
        let response_body = match request.await {
            Ok(res) => hyper::body::to_bytes(res.into_body()).await?,
            Err(err) => {
                tracing::info!("get: Error loading '{:?}': '{:?}'", url, err);
                return Err(err.into());
            }
        };
        tracing::debug!("get() done");
        tracing::trace!("Deserializing: {:?}", response_body);
        Ok(serde_json::from_slice(&response_body)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn init_log() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[test]
    fn it_detects_prometheus_errors() {
        init_log();
        let test0_json = hyper::body::Bytes::from(
            r#"
            {
              "status": "error",
              "errorType": "bad_data",
              "error": "end timestamp must not be before start time"
            }
            "#,
        );
        let res0_json: Result<PrometheusResponse, serde_json::Error> =
            serde_json::from_slice(&test0_json);
        assert_eq!(res0_json.is_err(), true);
        let test1_json = hyper::body::Bytes::from("Internal Server Error");
        let res1_json: Result<PrometheusResponse, serde_json::Error> =
            serde_json::from_slice(&test1_json);
        assert_eq!(res1_json.is_err(), true);
    }

    #[test]
    fn it_loads_prometheus_scalars() {
        init_log();
        // A json returned by prometheus
        let test0_json = hyper::body::Bytes::from(
            r#"
            { "status":"success",
              "data":{
                "resultType":"scalar",
                "result":[1558283674.829,"1"]
              }
            }"#,
        );
        let res_json: Result<PrometheusResponse, serde_json::Error> =
            serde_json::from_slice(&test0_json);
        assert_eq!(res_json.is_ok(), true);
        // This json is missing the value after the epoch
        let test1_json = hyper::body::Bytes::from(
            r#"
            { "status":"success",
              "data":{
                "resultType":"scalar",
                "result":[1558283674.829]
              }
            }"#,
        );
        let res_json: Result<PrometheusResponse, serde_json::Error> =
            serde_json::from_slice(&test1_json);
        assert_eq!(res_json.is_ok(), true);
    }

    #[test]
    fn it_loads_prometheus_matrix() {
        init_log();
        // A json returned by prometheus
        let test0_json = hyper::body::Bytes::from(
            r#"
            {
              "status": "success",
              "data": {
                "resultType": "matrix",
                "result": [
                  {
                    "metric": {
                      "__name__": "node_load1",
                      "instance": "localhost:9100",
                      "job": "node_exporter"
                    },
                    "values": [
                        [1558253469,"1.69"],[1558253470,"1.70"],[1558253471,"1.71"],
                        [1558253472,"1.72"],[1558253473,"1.73"],[1558253474,"1.74"],
                        [1558253475,"1.75"],[1558253476,"1.76"],[1558253477,"1.77"],
                        [1558253478,"1.78"],[1558253479,"1.79"]]
                  }
                ]
              }
            }"#,
        );
        let res_json: Result<PrometheusResponse, serde_json::Error> =
            serde_json::from_slice(&test0_json);
        assert_eq!(res_json.is_ok(), true);
        // This json is missing the value after the epoch
        let test2_json = hyper::body::Bytes::from(
            r#"
            {
              "status": "success",
              "data": {
                "resultType": "matrix",
                "result": [
                  {
                    "metric": {
                      "__name__": "node_load1",
                      "instance": "localhost:9100",
                      "job": "node_exporter"
                    },
                    "values": [
                        [1558253478]
                    ]
                  }
                ]
              }
            }"#,
        );
        let res_json: Result<PrometheusResponse, serde_json::Error> =
            serde_json::from_slice(&test2_json);
        assert_eq!(res_json.is_ok(), true);
    }
    #[test]
    fn it_loads_prometheus_vector() {
        init_log();
        // A json returned by prometheus
        let test0_json = hyper::body::Bytes::from(
            r#"
            {
              "status": "success",
              "data": {
                "resultType": "vector",
                "result": [
                  {
                    "metric": {
                      "__name__": "up",
                      "instance": "localhost:9090",
                      "job": "prometheus"
                    },
                    "value": [
                      1557571137.732,
                      "1"
                    ]
                  },
                  {
                    "metric": {
                      "__name__": "up",
                      "instance": "localhost:9100",
                      "job": "node_exporter"
                    },
                    "value": [
                      1557571138.732,
                      "1"
                    ]
                  }
                ]
              }
            }"#,
        );
        let res_json: Result<PrometheusResponse, serde_json::Error> =
            serde_json::from_slice(&test0_json);
        assert_eq!(res_json.is_ok(), true);
    }
}
