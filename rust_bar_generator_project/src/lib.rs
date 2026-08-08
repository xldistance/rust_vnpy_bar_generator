//! 基于 PyO3 的高性能 K 线生成模块。
//!
//! 该模块将 vn.py 常用的 Tick/Bar 数据结构映射为 Rust 类型，并在 Rust 侧完成：
//! - 时间戳解析与上海时区转换；
//! - Tick 到 1 分钟 Bar 的聚合；
//! - 分钟 Bar 到更大时间窗口 Bar 的聚合；
//! - 通过 Python 回调将聚合结果返回给上层。

use chrono::{Datelike, Duration, Timelike, DateTime, NaiveDate, NaiveDateTime, TimeZone};
use chrono_tz::Asia::Shanghai;
use once_cell::sync::Lazy;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDateTime, PyDict, PyFloat, PyInt, PyModule, PyString, PyTuple};
use regex::Regex;
use std::sync::RwLock;
use std::collections::HashSet;

// ================================================================================================
// 时区常量
// ================================================================================================
/// 模块内部统一使用的交易时区。
static TZ_INFO: Lazy<chrono_tz::Tz> = Lazy::new(|| Shanghai);

// ================================================================================================
// 辅助：毫秒时间戳 → chrono DateTime（纯 Rust，零 Python 开销）
// ================================================================================================
#[inline(always)]
/// 将毫秒级 Unix 时间戳转换为上海时区时间。
///
/// 当时间戳超出 `chrono` 可表示范围时返回 `None`。
fn millis_to_shanghai(ms: i64) -> Option<DateTime<chrono_tz::Tz>> {
    DateTime::from_timestamp_millis(ms).map(|dt| dt.with_timezone(&*TZ_INFO))
}

/// 从 Python `datetime` 对象提取毫秒时间戳。
///
/// 该函数会调用一次 Python 侧的 `timestamp()`，随后在 Rust 侧缓存结果，
/// 以减少后续重复跨语言调用的成本。
#[inline]
fn extract_millis_from_py(py: Python, dt_obj: &Py<PyAny>) -> PyResult<i64> {
    let ts = dt_obj.bind(py).call_method0("timestamp")?.extract::<f64>()?;
    Ok((ts * 1000.0) as i64)
}

// ================================================================================================
// 位掩码目标检查 — 替代 HashSet<u32>，用于 0..63 范围内的快速成员测试
// ================================================================================================
#[derive(Clone, Copy)]
/// 基于 `u64` 的紧凑位图，用于快速判断目标时间桶是否命中。
struct BitMask64(u64);

impl BitMask64 {
    fn new() -> Self { Self(0) }
    #[inline(always)]
    fn set(&mut self, bit: u32) { if bit < 64 { self.0 |= 1u64 << bit; } }
    #[inline(always)]
    fn contains(self, bit: u32) -> bool { bit < 64 && (self.0 & (1u64 << bit)) != 0 }
}

/// 根据起止范围和步长构建目标位图。
///
/// 主要用于分钟、小时、日、周、月等离散时间桶的快速成员测试。
fn build_bitmask(range_start: u32, range_end: u32, step: usize) -> BitMask64 {
    let mut m = BitMask64::new();
    let mut v = range_start;
    while v < range_end {
        m.set(v);
        v += step as u32;
    }
    m
}

// ================================================================================================
// RustInterval 枚举 - 时间周期
// ================================================================================================
/// 时间周期枚举。
///
/// 该类型会通过 PyO3 暴露给 Python，并兼容 vn.py 中常见的字符串与枚举表示。
#[pyclass(eq, eq_int, from_py_object, module = "rust_bar_generator")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RustInterval {
    /// Tick 级别数据。
    #[pyo3(name = "TICK")]    TICK,
    /// 1 分钟周期。
    #[pyo3(name = "MINUTE")]  MINUTE,
    /// 1 小时周期。
    #[pyo3(name = "HOUR")]    HOUR,
    /// 日线周期。
    #[pyo3(name = "DAILY")]   DAILY,
    /// 周线周期。
    #[pyo3(name = "WEEKLY")]  WEEKLY,
    /// 月线周期。
    #[pyo3(name = "MONTHLY")] MONTHLY,
}

#[pymethods]
impl RustInterval {
    fn __repr__(&self) -> String { format!("RustInterval.{:?}", self) }
    fn __str__(&self) -> &str { self.value() }
    #[getter]
    fn value(&self) -> &'static str {
        match self {
            Self::TICK => "tick", Self::MINUTE => "1m", Self::HOUR => "1h",
            Self::DAILY => "1d", Self::WEEKLY => "1w", Self::MONTHLY => "1M",
        }
    }
    fn __hash__(&self) -> isize { *self as isize }
}

impl RustInterval {
    /// 从 Python 对象中解析时间周期。
    ///
    /// 支持直接传入 `RustInterval`、字符串，以及带有 `name`/`value` 属性的对象。
    fn from_py_any(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(ri) = obj.extract::<RustInterval>() { return Ok(ri); }
        if let Ok(s) = obj.extract::<String>() { return Self::parse_string(&s); }
        for attr in &["name", "value"] {
            if let Ok(a) = obj.getattr(*attr) {
                if let Ok(s) = a.extract::<String>() { return Self::parse_string(&s); }
            }
        }
        if let Ok(m) = obj.getattr("__str__") {
            if let Ok(r) = m.call0() {
                if let Ok(s) = r.extract::<String>() { return Self::parse_string(&s); }
            }
        }
        Err(PyValueError::new_err("无法转换为 RustInterval"))
    }

    /// 从字符串表示解析时间周期。
    fn parse_string(s: &str) -> PyResult<Self> {
        match s {
            "tick" | "TICK" => Ok(Self::TICK),
            "1m" | "MINUTE" => Ok(Self::MINUTE),
            "1h" | "HOUR" => Ok(Self::HOUR),
            "1d" | "DAILY" => Ok(Self::DAILY),
            "1w" | "WEEKLY" => Ok(Self::WEEKLY),
            "1M" | "MONTHLY" => Ok(Self::MONTHLY),
            _ => Err(PyValueError::new_err(format!("无法识别的时间间隔: {}", s))),
        }
    }

    /// 返回用于序列化和日志输出的枚举名称。
    fn name_str(self) -> &'static str {
        match self {
            Self::TICK => "TICK", Self::MINUTE => "MINUTE", Self::HOUR => "HOUR",
            Self::DAILY => "DAILY", Self::WEEKLY => "WEEKLY", Self::MONTHLY => "MONTHLY",
        }
    }
}

