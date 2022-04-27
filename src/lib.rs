//! Prometheus HTTP API
//!
//! This crate provides data structures to interact with the prometheus HTTP API endpoints. The
//! crate allows constructing prometheus data sources with [`DataSourceBuilder`].
//!
//! A [`Query`] must be provided to the prometheus data source via the
//! [`DataSourceBuilder::with_query()`] that acceps either a
//! [`InstantQuery`] or a [`RangeQuery`], for these types, build-like methods
//! are provided for the optional parameters.
//!
//! ## Simple Usage
//!
//! To gather the data from `<http://localhost:9090/api/v1/query?query=up>`
//!
//! ```
//! use prometheus_http_api::{
//!     DataSourceBuilder, InstantQuery, Query,
//! };
//!
//! #[tokio::main]
//! async fn main() {
//!     let query = Query::Instant(InstantQuery::new("up"));
//!     let request = DataSourceBuilder::new("localhost:9090")
//!         .with_query(query)
//!         .build()
//!         .unwrap();
//!     let res_json = request.get().await;
//!     tracing::info!("{:?}", res_json);
//! }
//! ```

#![warn(rust_2018_idioms)]
#![warn(missing_docs)]
#![warn(rustdoc::missing_doc_code_examples)]
use hyper::client::connect::HttpConnector;
use hyper::client::Client;
use hyper_tls::HttpsConnector;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use thiserror::Error;

/// [`MatrixResult`] contains Prometheus Range Vectors
/// ```
/// let matrix_raw_response = hyper::body::Bytes::from(r#"
///   {
///     "metric": {
///       "__name__": "node_load1",
///       "instance": "localhost:9100",
///       "job": "node_exporter"
///     },
///     "values": [
///       [1558253469,"1.69"],[1558253470,"1.70"],[1558253471,"1.71"]
///     ]
///   }"#);
/// let res_json: Result<prometheus_http_api::MatrixResult, serde_json::Error> = serde_json::from_slice(&matrix_raw_response);
/// assert!(res_json.is_ok());
/// ```
#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Clone)]
pub struct MatrixResult {
    /// A series of labels for the matrix results. This is a HashMap of `{"label_name_1":
    /// "value_1", ...}`
    #[serde(rename = "metric")]
    pub labels: HashMap<String, String>,
    /// The values over time captured on prometheus, generally `[[<epoch>, "<value>"]]`
    pub values: Vec<Vec<serde_json::Value>>,
}

/// `VectorResult` contains Prometheus Instant Vectors
/// ```
/// let vector_raw_response = hyper::body::Bytes::from(r#"
///  {
///    "metric": {
///      "__name__": "up",
///      "instance": "localhost:9090",
///      "job": "prometheus"
///    },
///    "value": [
///      1557571137.732,
///      "1"
///    ]
///   }"#);
/// let res_json: Result<prometheus_http_api::VectorResult, serde_json::Error> = serde_json::from_slice(&vector_raw_response);
/// assert!(res_json.is_ok());
/// ```
#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Clone)]
pub struct VectorResult {
    /// A series of labels for the matrix results. This is a HashMap of `{"label_name_1":
    /// "value_1", ...}`
    #[serde(rename = "metric")]
    pub labels: HashMap<String, String>,
    /// The values over time captured on prometheus, generally `[[<epoch>, "<value>"]]`
    pub value: Vec<serde_json::Value>,
}

/// Available [`ResponseData`] formats documentation:
/// `https://prometheus.io/docs/prometheus/latest/querying/api/#expression-query-result-formats`
/// ```
/// let scalar_result_type = hyper::body::Bytes::from(r#"{
///   "resultType":"scalar",
///   "result":[1558283674.829,"1"]
///  }"#);
/// let res_json: Result<prometheus_http_api::ResponseData, serde_json::Error> = serde_json::from_slice(&scalar_result_type);
/// assert!(res_json.is_ok());
/// ```
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(tag = "resultType")]
pub enum ResponseData {
    /// Handles a Response of type [`VectorResult`]
    #[serde(rename = "vector")]
    Vector {
        /// The result vector.
        result: Vec<VectorResult>,
    },
    /// Handles a Response of type [`MatrixResult`]
    #[serde(rename = "matrix")]
    Matrix {
        /// The result vector.
        result: Vec<MatrixResult>,
    },
    /// Handles a scalar result, generally numeric
    #[serde(rename = "scalar")]
    Scalar {
        /// The result vector.
        result: Vec<serde_json::Value>,
    },
    /// Handles a String result, for example for label names
    #[serde(rename = "string")]
    String {
        /// The result vector.
        result: Vec<serde_json::Value>,
    },
}

