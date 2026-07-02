//! PyO3 bindings — `pip install async-sctp` gives a Rust-backed **async** SCTP
//! for Python, bridging tokio ↔ asyncio via `pyo3-async-runtimes`. The same
//! kernel-SCTP core the crate ships (multihoming, per-message stream/PPID,
//! one-to-one + one-to-many, notifications) is exposed as `await`-able coroutines.
//!
//! Compiled only with `--features python`; the default crate build is pyo3-free.
#![allow(clippy::too_many_arguments)] // PyO3 send() signatures carry kwargs

use std::net::SocketAddr;
use std::sync::Arc;

use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyModule};
use pyo3_async_runtimes::tokio::future_into_py;

use crate::{
    ppid, RecvInfo, SctpAssociation, SctpConfig, SctpListener, SctpServer, SendOptions,
    ServerMessage,
};

create_exception!(async_sctp, SctpError, PyException, "SCTP error.");

fn err(e: crate::SctpError) -> PyErr {
    SctpError::new_err(e.to_string())
}

fn parse_addr(s: &str) -> PyResult<SocketAddr> {
    s.parse()
        .map_err(|_| PyValueError::new_err(format!("invalid socket address: {s}")))
}

fn parse_addrs(v: &[String]) -> PyResult<Vec<SocketAddr>> {
    v.iter().map(|s| parse_addr(s)).collect()
}

/// Bring up the pyo3-async-runtimes tokio runtime once. The worker pool is
/// capped (SCTP work is I/O-bound and the asyncio wake needs the GIL);
/// override with `ASYNC_SCTP_WORKER_THREADS`.
fn init_runtime() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let mut builder = tokio::runtime::Builder::new_multi_thread();
        builder.enable_all();
        let threads = std::env::var("ASYNC_SCTP_WORKER_THREADS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(2);
        builder.worker_threads(threads);
        pyo3_async_runtimes::tokio::init(builder);
    });
}

/// Run a synchronous constructor (which creates an `AsyncFd`) inside the runtime
/// so tokio's reactor is available.
fn in_runtime<T>(f: impl FnOnce() -> Result<T, crate::SctpError>) -> PyResult<T> {
    init_runtime();
    let _guard = pyo3_async_runtimes::tokio::get_runtime().enter();
    f().map_err(err)
}

// ── RecvInfo ────────────────────────────────────────────────────────────────
/// Metadata about a received message.
#[pyclass(
    name = "RecvInfo",
    module = "async_sctp._async_sctp",
    skip_from_py_object
)]
#[derive(Clone)]
pub struct PyRecvInfo {
    #[pyo3(get)]
    pub stream: u16,
    #[pyo3(get)]
    pub ppid: u32,
    #[pyo3(get)]
    pub assoc_id: i32,
}

impl From<RecvInfo> for PyRecvInfo {
    fn from(i: RecvInfo) -> Self {
        Self {
            stream: i.stream,
            ppid: i.ppid,
            assoc_id: i.assoc_id,
        }
    }
}

#[pymethods]
impl PyRecvInfo {
    /// The well-known name of the PPID, if any (e.g. `"NGAP"`).
    fn ppid_name(&self) -> Option<&'static str> {
        ppid::name(self.ppid)
    }
    fn __repr__(&self) -> String {
        format!(
            "RecvInfo(stream={}, ppid={}, assoc_id={})",
            self.stream,
            ppid::display(self.ppid),
            self.assoc_id
        )
    }
}

// ── SctpConfig ──────────────────────────────────────────────────────────────
/// Socket configuration (stream counts + sockopts) applied before connect/bind.
#[pyclass(name = "SctpConfig", module = "async_sctp._async_sctp", from_py_object)]
#[derive(Clone)]
pub struct PyConfig {
    inner: SctpConfig,
}