// ================================================================================================
// RustExchange 枚举 - 交易所
// ================================================================================================
/// 交易所枚举。
///
/// 枚举值与 vn.py 常用交易所标识保持兼容，并支持从 Python 对象或字符串解析。
#[pyclass(eq, eq_int, from_py_object, module = "rust_bar_generator")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RustExchange {
    #[pyo3(name = "CFFEX")] CFFEX,
    #[pyo3(name = "SHFE")] SHFE,
    #[pyo3(name = "CZCE")] CZCE,
    #[pyo3(name = "DCE")] DCE,
    #[pyo3(name = "GFEX")] GFEX,
    #[pyo3(name = "INE")] INE,
    #[pyo3(name = "SSE")] SSE,
    #[pyo3(name = "SZSE")] SZSE,
    #[pyo3(name = "BSE")] BSE,
    #[pyo3(name = "SGE")] SGE,
    #[pyo3(name = "WXE")] WXE,
    #[pyo3(name = "CFETS")] CFETS,
    #[pyo3(name = "SMART")] SMART,
    #[pyo3(name = "NYSE")] NYSE,
    #[pyo3(name = "NASDAQ")] NASDAQ,
    #[pyo3(name = "ARCA")] ARCA,
    #[pyo3(name = "EDGEA")] EDGEA,
    #[pyo3(name = "ISLAND")] ISLAND,
    #[pyo3(name = "BATS")] BATS,
    #[pyo3(name = "IEX")] IEX,
    #[pyo3(name = "NYMEX")] NYMEX,
    #[pyo3(name = "COMEX")] COMEX,
    #[pyo3(name = "GLOBEX")] GLOBEX,
    #[pyo3(name = "IDEALPRO")] IDEALPRO,
    #[pyo3(name = "CME")] CME,
    #[pyo3(name = "ICE")] ICE,
    #[pyo3(name = "SEHK")] SEHK,
    #[pyo3(name = "HKFE")] HKFE,
    #[pyo3(name = "HKSE")] HKSE,
    #[pyo3(name = "SGX")] SGX,
    #[pyo3(name = "CBOT")] CBOT,
    #[pyo3(name = "CBOE")] CBOE,
    #[pyo3(name = "CFE")] CFE,
    #[pyo3(name = "DME")] DME,
    #[pyo3(name = "EUREX")] EUREX,
    #[pyo3(name = "APEX")] APEX,
    #[pyo3(name = "LME")] LME,
    #[pyo3(name = "BMD")] BMD,
    #[pyo3(name = "TOCOM")] TOCOM,
    #[pyo3(name = "EUNX")] EUNX,
    #[pyo3(name = "KRX")] KRX,
    #[pyo3(name = "OTC")] OTC,
    #[pyo3(name = "IBKRATS")] IBKRATS,
    #[pyo3(name = "TSE")] TSE,
    #[pyo3(name = "AMEX")] AMEX,
    #[pyo3(name = "BITMEX")] BITMEX,
    #[pyo3(name = "OKX")] OKX,
    #[pyo3(name = "HUOBI")] HUOBI,
    #[pyo3(name = "HUOBIP")] HUOBIP,
    #[pyo3(name = "HUOBIM")] HUOBIM,
    #[pyo3(name = "HUOBIF")] HUOBIF,
    #[pyo3(name = "HUOBISWAP")] HUOBISWAP,
    #[pyo3(name = "BITGETS")] BITGETS,
    #[pyo3(name = "BITGET")] BITGET,
    #[pyo3(name = "BITGETSPOT")] BITGETSPOT,
    #[pyo3(name = "BITFINEX")] BITFINEX,
    #[pyo3(name = "BITHUMB")] BITHUMB,
    #[pyo3(name = "BINANCE")] BINANCE,
    #[pyo3(name = "BINANCEF")] BINANCEF,
    #[pyo3(name = "BINANCES")] BINANCES,
    #[pyo3(name = "BINANCEO")] BINANCEO,
    #[pyo3(name = "POLY")] POLY,
    #[pyo3(name = "COINBASE")] COINBASE,
    #[pyo3(name = "BYBIT")] BYBIT,
    #[pyo3(name = "BYBITSPOT")] BYBITSPOT,
    #[pyo3(name = "KRAKEN")] KRAKEN,
    #[pyo3(name = "DERIBIT")] DERIBIT,
    #[pyo3(name = "GATEIO")] GATEIO,
    #[pyo3(name = "BITSTAMP")] BITSTAMP,
    #[pyo3(name = "BINGXS")] BINGXS,
    #[pyo3(name = "ORANGEX")] ORANGEX,
    #[pyo3(name = "KUCOIN")] KUCOIN,
    #[pyo3(name = "DYDX")] DYDX,
    #[pyo3(name = "HYPE")] HYPE,
    #[pyo3(name = "HYPESPOT")] HYPESPOT,
    #[pyo3(name = "LOCAL")] LOCAL,
}

#[pymethods]
impl RustExchange {
    fn __repr__(&self) -> String { format!("RustExchange.{:?}", self) }
    fn __str__(&self) -> &str { self.value() }
    #[getter]
    fn value(&self) -> &'static str {
        match self {
            Self::CFFEX => "CFFEX", Self::SHFE => "SHFE", Self::CZCE => "CZCE",
            Self::DCE => "DCE", Self::GFEX => "GFEX", Self::INE => "INE",
            Self::SSE => "SSE", Self::SZSE => "SZSE", Self::BSE => "BSE",
            Self::SGE => "SGE", Self::WXE => "WXE", Self::CFETS => "CFETS",
            Self::SMART => "SMART", Self::NYSE => "NYSE", Self::NASDAQ => "NASDAQ",
            Self::ARCA => "ARCA", Self::EDGEA => "EDGEA", Self::ISLAND => "ISLAND",
            Self::BATS => "BATS", Self::IEX => "IEX", Self::NYMEX => "NYMEX",
            Self::COMEX => "COMEX", Self::GLOBEX => "GLOBEX", Self::IDEALPRO => "IDEALPRO",
            Self::CME => "CME", Self::ICE => "ICE", Self::SEHK => "SEHK",
            Self::HKFE => "HKFE", Self::HKSE => "HKSE", Self::SGX => "SGX",
            Self::CBOT => "CBT", Self::CBOE => "CBOE", Self::CFE => "CFE",
            Self::DME => "DME", Self::EUREX => "EUX", Self::APEX => "APEX",
            Self::LME => "LME", Self::BMD => "BMD", Self::TOCOM => "TOCOM",
            Self::EUNX => "EUNX", Self::KRX => "KRX", Self::OTC => "PINK",
            Self::IBKRATS => "IBKRATS", Self::TSE => "TSE", Self::AMEX => "AMEX",
            Self::BITMEX => "BITMEX", Self::OKX => "OKX", Self::HUOBI => "HUOBI",
            Self::HUOBIP => "HUOBIP", Self::HUOBIM => "HUOBIM", Self::HUOBIF => "HUOBIF",
            Self::HUOBISWAP => "HUOBISWAP", Self::BITGETS => "BITGETS",
            Self::BITGET => "BITGET",Self::BITGETSPOT => "BITGETSPOT",
            Self::BITFINEX => "BITFINEX", Self::BITHUMB => "BITHUMB",
            Self::BINANCE => "BINANCE", Self::BINANCEF => "BINANCEF",
            Self::BINANCES => "BINANCES", Self::COINBASE => "COINBASE",
            Self::BINANCEO => "BINANCEO",
            Self::BYBIT => "BYBIT", Self::BYBITSPOT => "BYBITSPOT",
            Self::KRAKEN => "KRAKEN", Self::DERIBIT => "DERIBIT",
            Self::GATEIO => "GATEIO", Self::BITSTAMP => "BITSTAMP",
            Self::BINGXS => "BINGXS", Self::ORANGEX => "ORANGEX",
            Self::KUCOIN => "KUCOIN", Self::DYDX => "DYDX",
            Self::HYPE => "HYPE", Self::HYPESPOT => "HYPESPOT", Self::LOCAL => "LOCAL",
            Self::POLY => "POLY",
        }
    }
}

impl RustExchange {
    /// 从 Python 对象中解析交易所枚举。
    fn from_py_any(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(re) = obj.extract::<RustExchange>() { return Ok(re); }
        if let Ok(s) = obj.extract::<String>() { return Self::parse_string(&s); }
        for attr in &["name", "value"] {
            if let Ok(a) = obj.getattr(*attr) {
                if let Ok(s) = a.extract::<String>() { return Self::parse_string(&s); }
            }
        }
        if let Ok(m) = obj.getattr("__str__") {
            if let Ok(r) = m.call0() {
                if let Ok(s) = r.extract::<String>() { return Self::parse_string(&s); }
            }
        }
        Err(PyValueError::new_err("无法转换为 RustExchange"))
    }