impl Default for ResponseData {
    fn default() -> Self {
        Self::Vector {
            result: vec![VectorResult::default()],
        }
    }
}

/// A Prometheus [`Response`] returned by an HTTP query.
/// ```
/// let full_response = hyper::body::Bytes::from(
///    r#"
///    { "status":"success",
///      "data":{
///        "resultType":"scalar",
///        "result":[1558283674.829,"1"]
///      }
///    }"#,
///  );
/// let res_json: Result<prometheus_http_api::Response, serde_json::Error> = serde_json::from_slice(&full_response);
/// assert!(res_json.is_ok());
/// ```
#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Clone)]
pub struct Response {
    /// A Data response, it may be vector, matrix, scalar or String
    pub data: ResponseData,
    /// An status string, either `"success"` or `"error"`
    pub status: String,
}

/// An instant query to send to Prometheus
#[derive(Debug)]
pub struct InstantQuery {
    ///  expression query string.
    query: String,
    /// Evaluation timestamp. Optional.
    time: Option<u64>,
    /// Evaluation timeout. Optional. Defaults to and is capped by the value of the -query.timeout flag.
    timeout: Option<u64>,
}

impl InstantQuery {
    /// Initializes an Instant query with optional fields set to None
    pub fn new(query: &str) -> Self {
        Self {
            query: query.to_string(),
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

    /// Transforms the typed query into HTTP GET query params, it contains a pre-built `base` that
    /// may use an HTTP path  prefix if configured.
    pub fn as_query_params(&self, mut base: String) -> String {
        tracing::trace!("InstantQuery::as_query_params raw query: {}", self.query);
        let encoded_query = utf8_percent_encode(&self.query, NON_ALPHANUMERIC).to_string();
        tracing::trace!(
            "InstantQuery::as_query_params encoded_query: {}",
            encoded_query
        );
        base.push_str(&format!("api/v1/query?query={}", encoded_query));
        if let Some(time) = self.time {
            base.push_str(&format!("&time={}", time));
        }
        if let Some(timeout) = self.timeout {
            base.push_str(&format!("&timeout={}", timeout));
        }
        base
    }
}

/// A range query to send to Prometheus
#[derive(Debug)]
pub struct RangeQuery {
    ///  expression query string.
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

impl RangeQuery {
    /// Initializes a Range query with optional fields set to None
    pub fn new(query: &str, start: u64, end: u64, step: f64) -> Self {
        Self {
            query: query.to_string(),
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

    /// Transforms the typed query into HTTP GET query params, it contains a pre-built `base` that
    /// may use an HTTP path  prefix if configured.
    pub fn as_query_params(&self, mut base: String) -> String {
        tracing::trace!("RangeQuery::as_query_params: raw query: {}", self.query);
        let encoded_query = utf8_percent_encode(&self.query, NON_ALPHANUMERIC).to_string();
        tracing::trace!(
            "RangeQuery::as_query_params encoded_query: {}",
            encoded_query
        );
        base.push_str(&format!(
            "api/v1/query_range?query={}&start={}&end={}&step={}",
            encoded_query, self.start, self.end, self.step
        ));
        if let Some(timeout) = self.timeout {
            base.push_str(&format!("&timeout={}", timeout));
        }
        base
    }
}

/// A query to the Prometheus HTTP API
#[derive(Debug)]
pub enum Query {
    /// Represents an instant query at a single point in time
    Instant(InstantQuery),
    /// Represents an expression query over a range of time
    Range(RangeQuery),
}

impl Query {
    /// Transforms the typed query into HTTP GET query params
    pub fn as_query_params(&self, prefix: Option<String>) -> String {
        let mut base = if let Some(prefix) = prefix {
            prefix
        } else {
            String::from("/")
        };
        if !base.ends_with("/") {
            base.push_str("/");
        }
        match self {
            Self::Instant(query) => query.as_query_params(base),
            Self::Range(query) => query.as_query_params(base),
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

/// A simple Error type to understand different errors.
#[derive(Error, Debug)]
pub enum DataSourceError {
    /// The DataSource request may fail due to an http module request, could happen while
    /// interacting with the HTTP server.
    #[error("http error: {0}")]
    Http(#[from] http::Error),
    /// The DataSource request building may fail due to invalid schemes, authority, etc.
    #[error("hyper error: {0}")]
    Hyper(#[from] hyper::Error),
    /// The DataSource request may fail due to invalid data returned from the server.
    #[error("Serde Error: {0}")]
    Serde(#[from] serde_json::Error),
    /// The DataSource building process may not specific a query, this is a required field.
    #[error("Missing query type")]
    MissingQueryParam,
}

/// Represents a prometheus data source that works over an http(s) host:port endpoint potentially
/// behind a /prometheus_prefix/
#[derive(Debug)]
pub struct DataSource {
    /// This should contain the scheme://<authority>/ portion of the URL, the params would be
    /// appended later.
    pub authority: String,

    /// Optionally specify if http/https is used. By default 'http'
    pub scheme: String,

    /// The prefix to reach prometheus on the authority, for example, prometheus may share a
    /// host:port with grafana, etc, and prometheus would be reached by <authority>/prom/
    pub prefix: Option<String>,

    /// The query to send to prometheus
    pub query: Query,

    /// Sets the timeout for the HTTP connection to the prometheus server
    pub http_timeout: Option<Duration>,
}

/// A Builder struct to create the [`DataSource`]
#[derive(Debug)]
pub struct DataSourceBuilder {
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
    pub query: Option<Query>,

    /// Sets the timeout for the HTTP connection to the prometheus server
    pub http_timeout: Option<Duration>,
}

impl DataSourceBuilder {
    /// Initializes the builder for the DataSource, required param is the authority, may contain
    /// `user:password@host:port`, or `host:port`
    pub fn new(authority: &str) -> Self {
        Self {
            authority: authority.to_string(),
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
    pub fn with_query(mut self, query: Query) -> Self {
        self.query = Some(query);
        self
    }

    /// Sets the URL scheme, be it http or https
    pub fn with_scheme(mut self, scheme: String) -> Self {
        self.scheme = Some(scheme);
        self
    }

    /// Builds into DataSource after checking and merging fields
    pub fn build(self) -> Result<DataSource, DataSourceError> {
        let query = match self.query {
            Some(query) => query,
            None => {
                tracing::error!("Missing query field in builder");
                return Err(DataSourceError::MissingQueryParam);
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
        Ok(DataSource {
            authority: self.authority,
            scheme,
            prefix: self.prefix,
            query,
            http_timeout: self.http_timeout,
        })
    }
}

impl DataSource {
    /// `get` is an async operation that returns potentially a Response
    pub async fn get(&self) -> Result<Response, DataSourceError> {
        let url = http::uri::Builder::new()
            .authority(self.authority.clone())
            .scheme(self.scheme.as_str())
            .path_and_query(self.query.as_query_params(self.prefix.clone()))
            .build()?;
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
        let res0_json: Result<Response, serde_json::Error> = serde_json::from_slice(&test0_json);
        assert!(res0_json.is_err());
        let test1_json = hyper::body::Bytes::from("Internal Server Error");
        let res1_json: Result<Response, serde_json::Error> = serde_json::from_slice(&test1_json);
        assert!(res1_json.is_err());
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
        let res_json: Result<Response, serde_json::Error> = serde_json::from_slice(&test0_json);
        assert!(res_json.is_ok());
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
        let res_json: Result<Response, serde_json::Error> = serde_json::from_slice(&test1_json);
        assert!(res_json.is_ok());
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
        let res_json: Result<Response, serde_json::Error> = serde_json::from_slice(&test0_json);
        assert!(res_json.is_ok());
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
        let res_json: Result<Response, serde_json::Error> = serde_json::from_slice(&test2_json);
        assert!(res_json.is_ok());
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
        let res_json: Result<Response, serde_json::Error> = serde_json::from_slice(&test0_json);
        assert!(res_json.is_ok());
    }

    #[tokio::test]
    #[ignore]
    async fn it_loads_prometheus() {
        let query = Query::Instant(InstantQuery::new("up"));
        let request = DataSourceBuilder::new("localhost:9090")
            .with_query(query)
            .build()
            .unwrap();
        let res_json = request.get().await;
        tracing::error!("{:?}", res_json);
    }
}