#[pymethods]
impl PyConfig {
    #[new]
    #[pyo3(signature = (*, out_streams=0, max_in_streams=0, nodelay=None, recv_buf=None, send_buf=None, autoclose=None))]
    fn new(
        out_streams: u16,
        max_in_streams: u16,
        nodelay: Option<bool>,
        recv_buf: Option<usize>,
        send_buf: Option<usize>,
        autoclose: Option<u32>,
    ) -> Self {
        let mut c = SctpConfig::new();
        if out_streams > 0 || max_in_streams > 0 {
            c = c.streams(out_streams, max_in_streams);
        }
        if let Some(n) = nodelay {
            c = c.nodelay(n);
        }
        if let Some(b) = recv_buf {
            c = c.recv_buf(b);
        }
        if let Some(b) = send_buf {
            c = c.send_buf(b);
        }
        if let Some(s) = autoclose {
            c = c.autoclose(s);
        }
        Self { inner: c }
    }
}

// ── SctpAssociation ─────────────────────────────────────────────────────────
/// A one-to-one SCTP association.
#[pyclass(
    name = "SctpAssociation",
    module = "async_sctp._async_sctp",
    skip_from_py_object
)]
pub struct PyAssociation {
    inner: Arc<SctpAssociation>,
}

#[pymethods]
impl PyAssociation {
    /// `await SctpAssociation.connect(addr, config=None)` — connect to a peer.
    #[staticmethod]
    #[pyo3(signature = (addr, config=None))]
    fn connect<'py>(
        py: Python<'py>,
        addr: &str,
        config: Option<PyConfig>,
    ) -> PyResult<Bound<'py, PyAny>> {
        init_runtime();
        let sa = parse_addr(addr)?;
        let cfg = config.map(|c| c.inner).unwrap_or_default();
        future_into_py(py, async move {
            let a = SctpAssociation::connect_with(sa, &cfg).await.map_err(err)?;
            Python::attach(|py| Py::new(py, PyAssociation { inner: Arc::new(a) }))
        })
    }

    /// `await SctpAssociation.connect_multi([addr, ...], config=None)`.
    #[staticmethod]
    #[pyo3(signature = (addrs, config=None))]
    fn connect_multi<'py>(
        py: Python<'py>,
        addrs: Vec<String>,
        config: Option<PyConfig>,
    ) -> PyResult<Bound<'py, PyAny>> {
        init_runtime();
        let sas = parse_addrs(&addrs)?;
        let cfg = config.map(|c| c.inner).unwrap_or_default();
        future_into_py(py, async move {
            let a = SctpAssociation::connect_multi_with(&sas, &cfg)
                .await
                .map_err(err)?;
            Python::attach(|py| Py::new(py, PyAssociation { inner: Arc::new(a) }))
        })
    }

    /// `await assoc.send(data, stream, ppid, *, unordered=False, ttl_ms=0)`.
    #[pyo3(signature = (data, stream, ppid, *, unordered=false, ttl_ms=0))]
    fn send<'py>(
        &self,
        py: Python<'py>,
        data: Vec<u8>,
        stream: u16,
        ppid: u32,
        unordered: bool,
        ttl_ms: u32,
    ) -> PyResult<Bound<'py, PyAny>> {
        let assoc = self.inner.clone();
        let opts = SendOptions {
            unordered,
            ttl_ms,
            ..Default::default()
        };
        future_into_py(py, async move {
            let n = assoc
                .send_with(&data, stream, ppid, &opts)
                .await
                .map_err(err)?;
            Ok(n)
        })
    }

    /// `await assoc.recv()` → `(bytes, RecvInfo)`. Notifications are skipped.
    fn recv<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let assoc = self.inner.clone();
        future_into_py(py, async move {
            let (data, info) = assoc.recv().await.map_err(err)?;
            Python::attach(|py| {
                let b: Py<PyAny> = PyBytes::new(py, &data).into_any().unbind();
                let i: Py<PyAny> = Py::new(py, PyRecvInfo::from(info))?.into_any();
                Ok((b, i))
            })
        })
    }

    /// The peer (remote) addresses of this association.
    fn peer_addrs(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .peer_addrs()
            .map_err(err)?
            .iter()
            .map(|a| a.to_string())
            .collect())
    }

    /// The local addresses of this association.
    fn local_addrs(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .local_addrs()
            .map_err(err)?
            .iter()
            .map(|a| a.to_string())
            .collect())
    }

    /// `await assoc.shutdown()` — graceful shutdown.
    fn shutdown<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let assoc = self.inner.clone();
        future_into_py(py, async move { assoc.shutdown().await.map_err(err) })
    }

    /// `await assoc.abort()` — abort the association immediately.
    fn abort<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let assoc = self.inner.clone();
        future_into_py(py, async move { assoc.abort().await.map_err(err) })
    }
}