    /// 从字符串表示解析交易所。
    ///
    /// 解析时会统一转换为大写，并兼容若干别名写法。
    fn parse_string(s: &str) -> PyResult<Self> {
        match s.to_uppercase().as_str() {
            "CFFEX" => Ok(Self::CFFEX), "SHFE" => Ok(Self::SHFE),
            "CZCE" => Ok(Self::CZCE), "DCE" => Ok(Self::DCE),
            "GFEX" => Ok(Self::GFEX), "INE" => Ok(Self::INE),
            "SSE" => Ok(Self::SSE), "SZSE" => Ok(Self::SZSE),
            "BSE" => Ok(Self::BSE), "SGE" => Ok(Self::SGE),
            "WXE" => Ok(Self::WXE), "CFETS" => Ok(Self::CFETS),
            "SMART" => Ok(Self::SMART), "NYSE" => Ok(Self::NYSE),
            "NASDAQ" => Ok(Self::NASDAQ), "ARCA" => Ok(Self::ARCA),
            "EDGEA" => Ok(Self::EDGEA), "ISLAND" => Ok(Self::ISLAND),
            "BATS" => Ok(Self::BATS), "IEX" => Ok(Self::IEX),
            "NYMEX" => Ok(Self::NYMEX), "COMEX" => Ok(Self::COMEX),
            "GLOBEX" => Ok(Self::GLOBEX), "IDEALPRO" => Ok(Self::IDEALPRO),
            "CME" => Ok(Self::CME), "ICE" => Ok(Self::ICE),
            "SEHK" => Ok(Self::SEHK), "HKFE" => Ok(Self::HKFE),
            "HKSE" => Ok(Self::HKSE), "SGX" => Ok(Self::SGX),
            "CBOT" | "CBT" => Ok(Self::CBOT), "CBOE" => Ok(Self::CBOE),
            "CFE" => Ok(Self::CFE), "DME" => Ok(Self::DME),
            "EUREX" | "EUX" => Ok(Self::EUREX), "APEX" => Ok(Self::APEX),
            "LME" => Ok(Self::LME), "BMD" => Ok(Self::BMD),
            "TOCOM" => Ok(Self::TOCOM), "EUNX" => Ok(Self::EUNX),
            "KRX" => Ok(Self::KRX), "OTC" | "PINK" => Ok(Self::OTC),
            "IBKRATS" => Ok(Self::IBKRATS), "TSE" => Ok(Self::TSE),
            "AMEX" => Ok(Self::AMEX),
            "BITMEX" => Ok(Self::BITMEX), "OKX" => Ok(Self::OKX),
            "HUOBI" => Ok(Self::HUOBI), "HUOBIP" => Ok(Self::HUOBIP),
            "HUOBIM" => Ok(Self::HUOBIM), "HUOBIF" => Ok(Self::HUOBIF),
            "HUOBISWAP" => Ok(Self::HUOBISWAP), "BITGETS" => Ok(Self::BITGETS),
            "BITGET" => Ok(Self::BITGET),"BITGETSPOT" => Ok(Self::BITGETSPOT),
            "BITFINEX" => Ok(Self::BITFINEX), "BITHUMB" => Ok(Self::BITHUMB),
            "BINANCE" => Ok(Self::BINANCE), "BINANCEF" => Ok(Self::BINANCEF),
            "BINANCES" => Ok(Self::BINANCES), "COINBASE" => Ok(Self::COINBASE),
            "BINANCEO" => Ok(Self::BINANCEO),
            "BYBIT" => Ok(Self::BYBIT), "BYBITSPOT" => Ok(Self::BYBITSPOT),
            "KRAKEN" => Ok(Self::KRAKEN), "DERIBIT" => Ok(Self::DERIBIT),
            "GATEIO" => Ok(Self::GATEIO), "BITSTAMP" => Ok(Self::BITSTAMP),
            "BINGXS" => Ok(Self::BINGXS), "ORANGEX" => Ok(Self::ORANGEX),
            "KUCOIN" => Ok(Self::KUCOIN), "DYDX" => Ok(Self::DYDX),
            "HYPE" => Ok(Self::HYPE), "HYPESPOT" => Ok(Self::HYPESPOT),
            "LOCAL" => Ok(Self::LOCAL),"POLY" => Ok(Self::POLY),
            _ => Err(PyValueError::new_err(format!("无法识别的交易所: {}", s))),
        }
    }
}

// ================================================================================================
// RustBarData - K线数据结构
// ================================================================================================
/// Rust 侧的 K 线数据结构。
///
/// 该结构与 vn.py 的 `BarData` 字段语义保持一致，同时额外缓存毫秒时间戳，
/// 以降低重复访问 Python `datetime` 的开销。
#[pyclass(from_py_object, module = "rust_bar_generator")]
#[derive(Debug)]
pub struct RustBarData {
    #[pyo3(get, set)] pub symbol: String,
    #[pyo3(get, set)] pub exchange: RustExchange,
    #[pyo3(get, set)] pub datetime: Option<Py<PyAny>>,
    #[pyo3(get, set)] pub interval: Option<RustInterval>,
    #[pyo3(get, set)] pub volume: f64,
    #[pyo3(get, set)] pub open_interest: f64,
    #[pyo3(get, set)] pub open_price: f64,
    #[pyo3(get, set)] pub high_price: f64,
    #[pyo3(get, set)] pub low_price: f64,
    #[pyo3(get, set)] pub close_price: f64,
    #[pyo3(get, set)] pub gateway_name: String,
    #[pyo3(get, set)] pub vt_symbol: String,
    /// 缓存后的毫秒时间戳，避免重复调用 Python `datetime.timestamp()`。
    datetime_millis: Option<i64>,
}

impl Clone for RustBarData {
    fn clone(&self) -> Self {
        Python::attach(|py| self.clone_with_py(py))
    }
}

impl RustBarData {
    /// 在已持有 Python 解释器上下文时克隆对象。
    fn clone_with_py(&self, py: Python) -> Self {
        RustBarData {
            symbol: self.symbol.clone(),
            exchange: self.exchange,
            datetime: self.datetime.as_ref().map(|dt| dt.clone_ref(py)),
            interval: self.interval,
            volume: self.volume,
            open_interest: self.open_interest,
            open_price: self.open_price,
            high_price: self.high_price,
            low_price: self.low_price,
            close_price: self.close_price,
            gateway_name: self.gateway_name.clone(),
            vt_symbol: self.vt_symbol.clone(),
            datetime_millis: self.datetime_millis,
        }
    }

    #[inline]
    /// 以 `chrono::DateTime` 形式获取缓存后的时间。
    fn get_datetime_chrono(&self, py: Python) -> PyResult<Option<DateTime<chrono_tz::Tz>>> {
        if let Some(ms) = self.datetime_millis {
            return Ok(millis_to_shanghai(ms));
        }
        if let Some(ref dt_obj) = self.datetime {
            let ms = extract_millis_from_py(py, dt_obj)?;
            Ok(millis_to_shanghai(ms))
        } else {
            Ok(None)
        }
    }

    #[inline]
    /// 获取当前对象对应的毫秒时间戳。
    fn get_millis(&self, py: Python) -> PyResult<Option<i64>> {
        if let Some(ms) = self.datetime_millis {
            return Ok(Some(ms));
        }
        if let Some(ref dt_obj) = self.datetime {
            Ok(Some(extract_millis_from_py(py, dt_obj)?))
        } else {
            Ok(None)
        }
    }

    /// 同步更新 Python `datetime` 对象与对应的毫秒缓存。
    fn set_datetime_cached(&mut self, py_dt: Py<PyAny>, millis: i64) {
        self.datetime = Some(py_dt);
        self.datetime_millis = Some(millis);
    }

    /// 从 Python 侧 `BarData` 风格对象转换为 Rust 结构。
    fn from_py_bar(py: Python, py_bar: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(rust_bar) = py_bar.extract::<RustBarData>() {
            return Ok(rust_bar);
        }

        let symbol = py_bar.getattr("symbol")?.extract::<String>()?;
        let gateway_name = py_bar.getattr("gateway_name")?.extract::<String>()?;
        let exchange_obj = py_bar.getattr("exchange")?;
        let exchange = RustExchange::from_py_any(&exchange_obj)?;

        let (datetime, datetime_millis) = if let Ok(dt_attr) = py_bar.getattr("datetime") {
            if dt_attr.is_none() {
                (None, None)
            } else {
                let ms = extract_millis_from_py(py, &dt_attr.unbind())?;
                (Some(py_bar.getattr("datetime")?.unbind()), Some(ms))
            }
        } else {
            (None, None)
        };

        let interval = if let Ok(iv) = py_bar.getattr("interval") {
            if iv.is_none() { None } else { Some(RustInterval::from_py_any(&iv)?) }
        } else { None };

        let volume = py_bar.getattr("volume")?.extract::<f64>().unwrap_or(0.0);
        let open_interest = py_bar.getattr("open_interest")?.extract::<f64>().unwrap_or(0.0);
        let open_price = py_bar.getattr("open_price")?.extract::<f64>().unwrap_or(0.0);
        let high_price = py_bar.getattr("high_price")?.extract::<f64>().unwrap_or(0.0);
        let low_price = py_bar.getattr("low_price")?.extract::<f64>().unwrap_or(0.0);
        let close_price = py_bar.getattr("close_price")?.extract::<f64>().unwrap_or(0.0);

        let vt_symbol = format!("{}_{}/{}", symbol, exchange.__str__(), gateway_name);

        Ok(RustBarData {
            symbol, exchange, datetime, interval, volume, open_interest,
            open_price, high_price, low_price, close_price,
            gateway_name, vt_symbol, datetime_millis,
        })
    }
}

#[pymethods]
impl RustBarData {
    /// 创建一个新的 K 线对象。
    ///
    /// `datetime` 和 `interval` 既可以传入 Rust 暴露的枚举，也可以传入兼容的
    /// Python 对象或字符串表示。
    #[new]
    #[pyo3(signature = (symbol, exchange, gateway_name, datetime=None, interval=None,
        volume=0.0, open_interest=0.0, open_price=0.0, high_price=0.0, low_price=0.0, close_price=0.0))]
    fn new(
        py: Python, symbol: String, exchange: &Bound<'_, PyAny>, gateway_name: String,
        datetime: Option<&Bound<'_, PyAny>>, interval: Option<&Bound<'_, PyAny>>,
        volume: f64, open_interest: f64, open_price: f64, high_price: f64,
        low_price: f64, close_price: f64,
    ) -> PyResult<Self> {
        let rust_exchange = RustExchange::from_py_any(exchange)?;
        let rust_interval = interval.map(RustInterval::from_py_any).transpose()?;

        let (py_datetime, datetime_millis) = if let Some(dt) = datetime {
            let obj = dt.clone().unbind();
            let ms = extract_millis_from_py(py, &obj)?;
            (Some(obj), Some(ms))
        } else {
            (None, None)
        };

        let vt_symbol = format!("{}_{}/{}", symbol, rust_exchange.__str__(), gateway_name);

        Ok(RustBarData {
            symbol, exchange: rust_exchange, datetime: py_datetime, interval: rust_interval,
            volume, open_interest, open_price, high_price, low_price, close_price,
            gateway_name, vt_symbol, datetime_millis,
        })
    }

    fn __reduce__<'py>(&self, py: Python<'py>) -> PyResult<(Py<PyAny>, Py<PyAny>)> {
        let cls = PyModule::import(py, "rust_bar_generator")?.getattr("RustBarData")?;
        let dt_for_pickle = self.datetime.as_ref().map(|dt| dt.clone_ref(py));
        let args = PyTuple::new(py, &[
            self.symbol.clone().into_pyobject(py)?.into_any().unbind(),
            self.exchange.__str__().into_pyobject(py)?.into_any().unbind(),
            self.gateway_name.clone().into_pyobject(py)?.into_any().unbind(),
            dt_for_pickle.into_pyobject(py)?.into_any().unbind(),
            self.interval.map(|i| i.name_str()).into_pyobject(py)?.into_any().unbind(),
            self.volume.into_pyobject(py)?.into_any().unbind(),
            self.open_interest.into_pyobject(py)?.into_any().unbind(),
            self.open_price.into_pyobject(py)?.into_any().unbind(),
            self.high_price.into_pyobject(py)?.into_any().unbind(),
            self.low_price.into_pyobject(py)?.into_any().unbind(),
            self.close_price.into_pyobject(py)?.into_any().unbind(),
        ])?;
        Ok((cls.unbind(), args.unbind().into()))
    }

    fn __repr__(&self) -> String {
        format!("RustBarData(symbol='{}', exchange={:?}, datetime={:?}, interval={:?})",
            self.symbol, self.exchange, self.datetime, self.interval)
    }
}

// ================================================================================================
// RustTickData - Tick数据结构
// ================================================================================================
/// Rust 侧的 Tick 数据结构。
///
/// 该结构与 vn.py 的 `TickData` 主要字段保持一致，并缓存毫秒时间戳用于高频聚合。
#[pyclass(from_py_object, module = "rust_bar_generator")]
#[derive(Debug)]
pub struct RustTickData {
    #[pyo3(get, set)] pub symbol: String,
    #[pyo3(get, set)] pub exchange: RustExchange,
    #[pyo3(get, set)] pub datetime: Option<Py<PyAny>>,
    #[pyo3(get, set)] pub name: String,
    #[pyo3(get, set)] pub volume: f64,
    #[pyo3(get, set)] pub open_interest: f64,
    #[pyo3(get, set)] pub last_price: f64,
    #[pyo3(get, set)] pub last_volume: f64,
    #[pyo3(get, set)] pub limit_up: f64,
    #[pyo3(get, set)] pub limit_down: f64,
    #[pyo3(get, set)] pub open_price: f64,
    #[pyo3(get, set)] pub high_price: f64,
    #[pyo3(get, set)] pub low_price: f64,
    #[pyo3(get, set)] pub pre_close: f64,
    #[pyo3(get, set)] pub bid_price_1: f64,
    #[pyo3(get, set)] pub bid_price_2: f64,
    #[pyo3(get, set)] pub bid_price_3: f64,
    #[pyo3(get, set)] pub bid_price_4: f64,
    #[pyo3(get, set)] pub bid_price_5: f64,
    #[pyo3(get, set)] pub ask_price_1: f64,
    #[pyo3(get, set)] pub ask_price_2: f64,
    #[pyo3(get, set)] pub ask_price_3: f64,
    #[pyo3(get, set)] pub ask_price_4: f64,
    #[pyo3(get, set)] pub ask_price_5: f64,
    #[pyo3(get, set)] pub bid_volume_1: f64,
    #[pyo3(get, set)] pub bid_volume_2: f64,
    #[pyo3(get, set)] pub bid_volume_3: f64,
    #[pyo3(get, set)] pub bid_volume_4: f64,
    #[pyo3(get, set)] pub bid_volume_5: f64,
    #[pyo3(get, set)] pub ask_volume_1: f64,
    #[pyo3(get, set)] pub ask_volume_2: f64,
    #[pyo3(get, set)] pub ask_volume_3: f64,
    #[pyo3(get, set)] pub ask_volume_4: f64,
    #[pyo3(get, set)] pub ask_volume_5: f64,
    #[pyo3(get, set)] pub gateway_name: String,
    #[pyo3(get, set)] pub vt_symbol: String,
    /// 缓存后的毫秒时间戳，避免在高频路径中重复调用 Python。
    datetime_millis: Option<i64>,
}

impl Clone for RustTickData {
    fn clone(&self) -> Self { Python::attach(|py| self.clone_with_py(py)) }
}

impl RustTickData {
    /// 在已持有 Python 解释器上下文时克隆对象。
    fn clone_with_py(&self, py: Python) -> Self {
        RustTickData {
            symbol: self.symbol.clone(), exchange: self.exchange,
            datetime: self.datetime.as_ref().map(|dt| dt.clone_ref(py)),
            name: self.name.clone(), volume: self.volume,
            open_interest: self.open_interest, last_price: self.last_price,
            last_volume: self.last_volume, limit_up: self.limit_up,
            limit_down: self.limit_down, open_price: self.open_price,
            high_price: self.high_price, low_price: self.low_price,
            pre_close: self.pre_close,
            bid_price_1: self.bid_price_1, bid_price_2: self.bid_price_2,
            bid_price_3: self.bid_price_3, bid_price_4: self.bid_price_4,
            bid_price_5: self.bid_price_5,
            ask_price_1: self.ask_price_1, ask_price_2: self.ask_price_2,
            ask_price_3: self.ask_price_3, ask_price_4: self.ask_price_4,
            ask_price_5: self.ask_price_5,
            bid_volume_1: self.bid_volume_1, bid_volume_2: self.bid_volume_2,
            bid_volume_3: self.bid_volume_3, bid_volume_4: self.bid_volume_4,
            bid_volume_5: self.bid_volume_5,
            ask_volume_1: self.ask_volume_1, ask_volume_2: self.ask_volume_2,
            ask_volume_3: self.ask_volume_3, ask_volume_4: self.ask_volume_4,
            ask_volume_5: self.ask_volume_5,
            gateway_name: self.gateway_name.clone(), vt_symbol: self.vt_symbol.clone(),
            datetime_millis: self.datetime_millis,
        }
    }

    #[inline]
    /// 以 `chrono::DateTime` 形式获取缓存后的时间。
    fn get_datetime_chrono(&self, py: Python) -> PyResult<Option<DateTime<chrono_tz::Tz>>> {
        if let Some(ms) = self.datetime_millis {
            return Ok(millis_to_shanghai(ms));
        }
        if let Some(ref dt_obj) = self.datetime {
            let ms = extract_millis_from_py(py, dt_obj)?;
            Ok(millis_to_shanghai(ms))
        } else {
            Ok(None)
        }
    }

    #[inline]
    /// 获取当前 Tick 对应的毫秒时间戳。
    fn get_millis(&self, py: Python) -> PyResult<Option<i64>> {
        if let Some(ms) = self.datetime_millis { return Ok(Some(ms)); }
        if let Some(ref dt_obj) = self.datetime {
            Ok(Some(extract_millis_from_py(py, dt_obj)?))
        } else { Ok(None) }
    }