// ── SctpListener ────────────────────────────────────────────────────────────
/// A one-to-one SCTP listener: `await accept()` yields an association per peer.
#[pyclass(
    name = "SctpListener",
    module = "async_sctp._async_sctp",
    skip_from_py_object
)]
pub struct PyListener {
    inner: Arc<SctpListener>,
}

#[pymethods]
impl PyListener {
    /// `SctpListener.bind(addr, config=None)` (synchronous).
    #[staticmethod]
    #[pyo3(signature = (addr, config=None))]
    fn bind(addr: &str, config: Option<PyConfig>) -> PyResult<Self> {
        let sa = parse_addr(addr)?;
        let cfg = config.map(|c| c.inner).unwrap_or_default();
        let l = in_runtime(|| SctpListener::bind_config(sa, &cfg))?;
        Ok(Self { inner: Arc::new(l) })
    }

    /// `SctpListener.bind_multi([addr, ...], config=None)` (synchronous).
    #[staticmethod]
    #[pyo3(signature = (addrs, config=None))]
    fn bind_multi(addrs: Vec<String>, config: Option<PyConfig>) -> PyResult<Self> {
        let sas = parse_addrs(&addrs)?;
        let cfg = config.map(|c| c.inner).unwrap_or_default();
        let l = in_runtime(|| SctpListener::bind_multi_with(&sas, &cfg))?;
        Ok(Self { inner: Arc::new(l) })
    }

    /// `await listener.accept()` → `(SctpAssociation, peer_addr_str)`.
    fn accept<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let listener = self.inner.clone();
        future_into_py(py, async move {
            let (assoc, peer) = listener.accept().await.map_err(err)?;
            Python::attach(|py| {
                let a: Py<PyAny> = Py::new(
                    py,
                    PyAssociation {
                        inner: Arc::new(assoc),
                    },
                )?
                .into_any();
                Ok((a, peer.to_string()))
            })
        })
    }

    /// The local (primary) address this listener is bound to.
    fn local_addr(&self) -> PyResult<String> {
        Ok(self.inner.local_addr().map_err(err)?.to_string())
    }
}

// ── SctpServer (one-to-many) ────────────────────────────────────────────────
/// A one-to-many SCTP socket: `await recv()` yields messages from any peer.
#[pyclass(
    name = "SctpServer",
    module = "async_sctp._async_sctp",
    skip_from_py_object
)]
pub struct PyServer {
    inner: Arc<SctpServer>,
}

#[pymethods]
impl PyServer {
    /// `SctpServer.bind(addr, config=None)` (synchronous).
    #[staticmethod]
    #[pyo3(signature = (addr, config=None))]
    fn bind(addr: &str, config: Option<PyConfig>) -> PyResult<Self> {
        let sa = parse_addr(addr)?;
        let cfg = config.map(|c| c.inner).unwrap_or_default();
        let s = in_runtime(|| SctpServer::bind_config(sa, &cfg))?;
        Ok(Self { inner: Arc::new(s) })
    }

    /// `SctpServer.bind_multi([addr, ...], config=None)` (synchronous).
    #[staticmethod]
    #[pyo3(signature = (addrs, config=None))]
    fn bind_multi(addrs: Vec<String>, config: Option<PyConfig>) -> PyResult<Self> {
        let sas = parse_addrs(&addrs)?;
        let cfg = config.map(|c| c.inner).unwrap_or_default();
        let s = in_runtime(|| SctpServer::bind_multi_with(&sas, &cfg))?;
        Ok(Self { inner: Arc::new(s) })
    }