    /// 从 Python 侧 `TickData` 风格对象转换为 Rust 结构。
    fn from_py_tick(py: Python, py_tick: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(rust_tick) = py_tick.extract::<RustTickData>() {
            return Ok(rust_tick);
        }

        let symbol = py_tick.getattr("symbol")?.extract::<String>()?;
        let gateway_name = py_tick.getattr("gateway_name")?.extract::<String>()?;
        let exchange_obj = py_tick.getattr("exchange")?;
        let exchange = RustExchange::from_py_any(&exchange_obj)?;

        let (datetime, datetime_millis) = if let Ok(dt_attr) = py_tick.getattr("datetime") {
            if dt_attr.is_none() {
                (None, None)
            } else {
                let obj = dt_attr.unbind();
                let ms = extract_millis_from_py(py, &obj)?;
                (Some(obj), Some(ms))
            }
        } else { (None, None) };

        macro_rules! get_f64 {
            ($attr:literal) => {
                py_tick.getattr($attr)?.extract::<f64>().unwrap_or(0.0)
            };
        }

        let vt_symbol = format!("{}_{}/{}", symbol, exchange.__str__(), gateway_name);

        Ok(RustTickData {
            symbol, exchange, datetime, datetime_millis,
            name: py_tick.getattr("name")?.extract::<String>().unwrap_or_default(),
            volume: get_f64!("volume"), open_interest: get_f64!("open_interest"),
            last_price: get_f64!("last_price"), last_volume: get_f64!("last_volume"),
            limit_up: get_f64!("limit_up"), limit_down: get_f64!("limit_down"),
            open_price: get_f64!("open_price"), high_price: get_f64!("high_price"),
            low_price: get_f64!("low_price"), pre_close: get_f64!("pre_close"),
            bid_price_1: get_f64!("bid_price_1"), bid_price_2: get_f64!("bid_price_2"),
            bid_price_3: get_f64!("bid_price_3"), bid_price_4: get_f64!("bid_price_4"),
            bid_price_5: get_f64!("bid_price_5"),
            ask_price_1: get_f64!("ask_price_1"), ask_price_2: get_f64!("ask_price_2"),
            ask_price_3: get_f64!("ask_price_3"), ask_price_4: get_f64!("ask_price_4"),
            ask_price_5: get_f64!("ask_price_5"),
            bid_volume_1: get_f64!("bid_volume_1"), bid_volume_2: get_f64!("bid_volume_2"),
            bid_volume_3: get_f64!("bid_volume_3"), bid_volume_4: get_f64!("bid_volume_4"),
            bid_volume_5: get_f64!("bid_volume_5"),
            ask_volume_1: get_f64!("ask_volume_1"), ask_volume_2: get_f64!("ask_volume_2"),
            ask_volume_3: get_f64!("ask_volume_3"), ask_volume_4: get_f64!("ask_volume_4"),
            ask_volume_5: get_f64!("ask_volume_5"),
            gateway_name, vt_symbol,
        })
    }
}

#[pymethods]
impl RustTickData {
    /// 创建一个新的 Tick 对象。
    ///
    /// 除必填字段外，其余行情字段通过 `kwargs` 传入，未提供时使用默认值 `0.0`
    /// 或空字符串。
    #[new]
    #[pyo3(signature = (symbol, exchange, gateway_name, datetime=None, **kwargs))]
    fn new(
        py: Python, symbol: String, exchange: &Bound<'_, PyAny>,
        gateway_name: String, datetime: Option<&Bound<'_, PyAny>>,
        kwargs: Option<Bound<'_, PyDict>>,
    ) -> PyResult<Self> {
        let rust_exchange = RustExchange::from_py_any(exchange)?;
        let vt_symbol = format!("{}_{}/{}", symbol, rust_exchange.__str__(), gateway_name);

        let (py_datetime, datetime_millis) = if let Some(dt) = datetime {
            let obj = dt.clone().unbind();
            let ms = extract_millis_from_py(py, &obj)?;
            (Some(obj), Some(ms))
        } else { (None, None) };

        let mut tick = RustTickData {
            symbol, exchange: rust_exchange, datetime: py_datetime, datetime_millis,
            name: String::new(), volume: 0.0, open_interest: 0.0, last_price: 0.0,
            last_volume: 0.0, limit_up: 0.0, limit_down: 0.0, open_price: 0.0,
            high_price: 0.0, low_price: 0.0, pre_close: 0.0,
            bid_price_1: 0.0, bid_price_2: 0.0, bid_price_3: 0.0,
            bid_price_4: 0.0, bid_price_5: 0.0,
            ask_price_1: 0.0, ask_price_2: 0.0, ask_price_3: 0.0,
            ask_price_4: 0.0, ask_price_5: 0.0,
            bid_volume_1: 0.0, bid_volume_2: 0.0, bid_volume_3: 0.0,
            bid_volume_4: 0.0, bid_volume_5: 0.0,
            ask_volume_1: 0.0, ask_volume_2: 0.0, ask_volume_3: 0.0,
            ask_volume_4: 0.0, ask_volume_5: 0.0,
            gateway_name, vt_symbol,
        };

        if let Some(kw) = kwargs {
            macro_rules! set_f64 {
                ($field:ident, $key:literal) => {
                    if let Ok(Some(v)) = kw.get_item($key) {
                        tick.$field = v.extract().unwrap_or(0.0);
                    }
                };
            }
            macro_rules! set_str {
                ($field:ident, $key:literal) => {
                    if let Ok(Some(v)) = kw.get_item($key) {
                        tick.$field = v.extract().unwrap_or_default();
                    }
                };
            }
            set_str!(name, "name");
            set_f64!(volume, "volume"); set_f64!(open_interest, "open_interest");
            set_f64!(last_price, "last_price"); set_f64!(last_volume, "last_volume");
            set_f64!(limit_up, "limit_up"); set_f64!(limit_down, "limit_down");
            set_f64!(open_price, "open_price"); set_f64!(high_price, "high_price");
            set_f64!(low_price, "low_price"); set_f64!(pre_close, "pre_close");
            set_f64!(bid_price_1, "bid_price_1"); set_f64!(bid_price_2, "bid_price_2");
            set_f64!(bid_price_3, "bid_price_3"); set_f64!(bid_price_4, "bid_price_4");
            set_f64!(bid_price_5, "bid_price_5");
            set_f64!(ask_price_1, "ask_price_1"); set_f64!(ask_price_2, "ask_price_2");
            set_f64!(ask_price_3, "ask_price_3"); set_f64!(ask_price_4, "ask_price_4");
            set_f64!(ask_price_5, "ask_price_5");
            set_f64!(bid_volume_1, "bid_volume_1"); set_f64!(bid_volume_2, "bid_volume_2");
            set_f64!(bid_volume_3, "bid_volume_3"); set_f64!(bid_volume_4, "bid_volume_4");
            set_f64!(bid_volume_5, "bid_volume_5");
            set_f64!(ask_volume_1, "ask_volume_1"); set_f64!(ask_volume_2, "ask_volume_2");
            set_f64!(ask_volume_3, "ask_volume_3"); set_f64!(ask_volume_4, "ask_volume_4");
            set_f64!(ask_volume_5, "ask_volume_5");
        }
        Ok(tick)
    }

    fn __reduce__<'py>(&self, py: Python<'py>) -> PyResult<(Py<PyAny>, Py<PyAny>, Py<PyAny>)> {
        let cls = PyModule::import(py, "rust_bar_generator")?.getattr("RustTickData")?;
        let dt_for_pickle = self.datetime.as_ref().map(|dt| dt.clone_ref(py));
        let args = PyTuple::new(py, &[
            self.symbol.clone().into_pyobject(py)?.into_any().unbind(),
            self.exchange.__str__().into_pyobject(py)?.into_any().unbind(),
            self.gateway_name.clone().into_pyobject(py)?.into_any().unbind(),
            dt_for_pickle.into_pyobject(py)?.into_any().unbind(),
        ])?;
        let kw = PyDict::new(py);
        macro_rules! kw_set {
            ($key:literal, $val:expr) => { kw.set_item($key, $val)?; };
        }
        kw_set!("name", &self.name); kw_set!("volume", self.volume);
        kw_set!("open_interest", self.open_interest); kw_set!("last_price", self.last_price);
        kw_set!("last_volume", self.last_volume); kw_set!("limit_up", self.limit_up);
        kw_set!("limit_down", self.limit_down); kw_set!("open_price", self.open_price);
        kw_set!("high_price", self.high_price); kw_set!("low_price", self.low_price);
        kw_set!("pre_close", self.pre_close);
        kw_set!("bid_price_1", self.bid_price_1); kw_set!("bid_price_2", self.bid_price_2);
        kw_set!("bid_price_3", self.bid_price_3); kw_set!("bid_price_4", self.bid_price_4);
        kw_set!("bid_price_5", self.bid_price_5);
        kw_set!("ask_price_1", self.ask_price_1); kw_set!("ask_price_2", self.ask_price_2);
        kw_set!("ask_price_3", self.ask_price_3); kw_set!("ask_price_4", self.ask_price_4);
        kw_set!("ask_price_5", self.ask_price_5);
        kw_set!("bid_volume_1", self.bid_volume_1); kw_set!("bid_volume_2", self.bid_volume_2);
        kw_set!("bid_volume_3", self.bid_volume_3); kw_set!("bid_volume_4", self.bid_volume_4);
        kw_set!("bid_volume_5", self.bid_volume_5);
        kw_set!("ask_volume_1", self.ask_volume_1); kw_set!("ask_volume_2", self.ask_volume_2);
        kw_set!("ask_volume_3", self.ask_volume_3); kw_set!("ask_volume_4", self.ask_volume_4);
        kw_set!("ask_volume_5", self.ask_volume_5);
        Ok((cls.unbind(), args.unbind().into(), kw.unbind().into()))
    }

    fn __repr__(&self) -> String {
        format!("RustTickData(symbol='{}', exchange={:?}, datetime={:?}, last_price={})",
            self.symbol, self.exchange, self.datetime, self.last_price)
    }
}

// ================================================================================================
// 时间解析函数
// ================================================================================================
/// 解析字符串形式的时间戳。
///
/// 支持 `YYYY-mm-dd HH:MM:SS`、`YYYYmmdd HH:MM:SS`、ISO-8601 以及带小数秒格式。
fn parse_str_timestamp(timestamp: &str) -> PyResult<NaiveDateTime> {
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[+Z]").unwrap());
    let cleaned = RE.split(timestamp).next().unwrap_or("").trim();
    let fmt = if cleaned.contains('-') {
        if cleaned.contains('T') {
            if cleaned.contains('.') { "%Y-%m-%dT%H:%M:%S%.f" } else { "%Y-%m-%dT%H:%M:%S" }
        } else if cleaned.contains('.') { "%Y-%m-%d %H:%M:%S%.f" }
        else { "%Y-%m-%d %H:%M:%S" }
    } else if cleaned.contains('.') { "%Y%m%d %H:%M:%S%.f" }
    else { "%Y%m%d %H:%M:%S" };
    NaiveDateTime::parse_from_str(cleaned, fmt)
        .map_err(|e| PyValueError::new_err(format!("时间解析失败: {}", e)))
}

#[pyfunction]
#[pyo3(signature = (timestamp, hours=0))]
/// 将输入时间戳转换为带系统本地时区信息的 Python `datetime` 对象。
///
/// 整数根据位数识别秒、毫秒、微秒和纳秒，浮点数按秒处理，
/// `hours` 默认为 `0`。返回值的时区跟随运行机器的系统时区。
fn get_local_datetime(py: Python, timestamp: Bound<'_, PyAny>, hours: i64) -> PyResult<Py<PyAny>> {
    let datetime_mod = py.import("datetime")?;
    let datetime_cls = datetime_mod.getattr("datetime")?;

    let local_time = if timestamp.is_instance_of::<PyInt>() {
        let value = timestamp.extract::<i64>()?;
        let divisor = match value.to_string().len() {
            10 => 1.0,
            13 => 1_000.0,
            16 => 1_000_000.0,
            19 => 1_000_000_000.0,
            _ => 1.0,
        };
        datetime_cls.call_method1("fromtimestamp", (value as f64 / divisor,))?
    } else if timestamp.is_instance_of::<PyFloat>() {
        datetime_cls.call_method1("fromtimestamp", (timestamp.extract::<f64>()?,))?
    } else if timestamp.is_instance_of::<PyString>() {
        let value = timestamp.extract::<String>()?;
        if timestamp.call_method0("isdigit")?.is_truthy()? {
            let integer = value.parse::<i64>()
                .map_err(|_| PyValueError::new_err("无效的时间戳字符串"))?;
            let divisor = match integer.to_string().len() {
                10 => 1.0,
                13 => 1_000.0,
                16 => 1_000_000.0,
                19 => 1_000_000_000.0,
                _ => 1.0,
            };
            datetime_cls.call_method1("fromtimestamp", (integer as f64 / divisor,))?
        } else {
            let dt = parse_str_timestamp(&value)?;
            datetime_cls.call1((
                dt.year(), dt.month(), dt.day(), dt.hour(), dt.minute(), dt.second(),
                dt.nanosecond() / 1_000,
            ))?
        }
    } else {
        return Err(PyTypeError::new_err(format!(
            "不支持的类型: {}",
            timestamp.get_type().name()?
        )));
    };

    let shifted = local_time.call_method1(
        "__add__",
        (datetime_mod.getattr("timedelta")?.call((), Some(&{
            let kwargs = PyDict::new(py);
            kwargs.set_item("hours", hours)?;
            kwargs
        }))?,),
    )?;

    // 对无时区 datetime 调用 astimezone()，由 Python 读取系统本地时区并附加
    // 正确的 UTC 偏移，避免在 UTC 系统上把本地钟表时间错误标记成 UTC+8。
    Ok(shifted.call_method0("astimezone")?.unbind())
}

// ================================================================================================
// BarGeneratorInner - 内部可变状态
// ================================================================================================
/// `BarGenerator` 的内部可变状态。
///
/// 该结构仅在 Rust 内部使用，通过 `RwLock` 进行同步保护。
struct BarGeneratorInner {
    bar: Option<RustBarData>,
    bar_millis: Option<i64>,
    interval_count: usize,
    window_bar: Option<RustBarData>,
    last_tick_volume: Option<f64>,
    last_bar_millis: Option<i64>,
    last_bar: Option<RustBarData>,
    bar_push_status: HashSet<i64>,
}

// ================================================================================================
// BarGenerator - K线生成器核心类
// ================================================================================================
/// K 线生成器。
///
/// 支持两类核心能力：
/// - 根据 Tick 数据实时聚合 1 分钟 K 线；
/// - 根据分钟 K 线继续聚合多窗口分钟、小时、日、周、月 K 线。
///
/// 聚合结果通过 `on_bar` 和 `on_window_bar` 回调返回给 Python 上层。
#[pyclass(module = "rust_bar_generator")]
pub struct BarGenerator {
    inner: RwLock<BarGeneratorInner>,
    on_bar: Option<Py<PyAny>>,
    on_window_bar: Option<Py<PyAny>>,
    interval: RustInterval,
    window: usize,
    interval_slice: bool,
    target_minutes: BitMask64,
    target_hours: BitMask64,
    target_days: BitMask64,
    target_weeks: BitMask64,
    target_months: BitMask64,
}

/// 直接从毫秒时间戳创建裁剪到分钟精度的 Python `datetime`。
fn make_trimmed_py_dt(py: Python, ms: i64) -> PyResult<Bound<'_, PyDateTime>> {
    let dt = millis_to_shanghai(ms)
        .ok_or_else(|| PyValueError::new_err("无效的时间戳"))?;
    PyDateTime::new(py, dt.year(), dt.month() as u8, dt.day() as u8,
        dt.hour() as u8, dt.minute() as u8, 0, 0, None)
}

/// 将 Bar 的时间截断到分钟精度，并同步更新内部毫秒缓存。
fn trim_bar_time(py: Python, mut bar: RustBarData) -> PyResult<RustBarData> {
    if let Some(ms) = bar.datetime_millis {
        let trimmed = make_trimmed_py_dt(py, ms)?;
        let dt = millis_to_shanghai(ms).unwrap();
        let trimmed_ms = dt.with_second(0).unwrap().with_nanosecond(0).unwrap().timestamp_millis();
        bar.datetime = Some(trimmed.into());
        bar.datetime_millis = Some(trimmed_ms);
    } else if let Some(ref dt_obj) = bar.datetime {
        let ms = extract_millis_from_py(py, dt_obj)?;
        let trimmed = make_trimmed_py_dt(py, ms)?;
        let dt = millis_to_shanghai(ms).unwrap();
        let trimmed_ms = dt.with_second(0).unwrap().with_nanosecond(0).unwrap().timestamp_millis();
        bar.datetime = Some(trimmed.into());
        bar.datetime_millis = Some(trimmed_ms);
    }
    Ok(bar)
}