    /// `await server.recv()` → the next data message, skipping notifications,
    /// as `(bytes, RecvInfo, peer_addr_str)`.
    fn recv<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let server = self.inner.clone();
        future_into_py(py, async move {
            loop {
                match server.recv().await.map_err(err)? {
                    ServerMessage::Data { data, info, addr } => {
                        return Python::attach(|py| {
                            let b: Py<PyAny> = PyBytes::new(py, &data).into_any().unbind();
                            let i: Py<PyAny> = Py::new(py, PyRecvInfo::from(info))?.into_any();
                            Ok((b, i, addr.to_string()))
                        });
                    }
                    ServerMessage::Notification(_) => continue,
                }
            }
        })
    }

    /// `await server.send(assoc_id, data, stream, ppid, *, unordered=False, ttl_ms=0)`.
    #[pyo3(signature = (assoc_id, data, stream, ppid, *, unordered=false, ttl_ms=0))]
    fn send<'py>(
        &self,
        py: Python<'py>,
        assoc_id: i32,
        data: Vec<u8>,
        stream: u16,
        ppid: u32,
        unordered: bool,
        ttl_ms: u32,
    ) -> PyResult<Bound<'py, PyAny>> {
        let server = self.inner.clone();
        let opts = SendOptions {
            unordered,
            ttl_ms,
            ..Default::default()
        };
        future_into_py(py, async move {
            let n = server
                .send_with(assoc_id, &data, stream, ppid, &opts)
                .await
                .map_err(err)?;
            Ok(n)
        })
    }

    /// Branch an association off into its own one-to-one `SctpAssociation`.
    fn peeloff(&self, assoc_id: i32) -> PyResult<PyAssociation> {
        let server = self.inner.clone();
        // peeloff builds an AsyncFd, so it needs the tokio reactor in scope.
        let a = in_runtime(|| server.peeloff(assoc_id))?;
        Ok(PyAssociation { inner: Arc::new(a) })
    }

    /// The local (primary) address this socket is bound to.
    fn local_addr(&self) -> PyResult<String> {
        Ok(self.inner.local_addr().map_err(err)?.to_string())
    }
}

// ── module wiring ───────────────────────────────────────────────────────────
const PPIDS: &[(&str, u32)] = &[
    ("IUA", ppid::IUA),
    ("M2UA", ppid::M2UA),
    ("M3UA", ppid::M3UA),
    ("SUA", ppid::SUA),
    ("M2PA", ppid::M2PA),
    ("V5UA", ppid::V5UA),
    ("S1AP", ppid::S1AP),
    ("X2AP", ppid::X2AP),
    ("NGAP", ppid::NGAP),
    ("XNAP", ppid::XNAP),
    ("F1AP", ppid::F1AP),
    ("E1AP", ppid::E1AP),
    ("DIAMETER", ppid::DIAMETER),
];

fn add_contents(m: &Bound<'_, PyModule>) -> PyResult<()> {
    init_runtime();
    m.add("SctpError", m.py().get_type::<SctpError>())?;
    m.add_class::<PyRecvInfo>()?;
    m.add_class::<PyConfig>()?;
    m.add_class::<PyAssociation>()?;
    m.add_class::<PyListener>()?;
    m.add_class::<PyServer>()?;
    // Well-known PPIDs as module constants (async_sctp.NGAP, …).
    for (name, val) in PPIDS {
        m.add(*name, *val)?;
    }
    Ok(())
}

/// Standalone wheel entry point (maturin `module-name = "async_sctp._async_sctp"`).
#[pymodule]
fn _async_sctp(m: &Bound<'_, PyModule>) -> PyResult<()> {
    add_contents(m)
}

/// Embedding entry point: mount `async_sctp` as a submodule of `parent`.
pub fn register(py: Python<'_>, parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(py, "async_sctp")?;
    add_contents(&m)?;
    parent.setattr("async_sctp", &m)?;
    Ok(())
}