#[pymethods]
impl BarGenerator {
    /// 创建一个新的 K 线生成器。
    ///
    /// - `on_bar`：分钟 Bar 生成后的回调。
    /// - `window`：聚合窗口长度，最小值为 `1`。
    /// - `on_window_bar`：窗口 Bar 生成后的回调。
    /// - `interval`：目标聚合周期，默认值为分钟。
    /// - `interval_slice`：是否按照自然时间切片对齐。
    #[new]
    #[pyo3(signature = (on_bar=None, window=1, on_window_bar=None, interval=None,
        interval_slice=true))]
    fn new(
        _py: Python, on_bar: Option<Py<PyAny>>, window: usize,
        on_window_bar: Option<Py<PyAny>>, interval: Option<&Bound<'_, PyAny>>,
        interval_slice: bool,
    ) -> PyResult<Self> {
        let rust_interval = interval.map(RustInterval::from_py_any).transpose()?
            .unwrap_or(RustInterval::MINUTE);
        let w = window.max(1);

        Ok(BarGenerator {
            inner: RwLock::new(BarGeneratorInner {
                bar: None, bar_millis: None, interval_count: 0,
                window_bar: None, last_tick_volume: None,
                last_bar_millis: None, last_bar: None,
                bar_push_status: HashSet::new(),
            }),
            on_bar, on_window_bar, interval: rust_interval, window: w,
            interval_slice,
            target_minutes: build_bitmask(0, 60, w),
            target_hours: build_bitmask(0, 24, w),
            target_days: build_bitmask(1, 32, w),
            target_weeks: build_bitmask(1, 53, w),
            target_months: build_bitmask(1, 13, w),
        })
    }

    fn __reduce__<'py>(&self, py: Python<'py>) -> PyResult<(Py<PyAny>, Py<PyAny>)> {
        let cls = PyModule::import(py, "rust_bar_generator")?.getattr("BarGenerator")?;
        let args = (
            self.on_bar.as_ref().map(|f| f.clone_ref(py)), self.window,
            self.on_window_bar.as_ref().map(|f| f.clone_ref(py)),
            self.interval.name_str(), self.interval_slice,
        );
        Ok((cls.into(), args.into_pyobject(py)?.into()))
    }

    /// 输入一个 Tick 数据点，并尝试更新当前分钟 K 线。
    fn update_tick(&self, py: Python, tick: Bound<'_, PyAny>) -> PyResult<()> {
        let rust_tick = RustTickData::from_py_tick(py, &tick)?;
        self.update_tick_internal(py, rust_tick)
    }

    /// 输入一个分钟 Bar，并尝试聚合更大窗口的目标周期 K 线。
    fn update_bar(&self, py: Python, bar: Bound<'_, PyAny>) -> PyResult<()> {
        let rust_bar = RustBarData::from_py_bar(py, &bar)?;
        self.update_bar_internal(py, rust_bar)
    }

    /// 强制结束当前分钟 K 线并触发 `on_bar` 回调。
    fn generate(&self, py: Python) -> PyResult<()> {
        let bar_to_callback = {
            let mut inner = self.inner.write().unwrap();
            inner.bar_millis = None;
            inner.bar.take()
        };
        if let Some(bar) = bar_to_callback {
            if let Some(ref callback) = self.on_bar {
                let mut new_bar = bar;
                let now = chrono::Utc::now().with_timezone(&*TZ_INFO) - Duration::minutes(1);
                let ms = now.with_second(0).unwrap().with_nanosecond(0).unwrap().timestamp_millis();
                let py_dt = PyDateTime::new(py, now.year(), now.month() as u8, now.day() as u8,
                    now.hour() as u8, now.minute() as u8, 0, 0, None)?;
                new_bar.set_datetime_cached(py_dt.into(), ms);
                let trimmed = trim_bar_time(py, new_bar)?;
                callback.call1(py, (trimmed,)).map_err(|e|
                    PyValueError::new_err(format!("trimmed_bar回调处理错误：{:#?}", e)))?;
            }
        }
        Ok(())
    }

    /// 根据外部事件检查当前分钟 Bar 是否超时，并在必要时强制补发。
    fn generate_bar_event(&self, py: Python, _event: Bound<'_, PyAny>) -> PyResult<()> {
        let should_generate = {
            let mut inner = self.inner.write().unwrap();
            let ms = match inner.bar_millis {
                Some(ms) => ms,
                None => return Ok(()),
            };
            let bar = match inner.bar.as_ref() {
                Some(bar) => bar,
                None => return Ok(()),
            };
            let bar_dt = millis_to_shanghai(ms)
                .ok_or_else(|| PyValueError::new_err("Bar datetime 转换失败"))?;
            let bar_minute_timestamp = bar_dt
                .with_second(0).unwrap()
                .with_nanosecond(0).unwrap()
                .timestamp_millis();
            if inner.bar_push_status.contains(&bar_minute_timestamp) {
                return Ok(());
            }
            let now_ms = chrono::Utc::now().timestamp_millis();
            if (now_ms - ms) <= 120_000 {
                return Ok(());
            }
            let vt = bar.vt_symbol.clone();
            let dt_str = bar_dt.to_string();
            inner.bar_push_status.insert(bar_minute_timestamp);
            Some((vt, dt_str))
        };

        if let Some((vt_symbol, bar_dt_str)) = should_generate {
            println!("合约：{}，最新bar时间：{}，分钟bar缺失即将强制合成分钟bar", vt_symbol, bar_dt_str);
            self.generate(py)?;
        }
        Ok(())
    }

    fn __repr__(&self) -> String {
        format!("BarGenerator(interval={:?}, window={})", self.interval, self.window)
    }
}

impl BarGenerator {
    /// Tick 到分钟 Bar 的核心聚合逻辑。
    fn update_tick_internal(&self, py: Python, tick: RustTickData) -> PyResult<()> {
        if tick.last_price == 0.0 { return Ok(()); }

        let tick_ms = tick.get_millis(py)?
            .ok_or_else(|| PyValueError::new_err("Tick缺少datetime"))?;
        let tick_dt = millis_to_shanghai(tick_ms)
            .ok_or_else(|| PyValueError::new_err("Tick datetime 转换失败"))?;
        let tick_minute_ms = tick_dt
            .with_second(0).unwrap()
            .with_nanosecond(0).unwrap()
            .timestamp_millis();

        let (volume_change, old_bar) = {
            let mut inner = self.inner.write().unwrap();

            let bar_minute_ms = inner.bar_millis.map(|bar_ms| {
                millis_to_shanghai(bar_ms).unwrap()
                    .with_second(0).unwrap()
                    .with_nanosecond(0).unwrap()
                    .timestamp_millis()
            });

            // 丢弃早于当前分钟的乱序Tick；它不能回退当前Bar或成交量基线。
            if let Some(bar_minute_ms) = bar_minute_ms {
                if tick_minute_ms < bar_minute_ms {
                    return Ok(());
                }
            }

            let new_minute = bar_minute_ms.is_none() || tick_minute_ms > bar_minute_ms.unwrap();
            let update_latest_state = new_minute || inner.bar_millis
                .map(|bar_ms| tick_ms >= bar_ms)
                .unwrap_or(true);
            let volume_change = if update_latest_state {
                inner.last_tick_volume
                    .map(|lv| (tick.volume - lv).max(0.0))
                    .unwrap_or(0.0)
            } else { 0.0 };
            let old_bar = if new_minute { inner.bar.take() } else { None };

            if new_minute {
                inner.bar_millis = None;
            } else if let Some(ref mut bar) = inner.bar {
                bar.high_price = bar.high_price.max(tick.last_price);
                bar.low_price = bar.low_price.min(tick.last_price);
                if update_latest_state {
                    bar.close_price = tick.last_price;
                    bar.datetime = tick.datetime.as_ref().map(|dt| dt.clone_ref(py));
                    bar.datetime_millis = Some(tick_ms);
                    inner.bar_millis = Some(tick_ms);
                }
            }

            // 只有时间不倒退的Tick才能更新成交量基线。
            if update_latest_state {
                inner.last_tick_volume = Some(tick.volume);
            }
            (volume_change, old_bar)
        };

        // 回调在锁外执行
        if let Some(bar_data) = old_bar {
            if let Some(ref callback) = self.on_bar {
                let trimmed = trim_bar_time(py, bar_data)?;
                callback.call1(py, (trimmed,)).map_err(|e|
                    PyValueError::new_err(format!("on_bar回调处理错误：{:#?}", e)))?;
            }
        }

        {
            let mut inner = self.inner.write().unwrap();
            let new_bar = inner.bar.is_none();
            if new_bar {
                inner.bar = Some(RustBarData {
                    symbol: tick.symbol.clone(), exchange: tick.exchange,
                    datetime: tick.datetime.as_ref().map(|dt| dt.clone_ref(py)),
                    interval: Some(RustInterval::MINUTE), volume: 0.0, open_interest: 0.0,
                    open_price: tick.last_price, high_price: tick.last_price,
                    low_price: tick.last_price, close_price: tick.last_price,
                    gateway_name: tick.gateway_name.clone(), vt_symbol: tick.vt_symbol.clone(),
                    datetime_millis: Some(tick_ms),
                });
                inner.bar_millis = Some(tick_ms);
            }
            if let Some(ref mut bar) = inner.bar {
                bar.open_interest = tick.open_interest;
                bar.volume += volume_change;
            }
        }
        Ok(())
    }

    /// 分钟 Bar 到目标窗口 Bar 的核心聚合逻辑。
    fn update_bar_internal(&self, py: Python, bar: RustBarData) -> PyResult<()> {
        let bar_ms = bar.get_millis(py)?
            .ok_or_else(|| PyValueError::new_err("Bar缺少datetime"))?;
        let bar_dt = millis_to_shanghai(bar_ms)
            .ok_or_else(|| PyValueError::new_err("Bar datetime 转换失败"))?;

        let (_last_dt_opt, window_bar_to_callback) = {
            let mut inner = self.inner.write().unwrap();

            let last_dt_opt = inner.last_bar_millis.and_then(millis_to_shanghai);

            // 初始化或更新 window_bar
            if inner.window_bar.is_none() {
                let dt = match self.interval {
                    RustInterval::MINUTE => bar_dt.with_second(0).unwrap().with_nanosecond(0).unwrap(),
                    RustInterval::HOUR => bar_dt.with_minute(0).unwrap().with_second(0).unwrap().with_nanosecond(0).unwrap(),
                    RustInterval::DAILY => (bar_dt + Duration::days(1)).date_naive().and_hms_opt(0,0,0).unwrap().and_local_timezone(*TZ_INFO).unwrap(),
                    RustInterval::WEEKLY => (bar_dt + Duration::weeks(1)).date_naive().and_hms_opt(0,0,0).unwrap().and_local_timezone(*TZ_INFO).unwrap(),
                    RustInterval::MONTHLY => {
                        let (y, m) = if bar_dt.month() == 12 { (bar_dt.year()+1, 1) } else { (bar_dt.year(), bar_dt.month()+1) };
                        match bar_dt.timezone().from_local_datetime(
                            &NaiveDate::from_ymd_opt(y,m,1).unwrap().and_hms_opt(0,0,0).unwrap()
                        ) { chrono::LocalResult::Single(t) => t, _ => bar_dt }
                    }
                    _ => bar_dt,
                };
                let py_dt = PyDateTime::new(py, dt.year(), dt.month() as u8, dt.day() as u8,
                    dt.hour() as u8, dt.minute() as u8, dt.second() as u8,
                    dt.nanosecond() / 1000, None)?;
                inner.window_bar = Some(RustBarData {
                    symbol: bar.symbol.clone(), exchange: bar.exchange,
                    datetime: Some(py_dt.into()), interval: Some(self.interval),
                    volume: 0.0, open_interest: bar.open_interest,
                    open_price: bar.open_price, high_price: bar.high_price,
                    low_price: bar.low_price, close_price: bar.close_price,
                    gateway_name: bar.gateway_name.clone(), vt_symbol: bar.vt_symbol.clone(),
                    datetime_millis: Some(dt.timestamp_millis()),
                });
            } else if let Some(ref mut wb) = inner.window_bar {
                wb.high_price = wb.high_price.max(bar.high_price);
                wb.low_price = wb.low_price.min(bar.low_price);
            }

            if let Some(ref mut wb) = inner.window_bar {
                wb.close_price = bar.close_price;
                wb.volume += bar.volume;
                wb.open_interest = bar.open_interest;
            }

            // 计算是否触发回调
            let now_value = self.get_interval_value_from_dt(&bar_dt);
            let mut finished = false;

            if let Some(ref last_dt) = last_dt_opt {
                let last_value = self.get_interval_value_from_dt(last_dt);
                if now_value != last_value {
                    // 判断是否使用目标时间点检查模式
                    let use_target = match self.interval {
                        RustInterval::MINUTE => self.interval_slice && 1440 % self.window == 0,
                        RustInterval::HOUR => self.interval_slice && 24 % self.window == 0,
                        RustInterval::DAILY => self.interval_slice && 7 % self.window == 0,
                        RustInterval::WEEKLY => self.interval_slice && 52 % self.window == 0,
                        RustInterval::MONTHLY => self.interval_slice && 12 % self.window == 0,
                        _ => self.interval_slice,
                    };
                    if use_target && self.check_target_value(now_value) {
                        finished = true;
                    } else if !use_target {
                        // 对于 DAILY/WEEKLY/MONTHLY 或不能整除的情况，使用计数器方式
                        // 每次日期值变化时递增计数器
                        inner.interval_count += 1;
                        // 当计数达到 window 时触发
                        if inner.interval_count % self.window == 0 { finished = true; }
                    }
                }
            }

            let wb_callback = if finished {
                let wb = inner.window_bar.take();
                inner.interval_count = 0;
                inner.bar_push_status.clear();
                wb
            } else { None };

            // 更新 last_bar 缓存
            inner.last_bar_millis = Some(bar_ms);
            inner.last_bar = Some(bar);

            (last_dt_opt, wb_callback)
        }; // 锁释放

        // 回调在锁外执行
        if let Some(wb) = window_bar_to_callback {
            if let Some(ref callback) = self.on_window_bar {
                callback.call1(py, (wb,)).map_err(|e|
                    PyValueError::new_err(format!("on_window_bar回调处理错误：{:#?}", e)))?;
            }
        }
        Ok(())
    }

    #[inline(always)]
    /// 根据当前目标周期提取用于聚合判断的离散时间值。
    fn get_interval_value_from_dt(&self, dt: &DateTime<chrono_tz::Tz>) -> u32 {
        match self.interval {
            RustInterval::MINUTE => {
                if self.interval_slice && self.window >= 60 {
                    dt.hour() * 60 + dt.minute()
                } else { dt.minute() }
            }
            RustInterval::HOUR => dt.hour(),
            RustInterval::DAILY => dt.day(),
            RustInterval::WEEKLY => dt.iso_week().week().min(52),
            RustInterval::MONTHLY => dt.month(),
            _ => 0,
        }
    }

    #[inline(always)]
    /// 判断当前离散时间值是否命中预定义的窗口结束点。
    fn check_target_value(&self, value: u32) -> bool {
        match self.interval {
            RustInterval::MINUTE => {
                if self.interval_slice && self.window >= 60 {
                    (value as usize) % self.window == 0
                } else { self.target_minutes.contains(value) }
            }
            RustInterval::HOUR => self.target_hours.contains(value),
            RustInterval::DAILY => self.target_days.contains(value),
            RustInterval::WEEKLY => self.target_weeks.contains(value),
            RustInterval::MONTHLY => self.target_months.contains(value),
            _ => false,
        }
    }
}

// ================================================================================================
// Python 模块定义
// ================================================================================================
/// Python 模块入口。
#[pymodule]
fn rust_bar_generator(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<RustInterval>()?;
    m.add_class::<RustExchange>()?;
    m.add_class::<RustBarData>()?;
    m.add_class::<RustTickData>()?;
    m.add_class::<BarGenerator>()?;
    m.add_function(wrap_pyfunction!(get_local_datetime, m)?)?;
    Ok(())
}
