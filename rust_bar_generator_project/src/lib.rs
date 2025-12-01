use chrono::{Datelike, Duration, Timelike, DateTime, NaiveDate, NaiveDateTime, TimeZone};
use chrono_tz::Asia::Shanghai;
use once_cell::sync::Lazy;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule, PyTuple, PyDateTime};
use regex::Regex;
use std::sync::RwLock;
use std::collections::{HashMap, HashSet};

// ================================================================================================
// 时区常量
// ================================================================================================
static TZ_INFO: Lazy<chrono_tz::Tz> = Lazy::new(|| Shanghai);

// ================================================================================================
// RustInterval 枚举 - 时间周期
// ================================================================================================
#[pyclass(eq, eq_int, module = "rust_bar_generator")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RustInterval {
    #[pyo3(name = "TICK")]
    TICK,
    #[pyo3(name = "MINUTE")]
    MINUTE,
    #[pyo3(name = "HOUR")]
    HOUR,
    #[pyo3(name = "DAILY")]
    DAILY,
    #[pyo3(name = "WEEKLY")]
    WEEKLY,
    #[pyo3(name = "MONTHLY")]
    MONTHLY,
}

#[pymethods]
impl RustInterval {
    fn __repr__(&self) -> String {
        format!("RustInterval.{:?}", self)
    }
    fn __str__(&self) -> &str {
        self.value()
    }
    #[getter]
    fn value(&self) -> &'static str {
        match self {
            RustInterval::TICK => "tick",
            RustInterval::MINUTE => "1m",
            RustInterval::HOUR => "1h",
            RustInterval::DAILY => "1d",
            RustInterval::WEEKLY => "1w",
            RustInterval::MONTHLY => "1M",
        }
    }
    fn __hash__(&self) -> isize {
        *self as isize
    }
}

impl RustInterval {
    fn from_py_any(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(ri) = obj.extract::<RustInterval>() {
            Ok(ri)
        } else if let Ok(s) = obj.extract::<String>() {
            Self::parse_string(&s)
        } else if let Ok(name_attr) = obj.getattr("name") {
            let s = name_attr.extract::<String>()?;
            Self::parse_string(&s)
        } else if let Ok(value_attr) = obj.getattr("value") {
            let s = value_attr.extract::<String>()?;
            Self::parse_string(&s)
        } else if let Ok(str_method) = obj.getattr("__str__") {
            let result = str_method.call0()?;
            let s = result.extract::<String>()?;
            Self::parse_string(&s)
        } else {
            Err(PyValueError::new_err("无法转换为 RustInterval"))
        }
    }

    fn parse_string(s: &str) -> PyResult<Self> {
        match s {
            "tick" => Ok(RustInterval::TICK),
            "TICK" => Ok(RustInterval::TICK),
            "1m" => Ok(RustInterval::MINUTE),
            "MINUTE" => Ok(RustInterval::MINUTE),
            "1h" => Ok(RustInterval::HOUR),
            "HOUR" => Ok(RustInterval::HOUR),
            "1d" => Ok(RustInterval::DAILY),
            "DAILY" => Ok(RustInterval::DAILY),
            "1w" => Ok(RustInterval::WEEKLY),
            "WEEKLY" => Ok(RustInterval::WEEKLY),
            "1M" => Ok(RustInterval::MONTHLY),
            "MONTHLY" => Ok(RustInterval::MONTHLY),
            _ => Err(PyValueError::new_err(format!("无法识别的时间间隔: {}", s))),
        }
    }
}

// ================================================================================================
// RustExchange 枚举 - 交易所
// ================================================================================================
#[pyclass(eq, eq_int, module = "rust_bar_generator")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RustExchange {
    // Chinese
    #[pyo3(name = "CFFEX")]
    CFFEX,
    #[pyo3(name = "SHFE")]
    SHFE,
    #[pyo3(name = "CZCE")]
    CZCE,
    #[pyo3(name = "DCE")]
    DCE,
    #[pyo3(name = "GFEX")]
    GFEX,
    #[pyo3(name = "INE")]
    INE,
    #[pyo3(name = "SSE")]
    SSE,
    #[pyo3(name = "SZSE")]
    SZSE,
    #[pyo3(name = "BSE")]
    BSE,
    #[pyo3(name = "SGE")]
    SGE,
    #[pyo3(name = "WXE")]
    WXE,
    #[pyo3(name = "CFETS")]
    CFETS,
    // Global
    #[pyo3(name = "SMART")]
    SMART,
    #[pyo3(name = "NYSE")]
    NYSE,
    #[pyo3(name = "NASDAQ")]
    NASDAQ,
    #[pyo3(name = "ARCA")]
    ARCA,
    #[pyo3(name = "EDGEA")]
    EDGEA,
    #[pyo3(name = "ISLAND")]
    ISLAND,
    #[pyo3(name = "BATS")]
    BATS,
    #[pyo3(name = "IEX")]
    IEX,
    #[pyo3(name = "NYMEX")]
    NYMEX,
    #[pyo3(name = "COMEX")]
    COMEX,
    #[pyo3(name = "GLOBEX")]
    GLOBEX,
    #[pyo3(name = "IDEALPRO")]
    IDEALPRO,
    #[pyo3(name = "CME")]
    CME,
    #[pyo3(name = "ICE")]
    ICE,
    #[pyo3(name = "SEHK")]
    SEHK,
    #[pyo3(name = "HKFE")]
    HKFE,
    #[pyo3(name = "HKSE")]
    HKSE,
    #[pyo3(name = "SGX")]
    SGX,
    #[pyo3(name = "CBOT")]
    CBOT,
    #[pyo3(name = "CBOE")]
    CBOE,
    #[pyo3(name = "CFE")]
    CFE,
    #[pyo3(name = "DME")]
    DME,
    #[pyo3(name = "EUREX")]
    EUREX,
    #[pyo3(name = "APEX")]
    APEX,
    #[pyo3(name = "LME")]
    LME,
    #[pyo3(name = "BMD")]
    BMD,
    #[pyo3(name = "TOCOM")]
    TOCOM,
    #[pyo3(name = "EUNX")]
    EUNX,
    #[pyo3(name = "KRX")]
    KRX,
    #[pyo3(name = "OTC")]
    OTC,
    #[pyo3(name = "IBKRATS")]
    IBKRATS,
    #[pyo3(name = "TSE")]
    TSE,
    #[pyo3(name = "AMEX")]
    AMEX,
    // 数字货币交易所
    #[pyo3(name = "BITMEX")]
    BITMEX,
    #[pyo3(name = "OKX")]
    OKX,
    #[pyo3(name = "HUOBI")]
    HUOBI,
    #[pyo3(name = "HUOBIP")]
    HUOBIP,
    #[pyo3(name = "HUOBIM")]
    HUOBIM,
    #[pyo3(name = "HUOBIF")]
    HUOBIF,
    #[pyo3(name = "HUOBISWAP")]
    HUOBISWAP,
    #[pyo3(name = "BITGETS")]
    BITGETS,
    #[pyo3(name = "BITFINEX")]
    BITFINEX,
    #[pyo3(name = "BITHUMB")]
    BITHUMB,
    #[pyo3(name = "BINANCE")]
    BINANCE,
    #[pyo3(name = "BINANCEF")]
    BINANCEF,
    #[pyo3(name = "BINANCES")]
    BINANCES,
    #[pyo3(name = "COINBASE")]
    COINBASE,
    #[pyo3(name = "BYBIT")]
    BYBIT,
    #[pyo3(name = "BYBITSPOT")]
    BYBITSPOT,
    #[pyo3(name = "KRAKEN")]
    KRAKEN,
    #[pyo3(name = "DERIBIT")]
    DERIBIT,
    #[pyo3(name = "GATEIO")]
    GATEIO,
    #[pyo3(name = "BITSTAMP")]
    BITSTAMP,
    #[pyo3(name = "BINGXS")]
    BINGXS,
    #[pyo3(name = "ORANGEX")]
    ORANGEX,
    #[pyo3(name = "KUCOIN")]
    KUCOIN,
    #[pyo3(name = "DYDX")]
    DYDX,
    #[pyo3(name = "HYPE")]
    HYPE,
    #[pyo3(name = "HYPESPOT")]
    HYPESPOT,
    #[pyo3(name = "LOCAL")]
    LOCAL,
}

#[pymethods]
impl RustExchange {
    fn __repr__(&self) -> String {
        format!("RustExchange.{:?}", self)
    }
    fn __str__(&self) -> &str {
        self.value()
    }
    #[getter]
    fn value(&self) -> &'static str {
        match self {
            // Chinese
            RustExchange::CFFEX => "CFFEX",
            RustExchange::SHFE => "SHFE",
            RustExchange::CZCE => "CZCE",
            RustExchange::DCE => "DCE",
            RustExchange::GFEX => "GFEX",
            RustExchange::INE => "INE",
            RustExchange::SSE => "SSE",
            RustExchange::SZSE => "SZSE",
            RustExchange::BSE => "BSE",
            RustExchange::SGE => "SGE",
            RustExchange::WXE => "WXE",
            RustExchange::CFETS => "CFETS",
            // Global
            RustExchange::SMART => "SMART",
            RustExchange::NYSE => "NYSE",
            RustExchange::NASDAQ => "NASDAQ",
            RustExchange::ARCA => "ARCA",
            RustExchange::EDGEA => "EDGEA",
            RustExchange::ISLAND => "ISLAND",
            RustExchange::BATS => "BATS",
            RustExchange::IEX => "IEX",
            RustExchange::NYMEX => "NYMEX",
            RustExchange::COMEX => "COMEX",
            RustExchange::GLOBEX => "GLOBEX",
            RustExchange::IDEALPRO => "IDEALPRO",
            RustExchange::CME => "CME",
            RustExchange::ICE => "ICE",
            RustExchange::SEHK => "SEHK",
            RustExchange::HKFE => "HKFE",
            RustExchange::HKSE => "HKSE",
            RustExchange::SGX => "SGX",
            RustExchange::CBOT => "CBT",
            RustExchange::CBOE => "CBOE",
            RustExchange::CFE => "CFE",
            RustExchange::DME => "DME",
            RustExchange::EUREX => "EUX",
            RustExchange::APEX => "APEX",
            RustExchange::LME => "LME",
            RustExchange::BMD => "BMD",
            RustExchange::TOCOM => "TOCOM",
            RustExchange::EUNX => "EUNX",
            RustExchange::KRX => "KRX",
            RustExchange::OTC => "PINK",
            RustExchange::IBKRATS => "IBKRATS",
            RustExchange::TSE => "TSE",
            RustExchange::AMEX => "AMEX",
            // 数字货币交易所
            RustExchange::BITMEX => "BITMEX",
            RustExchange::OKX => "OKX",
            RustExchange::HUOBI => "HUOBI",
            RustExchange::HUOBIP => "HUOBIP",
            RustExchange::HUOBIM => "HUOBIM",
            RustExchange::HUOBIF => "HUOBIF",
            RustExchange::HUOBISWAP => "HUOBISWAP",
            RustExchange::BITGETS => "BITGETS",
            RustExchange::BITFINEX => "BITFINEX",
            RustExchange::BITHUMB => "BITHUMB",
            RustExchange::BINANCE => "BINANCE",
            RustExchange::BINANCEF => "BINANCEF",
            RustExchange::BINANCES => "BINANCES",
            RustExchange::COINBASE => "COINBASE",
            RustExchange::BYBIT => "BYBIT",
            RustExchange::BYBITSPOT => "BYBITSPOT",
            RustExchange::KRAKEN => "KRAKEN",
            RustExchange::DERIBIT => "DERIBIT",
            RustExchange::GATEIO => "GATEIO",
            RustExchange::BITSTAMP => "BITSTAMP",
            RustExchange::BINGXS => "BINGXS",
            RustExchange::ORANGEX => "ORANGEX",
            RustExchange::KUCOIN => "KUCOIN",
            RustExchange::DYDX => "DYDX",
            RustExchange::HYPE => "HYPE",
            RustExchange::HYPESPOT => "HYPESPOT",
            RustExchange::LOCAL => "LOCAL",
        }
    }
}

impl RustExchange {
    fn from_py_any(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(re) = obj.extract::<RustExchange>() {
            Ok(re)
        } else if let Ok(s) = obj.extract::<String>() {
            Self::parse_string(&s)
        } else if let Ok(name_attr) = obj.getattr("name") {
            let s = name_attr.extract::<String>()?;
            Self::parse_string(&s)
        } else if let Ok(value_attr) = obj.getattr("value") {
            let s = value_attr.extract::<String>()?;
            Self::parse_string(&s)
        } else if let Ok(str_method) = obj.getattr("__str__") {
            let result = str_method.call0()?;
            let s = result.extract::<String>()?;
            Self::parse_string(&s)
        } else {
            Err(PyValueError::new_err("无法转换为 RustExchange"))
        }
    }

    fn parse_string(s: &str) -> PyResult<Self> {
        match s.to_uppercase().as_str() {
            // Chinese
            "CFFEX" => Ok(RustExchange::CFFEX),
            "SHFE" => Ok(RustExchange::SHFE),
            "CZCE" => Ok(RustExchange::CZCE),
            "DCE" => Ok(RustExchange::DCE),
            "GFEX" => Ok(RustExchange::GFEX),
            "INE" => Ok(RustExchange::INE),
            "SSE" => Ok(RustExchange::SSE),
            "SZSE" => Ok(RustExchange::SZSE),
            "BSE" => Ok(RustExchange::BSE),
            "SGE" => Ok(RustExchange::SGE),
            "WXE" => Ok(RustExchange::WXE),
            "CFETS" => Ok(RustExchange::CFETS),
            // Global
            "SMART" => Ok(RustExchange::SMART),
            "NYSE" => Ok(RustExchange::NYSE),
            "NASDAQ" => Ok(RustExchange::NASDAQ),
            "ARCA" => Ok(RustExchange::ARCA),
            "EDGEA" => Ok(RustExchange::EDGEA),
            "ISLAND" => Ok(RustExchange::ISLAND),
            "BATS" => Ok(RustExchange::BATS),
            "IEX" => Ok(RustExchange::IEX),
            "NYMEX" => Ok(RustExchange::NYMEX),
            "COMEX" => Ok(RustExchange::COMEX),
            "GLOBEX" => Ok(RustExchange::GLOBEX),
            "IDEALPRO" => Ok(RustExchange::IDEALPRO),
            "CME" => Ok(RustExchange::CME),
            "ICE" => Ok(RustExchange::ICE),
            "SEHK" => Ok(RustExchange::SEHK),
            "HKFE" => Ok(RustExchange::HKFE),
            "HKSE" => Ok(RustExchange::HKSE),
            "SGX" => Ok(RustExchange::SGX),
            "CBOT" | "CBT" => Ok(RustExchange::CBOT),
            "CBOE" => Ok(RustExchange::CBOE),
            "CFE" => Ok(RustExchange::CFE),
            "DME" => Ok(RustExchange::DME),
            "EUREX" | "EUX" => Ok(RustExchange::EUREX),
            "APEX" => Ok(RustExchange::APEX),
            "LME" => Ok(RustExchange::LME),
            "BMD" => Ok(RustExchange::BMD),
            "TOCOM" => Ok(RustExchange::TOCOM),
            "EUNX" => Ok(RustExchange::EUNX),
            "KRX" => Ok(RustExchange::KRX),
            "OTC" | "PINK" => Ok(RustExchange::OTC),
            "IBKRATS" => Ok(RustExchange::IBKRATS),
            "TSE" => Ok(RustExchange::TSE),
            "AMEX" => Ok(RustExchange::AMEX),
            // 数字货币交易所
            "BITMEX" => Ok(RustExchange::BITMEX),
            "OKX" => Ok(RustExchange::OKX),
            "HUOBI" => Ok(RustExchange::HUOBI),
            "HUOBIP" => Ok(RustExchange::HUOBIP),
            "HUOBIM" => Ok(RustExchange::HUOBIM),
            "HUOBIF" => Ok(RustExchange::HUOBIF),
            "HUOBISWAP" => Ok(RustExchange::HUOBISWAP),
            "BITGETS" => Ok(RustExchange::BITGETS),
            "BITFINEX" => Ok(RustExchange::BITFINEX),
            "BITHUMB" => Ok(RustExchange::BITHUMB),
            "BINANCE" => Ok(RustExchange::BINANCE),
            "BINANCEF" => Ok(RustExchange::BINANCEF),
            "BINANCES" => Ok(RustExchange::BINANCES),
            "COINBASE" => Ok(RustExchange::COINBASE),
            "BYBIT" => Ok(RustExchange::BYBIT),
            "BYBITSPOT" => Ok(RustExchange::BYBITSPOT),
            "KRAKEN" => Ok(RustExchange::KRAKEN),
            "DERIBIT" => Ok(RustExchange::DERIBIT),
            "GATEIO" => Ok(RustExchange::GATEIO),
            "BITSTAMP" => Ok(RustExchange::BITSTAMP),
            "BINGXS" => Ok(RustExchange::BINGXS),
            "ORANGEX" => Ok(RustExchange::ORANGEX),
            "KUCOIN" => Ok(RustExchange::KUCOIN),
            "DYDX" => Ok(RustExchange::DYDX),
            "HYPE" => Ok(RustExchange::HYPE),
            "HYPESPOT" => Ok(RustExchange::HYPESPOT),
            "LOCAL" => Ok(RustExchange::LOCAL),
            _ => Err(PyValueError::new_err(format!("无法识别的交易所: {}", s))),
        }
    }
}

// ================================================================================================
// RustBarData - K线数据结构
// ================================================================================================
#[pyclass(module = "rust_bar_generator")]
#[derive(Debug)]
pub struct RustBarData {
    #[pyo3(get, set)]
    pub symbol: String,
    #[pyo3(get, set)]
    pub exchange: RustExchange,
    #[pyo3(get, set)]
    pub datetime: Option<Py<PyAny>>,
    #[pyo3(get, set)]
    pub interval: Option<RustInterval>,
    #[pyo3(get, set)]
    pub volume: f64,
    #[pyo3(get, set)]
    pub open_interest: f64,
    #[pyo3(get, set)]
    pub open_price: f64,
    #[pyo3(get, set)]
    pub high_price: f64,
    #[pyo3(get, set)]
    pub low_price: f64,
    #[pyo3(get, set)]
    pub close_price: f64,
    #[pyo3(get, set)]
    pub gateway_name: String,
    #[pyo3(get, set)]
    pub vt_symbol: String,
}

impl Clone for RustBarData {
    fn clone(&self) -> Self {
        Python::attach(|py| {
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
            }
        })
    }
}

impl RustBarData {
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
        }
    }

    fn get_datetime_chrono(&self, py: Python) -> PyResult<Option<DateTime<chrono_tz::Tz>>> {
        if let Some(ref dt_obj) = self.datetime {
            let dt_bound = dt_obj.bind(py);
            let ts_method = dt_bound.call_method0("timestamp")?;
            let ts_seconds = ts_method.extract::<f64>()?;
            let ts_millis = (ts_seconds * 1000.0) as i64;
            
            Ok(DateTime::from_timestamp_millis(ts_millis)
                .map(|dt| dt.with_timezone(&*TZ_INFO)))
        } else {
            Ok(None)
        }
    }

    fn from_py_bar(_py: Python, py_bar: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(rust_bar) = py_bar.extract::<RustBarData>() {
            return Ok(rust_bar);
        }

        let symbol = py_bar.getattr("symbol")?.extract::<String>()?;
        let gateway_name = py_bar.getattr("gateway_name")?.extract::<String>()?;
        
        let exchange_obj = py_bar.getattr("exchange")?;
        let exchange = RustExchange::from_py_any(&exchange_obj)?;

        let datetime = if let Ok(dt_attr) = py_bar.getattr("datetime") {
            Some(dt_attr.unbind())
        } else {
            None
        };

        let interval = if let Ok(interval_obj) = py_bar.getattr("interval") {
            Some(RustInterval::from_py_any(&interval_obj)?)
        } else {
            None
        };

        let volume = py_bar.getattr("volume")?.extract::<f64>().unwrap_or(0.0);
        let open_interest = py_bar.getattr("open_interest")?.extract::<f64>().unwrap_or(0.0);
        let open_price = py_bar.getattr("open_price")?.extract::<f64>().unwrap_or(0.0);
        let high_price = py_bar.getattr("high_price")?.extract::<f64>().unwrap_or(0.0);
        let low_price = py_bar.getattr("low_price")?.extract::<f64>().unwrap_or(0.0);
        let close_price = py_bar.getattr("close_price")?.extract::<f64>().unwrap_or(0.0);

        let vt_symbol = format!("{}_{}/{}", symbol, exchange.__str__(), gateway_name);

        Ok(RustBarData {
            symbol,
            exchange,
            datetime,
            interval,
            volume,
            open_interest,
            open_price,
            high_price,
            low_price,
            close_price,
            gateway_name,
            vt_symbol,
        })
    }
}

#[pymethods]
impl RustBarData {
    #[new]
    #[pyo3(signature = (symbol, exchange, gateway_name, datetime=None, interval=None, volume=0.0, open_interest=0.0, open_price=0.0, high_price=0.0, low_price=0.0, close_price=0.0))]
    fn new(
        _py: Python,
        symbol: String,
        exchange: &Bound<'_, PyAny>,
        gateway_name: String,
        datetime: Option<&Bound<'_, PyAny>>,
        interval: Option<&Bound<'_, PyAny>>,
        volume: f64,
        open_interest: f64,
        open_price: f64,
        high_price: f64,
        low_price: f64,
        close_price: f64,
    ) -> PyResult<Self> {
        let rust_exchange = RustExchange::from_py_any(exchange)?;
        let rust_interval = if let Some(iv) = interval {
            Some(RustInterval::from_py_any(iv)?)
        } else {
            None
        };

        let py_datetime = datetime.map(|dt| dt.clone().unbind());

        let vt_symbol = format!("{}_{}/{}", symbol, rust_exchange.__str__(), gateway_name);
        
        Ok(RustBarData {
            symbol,
            exchange: rust_exchange,
            datetime: py_datetime,
            interval: rust_interval,
            volume,
            open_interest,
            open_price,
            high_price,
            low_price,
            close_price,
            gateway_name,
            vt_symbol,
        })
    }

    fn __reduce__<'py>(&self, py: Python<'py>) -> PyResult<(Py<PyAny>, Py<PyAny>)> {
        let cls = PyModule::import(py, "rust_bar_generator")?.getattr("RustBarData")?;
        
        let exchange_str = self.exchange.__str__();
        let interval_str: Option<&str> = self.interval.map(|i| match i {
            RustInterval::TICK => "TICK",
            RustInterval::MINUTE => "MINUTE",
            RustInterval::HOUR => "HOUR",
            RustInterval::DAILY => "DAILY",
            RustInterval::WEEKLY => "WEEKLY",
            RustInterval::MONTHLY => "MONTHLY",
        });
        
        let dt_for_pickle = self.datetime.as_ref().map(|dt| dt.clone_ref(py));
        
        let args = PyTuple::new(py, &[
            self.symbol.clone().into_pyobject(py)?.into_any().unbind(),
            exchange_str.into_pyobject(py)?.into_any().unbind(),
            self.gateway_name.clone().into_pyobject(py)?.into_any().unbind(),
            dt_for_pickle.into_pyobject(py)?.into_any().unbind(),
            interval_str.into_pyobject(py)?.into_any().unbind(),
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
        format!(
            "RustBarData(symbol='{}', exchange={:?}, datetime={:?}, interval={:?})",
            self.symbol, self.exchange, self.datetime, self.interval
        )
    }
}

// ================================================================================================
// RustTickData - Tick数据结构
// ================================================================================================
#[pyclass(module = "rust_bar_generator")]
#[derive(Debug)]
pub struct RustTickData {
    #[pyo3(get, set)]
    pub symbol: String,
    #[pyo3(get, set)]
    pub exchange: RustExchange,
    #[pyo3(get, set)]
    pub datetime: Option<Py<PyAny>>,
    #[pyo3(get, set)]
    pub name: String,
    #[pyo3(get, set)]
    pub volume: f64,
    #[pyo3(get, set)]
    pub open_interest: f64,
    #[pyo3(get, set)]
    pub last_price: f64,
    #[pyo3(get, set)]
    pub last_volume: f64,
    #[pyo3(get, set)]
    pub limit_up: f64,
    #[pyo3(get, set)]
    pub limit_down: f64,
    #[pyo3(get, set)]
    pub open_price: f64,
    #[pyo3(get, set)]
    pub high_price: f64,
    #[pyo3(get, set)]
    pub low_price: f64,
    #[pyo3(get, set)]
    pub pre_close: f64,
    #[pyo3(get, set)]
    pub bid_price_1: f64,
    #[pyo3(get, set)]
    pub bid_price_2: f64,
    #[pyo3(get, set)]
    pub bid_price_3: f64,
    #[pyo3(get, set)]
    pub bid_price_4: f64,
    #[pyo3(get, set)]
    pub bid_price_5: f64,
    #[pyo3(get, set)]
    pub ask_price_1: f64,
    #[pyo3(get, set)]
    pub ask_price_2: f64,
    #[pyo3(get, set)]
    pub ask_price_3: f64,
    #[pyo3(get, set)]
    pub ask_price_4: f64,
    #[pyo3(get, set)]
    pub ask_price_5: f64,
    #[pyo3(get, set)]
    pub bid_volume_1: f64,
    #[pyo3(get, set)]
    pub bid_volume_2: f64,
    #[pyo3(get, set)]
    pub bid_volume_3: f64,
    #[pyo3(get, set)]
    pub bid_volume_4: f64,
    #[pyo3(get, set)]
    pub bid_volume_5: f64,
    #[pyo3(get, set)]
    pub ask_volume_1: f64,
    #[pyo3(get, set)]
    pub ask_volume_2: f64,
    #[pyo3(get, set)]
    pub ask_volume_3: f64,
    #[pyo3(get, set)]
    pub ask_volume_4: f64,
    #[pyo3(get, set)]
    pub ask_volume_5: f64,
    #[pyo3(get, set)]
    pub gateway_name: String,
    #[pyo3(get, set)]
    pub vt_symbol: String,
}

impl Clone for RustTickData {
    fn clone(&self) -> Self {
        Python::attach(|py| self.clone_with_py(py))
    }
}

impl RustTickData {
    fn clone_with_py(&self, py: Python) -> Self {
        RustTickData {
            symbol: self.symbol.clone(),
            exchange: self.exchange,
            datetime: self.datetime.as_ref().map(|dt| dt.clone_ref(py)),
            name: self.name.clone(),
            volume: self.volume,
            open_interest: self.open_interest,
            last_price: self.last_price,
            last_volume: self.last_volume,
            limit_up: self.limit_up,
            limit_down: self.limit_down,
            open_price: self.open_price,
            high_price: self.high_price,
            low_price: self.low_price,
            pre_close: self.pre_close,
            bid_price_1: self.bid_price_1,
            bid_price_2: self.bid_price_2,
            bid_price_3: self.bid_price_3,
            bid_price_4: self.bid_price_4,
            bid_price_5: self.bid_price_5,
            ask_price_1: self.ask_price_1,
            ask_price_2: self.ask_price_2,
            ask_price_3: self.ask_price_3,
            ask_price_4: self.ask_price_4,
            ask_price_5: self.ask_price_5,
            bid_volume_1: self.bid_volume_1,
            bid_volume_2: self.bid_volume_2,
            bid_volume_3: self.bid_volume_3,
            bid_volume_4: self.bid_volume_4,
            bid_volume_5: self.bid_volume_5,
            ask_volume_1: self.ask_volume_1,
            ask_volume_2: self.ask_volume_2,
            ask_volume_3: self.ask_volume_3,
            ask_volume_4: self.ask_volume_4,
            ask_volume_5: self.ask_volume_5,
            gateway_name: self.gateway_name.clone(),
            vt_symbol: self.vt_symbol.clone(),
        }
    }

    fn get_datetime_chrono(&self, py: Python) -> PyResult<Option<DateTime<chrono_tz::Tz>>> {
        if let Some(ref dt_obj) = self.datetime {
            let dt_bound = dt_obj.bind(py);
            let ts_method = dt_bound.call_method0("timestamp")?;
            let ts_seconds = ts_method.extract::<f64>()?;
            let ts_millis = (ts_seconds * 1000.0) as i64;
            
            Ok(DateTime::from_timestamp_millis(ts_millis)
                .map(|dt| dt.with_timezone(&*TZ_INFO)))
        } else {
            Ok(None)
        }
    }

    fn from_py_tick(_py: Python, py_tick: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(rust_tick) = py_tick.extract::<RustTickData>() {
            return Ok(rust_tick);
        }

        let symbol = py_tick.getattr("symbol")?.extract::<String>()?;
        let gateway_name = py_tick.getattr("gateway_name")?.extract::<String>()?;
        
        let exchange_obj = py_tick.getattr("exchange")?;
        let exchange = RustExchange::from_py_any(&exchange_obj)?;

        let datetime = if let Ok(dt_attr) = py_tick.getattr("datetime") {
            Some(dt_attr.unbind())
        } else {
            None
        };

        let name = py_tick.getattr("name")?.extract::<String>().unwrap_or_default();
        let volume = py_tick.getattr("volume")?.extract::<f64>().unwrap_or(0.0);
        let open_interest = py_tick.getattr("open_interest")?.extract::<f64>().unwrap_or(0.0);
        let last_price = py_tick.getattr("last_price")?.extract::<f64>().unwrap_or(0.0);
        let last_volume = py_tick.getattr("last_volume")?.extract::<f64>().unwrap_or(0.0);
        let limit_up = py_tick.getattr("limit_up")?.extract::<f64>().unwrap_or(0.0);
        let limit_down = py_tick.getattr("limit_down")?.extract::<f64>().unwrap_or(0.0);
        let open_price = py_tick.getattr("open_price")?.extract::<f64>().unwrap_or(0.0);
        let high_price = py_tick.getattr("high_price")?.extract::<f64>().unwrap_or(0.0);
        let low_price = py_tick.getattr("low_price")?.extract::<f64>().unwrap_or(0.0);
        let pre_close = py_tick.getattr("pre_close")?.extract::<f64>().unwrap_or(0.0);
        
        let bid_price_1 = py_tick.getattr("bid_price_1")?.extract::<f64>().unwrap_or(0.0);
        let bid_price_2 = py_tick.getattr("bid_price_2")?.extract::<f64>().unwrap_or(0.0);
        let bid_price_3 = py_tick.getattr("bid_price_3")?.extract::<f64>().unwrap_or(0.0);
        let bid_price_4 = py_tick.getattr("bid_price_4")?.extract::<f64>().unwrap_or(0.0);
        let bid_price_5 = py_tick.getattr("bid_price_5")?.extract::<f64>().unwrap_or(0.0);
        
        let ask_price_1 = py_tick.getattr("ask_price_1")?.extract::<f64>().unwrap_or(0.0);
        let ask_price_2 = py_tick.getattr("ask_price_2")?.extract::<f64>().unwrap_or(0.0);
        let ask_price_3 = py_tick.getattr("ask_price_3")?.extract::<f64>().unwrap_or(0.0);
        let ask_price_4 = py_tick.getattr("ask_price_4")?.extract::<f64>().unwrap_or(0.0);
        let ask_price_5 = py_tick.getattr("ask_price_5")?.extract::<f64>().unwrap_or(0.0);
        
        let bid_volume_1 = py_tick.getattr("bid_volume_1")?.extract::<f64>().unwrap_or(0.0);
        let bid_volume_2 = py_tick.getattr("bid_volume_2")?.extract::<f64>().unwrap_or(0.0);
        let bid_volume_3 = py_tick.getattr("bid_volume_3")?.extract::<f64>().unwrap_or(0.0);
        let bid_volume_4 = py_tick.getattr("bid_volume_4")?.extract::<f64>().unwrap_or(0.0);
        let bid_volume_5 = py_tick.getattr("bid_volume_5")?.extract::<f64>().unwrap_or(0.0);
        
        let ask_volume_1 = py_tick.getattr("ask_volume_1")?.extract::<f64>().unwrap_or(0.0);
        let ask_volume_2 = py_tick.getattr("ask_volume_2")?.extract::<f64>().unwrap_or(0.0);
        let ask_volume_3 = py_tick.getattr("ask_volume_3")?.extract::<f64>().unwrap_or(0.0);
        let ask_volume_4 = py_tick.getattr("ask_volume_4")?.extract::<f64>().unwrap_or(0.0);
        let ask_volume_5 = py_tick.getattr("ask_volume_5")?.extract::<f64>().unwrap_or(0.0);

        let vt_symbol = format!("{}_{}/{}", symbol, exchange.__str__(), gateway_name);

        Ok(RustTickData {
            symbol,
            exchange,
            datetime,
            name,
            volume,
            open_interest,
            last_price,
            last_volume,
            limit_up,
            limit_down,
            open_price,
            high_price,
            low_price,
            pre_close,
            bid_price_1,
            bid_price_2,
            bid_price_3,
            bid_price_4,
            bid_price_5,
            ask_price_1,
            ask_price_2,
            ask_price_3,
            ask_price_4,
            ask_price_5,
            bid_volume_1,
            bid_volume_2,
            bid_volume_3,
            bid_volume_4,
            bid_volume_5,
            ask_volume_1,
            ask_volume_2,
            ask_volume_3,
            ask_volume_4,
            ask_volume_5,
            gateway_name,
            vt_symbol,
        })
    }
}

#[pymethods]
impl RustTickData {
    #[new]
    #[pyo3(signature = (symbol, exchange, gateway_name, datetime=None, **kwargs))]
    fn new(
        _py: Python,
        symbol: String,
        exchange: &Bound<'_, PyAny>,
        gateway_name: String,
        datetime: Option<&Bound<'_, PyAny>>,
        kwargs: Option<Bound<'_, PyDict>>,
    ) -> PyResult<Self> {
        let rust_exchange = RustExchange::from_py_any(exchange)?;
        let vt_symbol = format!("{}_{}/{}", symbol, rust_exchange.__str__(), gateway_name);
        
        let py_datetime = datetime.map(|dt| dt.clone().unbind());
        
        let mut tick = RustTickData {
            symbol,
            exchange: rust_exchange,
            datetime: py_datetime,
            name: String::new(),
            volume: 0.0,
            open_interest: 0.0,
            last_price: 0.0,
            last_volume: 0.0,
            limit_up: 0.0,
            limit_down: 0.0,
            open_price: 0.0,
            high_price: 0.0,
            low_price: 0.0,
            pre_close: 0.0,
            bid_price_1: 0.0,
            bid_price_2: 0.0,
            bid_price_3: 0.0,
            bid_price_4: 0.0,
            bid_price_5: 0.0,
            ask_price_1: 0.0,
            ask_price_2: 0.0,
            ask_price_3: 0.0,
            ask_price_4: 0.0,
            ask_price_5: 0.0,
            bid_volume_1: 0.0,
            bid_volume_2: 0.0,
            bid_volume_3: 0.0,
            bid_volume_4: 0.0,
            bid_volume_5: 0.0,
            ask_volume_1: 0.0,
            ask_volume_2: 0.0,
            ask_volume_3: 0.0,
            ask_volume_4: 0.0,
            ask_volume_5: 0.0,
            gateway_name,
            vt_symbol,
        };

        if let Some(kw) = kwargs {
            if let Ok(Some(val)) = kw.get_item("name") {
                tick.name = val.extract().unwrap_or_default();
            }
            if let Ok(Some(val)) = kw.get_item("volume") {
                tick.volume = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("open_interest") {
                tick.open_interest = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("last_price") {
                tick.last_price = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("last_volume") {
                tick.last_volume = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("limit_up") {
                tick.limit_up = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("limit_down") {
                tick.limit_down = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("open_price") {
                tick.open_price = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("high_price") {
                tick.high_price = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("low_price") {
                tick.low_price = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("pre_close") {
                tick.pre_close = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("bid_price_1") {
                tick.bid_price_1 = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("bid_price_2") {
                tick.bid_price_2 = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("bid_price_3") {
                tick.bid_price_3 = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("bid_price_4") {
                tick.bid_price_4 = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("bid_price_5") {
                tick.bid_price_5 = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("ask_price_1") {
                tick.ask_price_1 = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("ask_price_2") {
                tick.ask_price_2 = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("ask_price_3") {
                tick.ask_price_3 = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("ask_price_4") {
                tick.ask_price_4 = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("ask_price_5") {
                tick.ask_price_5 = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("bid_volume_1") {
                tick.bid_volume_1 = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("bid_volume_2") {
                tick.bid_volume_2 = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("bid_volume_3") {
                tick.bid_volume_3 = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("bid_volume_4") {
                tick.bid_volume_4 = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("bid_volume_5") {
                tick.bid_volume_5 = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("ask_volume_1") {
                tick.ask_volume_1 = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("ask_volume_2") {
                tick.ask_volume_2 = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("ask_volume_3") {
                tick.ask_volume_3 = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("ask_volume_4") {
                tick.ask_volume_4 = val.extract().unwrap_or(0.0);
            }
            if let Ok(Some(val)) = kw.get_item("ask_volume_5") {
                tick.ask_volume_5 = val.extract().unwrap_or(0.0);
            }
        }

        Ok(tick)
    }

    fn __reduce__<'py>(&self, py: Python<'py>) -> PyResult<(Py<PyAny>, Py<PyAny>, Py<PyAny>)> {
        let cls = PyModule::import(py, "rust_bar_generator")?.getattr("RustTickData")?;
        
        let exchange_str = self.exchange.__str__();
        
        let dt_for_pickle = self.datetime.as_ref().map(|dt| dt.clone_ref(py));
        
        let args = PyTuple::new(py, &[
            self.symbol.clone().into_pyobject(py)?.into_any().unbind(),
            exchange_str.into_pyobject(py)?.into_any().unbind(),
            self.gateway_name.clone().into_pyobject(py)?.into_any().unbind(),
            dt_for_pickle.into_pyobject(py)?.into_any().unbind(),
        ])?;
        
        let kwargs = PyDict::new(py);
        kwargs.set_item("name", &self.name)?;
        kwargs.set_item("volume", self.volume)?;
        kwargs.set_item("open_interest", self.open_interest)?;
        kwargs.set_item("last_price", self.last_price)?;
        kwargs.set_item("last_volume", self.last_volume)?;
        kwargs.set_item("limit_up", self.limit_up)?;
        kwargs.set_item("limit_down", self.limit_down)?;
        kwargs.set_item("open_price", self.open_price)?;
        kwargs.set_item("high_price", self.high_price)?;
        kwargs.set_item("low_price", self.low_price)?;
        kwargs.set_item("pre_close", self.pre_close)?;
        kwargs.set_item("bid_price_1", self.bid_price_1)?;
        kwargs.set_item("bid_price_2", self.bid_price_2)?;
        kwargs.set_item("bid_price_3", self.bid_price_3)?;
        kwargs.set_item("bid_price_4", self.bid_price_4)?;
        kwargs.set_item("bid_price_5", self.bid_price_5)?;
        kwargs.set_item("ask_price_1", self.ask_price_1)?;
        kwargs.set_item("ask_price_2", self.ask_price_2)?;
        kwargs.set_item("ask_price_3", self.ask_price_3)?;
        kwargs.set_item("ask_price_4", self.ask_price_4)?;
        kwargs.set_item("ask_price_5", self.ask_price_5)?;
        kwargs.set_item("bid_volume_1", self.bid_volume_1)?;
        kwargs.set_item("bid_volume_2", self.bid_volume_2)?;
        kwargs.set_item("bid_volume_3", self.bid_volume_3)?;
        kwargs.set_item("bid_volume_4", self.bid_volume_4)?;
        kwargs.set_item("bid_volume_5", self.bid_volume_5)?;
        kwargs.set_item("ask_volume_1", self.ask_volume_1)?;
        kwargs.set_item("ask_volume_2", self.ask_volume_2)?;
        kwargs.set_item("ask_volume_3", self.ask_volume_3)?;
        kwargs.set_item("ask_volume_4", self.ask_volume_4)?;
        kwargs.set_item("ask_volume_5", self.ask_volume_5)?;
        
        Ok((cls.unbind(), args.unbind().into(), kwargs.unbind().into()))
    }

    fn __repr__(&self) -> String {
        format!(
            "RustTickData(symbol='{}', exchange={:?}, datetime={:?}, last_price={})",
            self.symbol, self.exchange, self.datetime, self.last_price
        )
    }
}

// ================================================================================================
// 时间解析函数
// ================================================================================================

fn parse_str_timestamp(timestamp: &str) -> PyResult<NaiveDateTime> {
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[+Z]").unwrap());
    
    let cleaned = RE.split(timestamp).next().unwrap_or("").trim();
    
    let format = if cleaned.contains('-') {
        if cleaned.contains('T') {
            if cleaned.contains('.') {
                "%Y-%m-%dT%H:%M:%S%.f"
            } else {
                "%Y-%m-%dT%H:%M:%S"
            }
        } else if cleaned.contains('.') {
            "%Y-%m-%d %H:%M:%S%.f"
        } else {
            "%Y-%m-%d %H:%M:%S"
        }
    } else if cleaned.contains('.') {
        "%Y%m%d %H:%M:%S%.f"
    } else {
        "%Y%m%d %H:%M:%S"
    };

    NaiveDateTime::parse_from_str(cleaned, format)
        .map_err(|e| PyValueError::new_err(format!("时间解析失败: {}", e)))
}

fn parse_numeric_timestamp(timestamp: i64) -> PyResult<NaiveDateTime> {
    let dt = if timestamp > 1_000_000_000_000_000_000 {
        DateTime::from_timestamp(timestamp / 1_000_000_000, (timestamp % 1_000_000_000) as u32)
    } else if timestamp > 1_000_000_000_000_000 {
        DateTime::from_timestamp(timestamp / 1_000_000, ((timestamp % 1_000_000) * 1000) as u32)
    } else if timestamp > 1_000_000_000_000 {
        DateTime::from_timestamp(timestamp / 1000, ((timestamp % 1000) * 1_000_000) as u32)
    } else {
        DateTime::from_timestamp(timestamp, 0)
    };

    dt.map(|d| d.naive_utc())
        .ok_or_else(|| PyValueError::new_err("无效的时间戳"))
}

#[pyfunction]
#[pyo3(signature = (timestamp, hours=8))]
fn get_local_datetime(py: Python, timestamp: Bound<'_, PyAny>, hours: i64) -> PyResult<Py<PyAny>> {
    let naive_dt = if let Ok(s) = timestamp.extract::<String>() {
        if s.chars().all(|c| c.is_ascii_digit()) {
            let ts: i64 = s.parse().map_err(|_| PyValueError::new_err("无效的时间戳字符串"))?;
            parse_numeric_timestamp(ts)?
        } else {
            parse_str_timestamp(&s)?
        }
    } else if let Ok(ts) = timestamp.extract::<i64>() {
        parse_numeric_timestamp(ts)?
    } else if let Ok(ts) = timestamp.extract::<f64>() {
        parse_numeric_timestamp((ts * 1000.0) as i64)?
    } else {
        return Err(PyValueError::new_err("不支持的时间戳类型"));
    };

    let dt = naive_dt + Duration::hours(hours);
    
    let datetime_mod = py.import("datetime")?;
    let py_dt = datetime_mod.getattr("datetime")?.call1((
        dt.year(),
        dt.month(),
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second(),
        dt.nanosecond() / 1000,
    ))?;
    
    Ok(py_dt.unbind())
}

// ================================================================================================
// BarGeneratorInner - 内部可变状态
// ================================================================================================
struct BarGeneratorInner {
    bar: Option<RustBarData>,
    interval_count: usize,
    reset_count: usize,
    window_bar: Option<RustBarData>,
    last_tick: Option<RustTickData>,
    last_bar: Option<RustBarData>,
    finished: bool,
    bar_push_status: HashMap<i64, bool>,
}

// ================================================================================================
// BarGenerator - K线生成器核心类（使用 RefCell 实现内部可变性）
// ================================================================================================
#[pyclass(module = "rust_bar_generator")]
pub struct BarGenerator {
    // 使用 RefCell 包装可变状态
    inner: RwLock<BarGeneratorInner>,
    // 不可变配置
    on_bar: Option<Py<PyAny>>,
    on_window_bar: Option<Py<PyAny>>,
    interval: RustInterval,
    window: usize,
    interval_slice: bool,
    target_minutes: HashSet<u32>,
    target_hours: HashSet<u32>,
    target_days: HashSet<u32>,
    target_weeks: HashSet<u32>,
    target_months: HashSet<u32>,
}

/// 修剪时间到分钟精度
fn trim_bar_time(py: Python, mut bar: RustBarData) -> PyResult<RustBarData> {
    if let Some(ref dt_obj) = bar.datetime {
        let dt_bound = dt_obj.bind(py);
        let ts_method = dt_bound.call_method0("timestamp")?;
        let ts_seconds = ts_method.extract::<f64>()?;
        let ts_millis = (ts_seconds * 1000.0) as i64;
        
        if let Some(dt) = DateTime::from_timestamp_millis(ts_millis)
            .map(|dt| dt.with_timezone(&*TZ_INFO)) 
        {
            let trimmed_py_dt = PyDateTime::new(
                py,
                dt.year(),
                dt.month() as u8,
                dt.day() as u8,
                dt.hour() as u8,
                dt.minute() as u8,
                0,
                0,
                None
            )?;
            
            bar.datetime = Some(trimmed_py_dt.into());
        }
    }
    Ok(bar)
}

#[pymethods]
impl BarGenerator {
    #[new]
    #[pyo3(signature = (on_bar=None, window=1, on_window_bar=None, interval=None, interval_slice=true))]
    fn new(
        _py: Python,
        on_bar: Option<Py<PyAny>>,
        window: usize,
        on_window_bar: Option<Py<PyAny>>,
        interval: Option<&Bound<'_, PyAny>>,
        interval_slice: bool,
    ) -> PyResult<Self> {
        let rust_interval = if let Some(iv) = interval {
            RustInterval::from_py_any(iv)?
        } else {
            RustInterval::MINUTE
        };
        
        let target_minutes: HashSet<u32> = (0..60).step_by(window).collect();
        let target_hours: HashSet<u32> = (0..24).step_by(window).collect();
        let target_days: HashSet<u32> = (1..32).step_by(window).collect();
        let target_weeks: HashSet<u32> = (1..54).step_by(window).collect();
        let target_months: HashSet<u32> = (1..13).step_by(window).collect();

        Ok(BarGenerator {
            inner: RwLock::new(BarGeneratorInner {
                bar: None,
                interval_count: 0,
                reset_count: 0,
                window_bar: None,
                last_tick: None,
                last_bar: None,
                finished: false,
                bar_push_status: HashMap::new(),
            }),
            on_bar,
            on_window_bar,
            interval: rust_interval,
            window,
            interval_slice,
            target_minutes,
            target_hours,
            target_days,
            target_weeks,
            target_months,
        })
    }

    fn __reduce__<'py>(&self, py: Python<'py>) -> PyResult<(Py<PyAny>, Py<PyAny>)> {
        let cls = PyModule::import(py, "rust_bar_generator")?.getattr("BarGenerator")?;
        
        let interval_str = match self.interval {
            RustInterval::TICK => "TICK",
            RustInterval::MINUTE => "MINUTE",
            RustInterval::HOUR => "HOUR",
            RustInterval::DAILY => "DAILY",
            RustInterval::WEEKLY => "WEEKLY",
            RustInterval::MONTHLY => "MONTHLY",
        };
        
        let args = (
            self.on_bar.as_ref().map(|f| f.clone_ref(py)),
            self.window,
            self.on_window_bar.as_ref().map(|f| f.clone_ref(py)),
            interval_str,
            self.interval_slice,
        );
        
        Ok((cls.into(), args.into_pyobject(py)?.into()))
    }

    /// update_tick 使用 &self 而不是 &mut self，避免借用冲突
    fn update_tick(&self, py: Python, tick: Bound<'_, PyAny>) -> PyResult<()> {
        let rust_tick = RustTickData::from_py_tick(py, &tick)?;
        self.update_tick_internal(py, rust_tick)
    }

    /// update_bar 使用 &self 而不是 &mut self，避免借用冲突
    fn update_bar(&self, py: Python, bar: Bound<'_, PyAny>) -> PyResult<()> {
        let rust_bar = RustBarData::from_py_bar(py, &bar)?;
        self.update_bar_internal(py, rust_bar)
    }

    fn generate(&self, py: Python) -> PyResult<()> {
        // 先从 inner 中取出 bar，释放 RefCell 借用
        let bar_to_callback = {
            let mut inner = self.inner.write().unwrap();
            inner.bar.take()
        };

        if let Some(bar) = bar_to_callback {
            let callback_opt = self.on_bar.as_ref().map(|c| c.clone_ref(py));
            
            if let Some(callback) = callback_opt {
                let mut new_bar = bar;
                
                let now = chrono::Utc::now().with_timezone(&*TZ_INFO) - Duration::minutes(1);
                let py_dt = PyDateTime::new(
                    py,
                    now.year(),
                    now.month() as u8,
                    now.day() as u8,
                    now.hour() as u8,
                    now.minute() as u8,
                    now.second() as u8,
                    now.nanosecond() / 1000,
                    None
                )?;
                new_bar.datetime = Some(py_dt.into());
                
                let trimmed_bar = trim_bar_time(py, new_bar)?;
                // 回调在 RefCell 借用释放后执行，安全！
                callback.call1(py, (trimmed_bar,))?;
            }
        }
        Ok(())
    }

    fn generate_bar_event(&self, py: Python, _event: Bound<'_, PyAny>) -> PyResult<()> {
        // 先检查并获取必要的数据，然后释放借用
        // 修改：将 bar_dt 加入返回元组，使其能在作用域外使用
        let (should_generate, bar_timestamp, vt_symbol, bar_dt) = {
            let inner = self.inner.read().unwrap();
            
            if inner.bar.is_none() {
                return Ok(());
            }
            let bar = inner.bar.as_ref().unwrap();
            let bar_dt = bar.get_datetime_chrono(py)?
                .ok_or_else(|| PyValueError::new_err("Bar缺少datetime"))?;
            let bar_timestamp = bar_dt.timestamp_millis();
            if let Some(&status) = inner.bar_push_status.get(&bar_timestamp) {
                if status {
                    return Ok(());
                }
            }
            let now_datetime = chrono::Utc::now().with_timezone(&*TZ_INFO);
            let time_delta = now_datetime.signed_duration_since(bar_dt);
            
            let should_generate = time_delta > Duration::minutes(2);
            let vt_symbol = bar.vt_symbol.clone();
            
            // 返回 bar_dt (DateTime<Tz> 实现了 Copy)
            (should_generate, bar_timestamp, vt_symbol, bar_dt)
        };
        
        if should_generate {
            println!(
                "合约：{}，最新bar时间：{}，分钟bar缺失即将强制合成分钟bar",
                vt_symbol, bar_dt
            );
            
            // 更新状态
            {
                let mut inner = self.inner.write().unwrap();
                inner.bar_push_status.insert(bar_timestamp, true);
            }
            
            // 调用 generate（RefCell 借用已释放）
            self.generate(py)?;
        }
        
        Ok(())
    }
    fn __repr__(&self) -> String {
        format!("BarGenerator(interval={:?}, window={})", self.interval, self.window)
    }
}

impl BarGenerator {
    fn update_tick_internal(&self, py: Python, tick: RustTickData) -> PyResult<()> {
        if tick.last_price == 0.0 {
            return Ok(());
        }

        let tick_dt = tick.get_datetime_chrono(py)?
            .ok_or_else(|| PyValueError::new_err("Tick缺少datetime"))?;

        // 计算成交量变化和检查新分钟，使用临时借用
        let (volume_change, new_minute, old_bar) = {
            let mut inner = self.inner.write().unwrap();
            
            let volume_change = if let Some(ref last_tick) = inner.last_tick {
                (tick.volume - last_tick.volume).max(0.0)
            } else {
                0.0
            };

            let new_minute = if let Some(ref bar) = inner.bar {
                let bar_dt = bar.get_datetime_chrono(py)?
                    .ok_or_else(|| PyValueError::new_err("Bar缺少datetime"))?;
                bar_dt.minute() != tick_dt.minute()
            } else {
                true
            };

            let old_bar = if new_minute {
                inner.bar.take()
            } else {
                None
            };

            (volume_change, new_minute, old_bar)
        };  // inner 借用在这里释放

        // 处理旧 bar 的回调（在 RefCell 借用释放后）
        if let Some(bar_data) = old_bar {
            if let Some(ref callback) = self.on_bar {
                let trimmed_bar = trim_bar_time(py, bar_data)?;
                if let Err(e) = callback.call1(py, (trimmed_bar,)) {
                    eprintln!("Error in on_bar callback: {:?}", e);
                }
            }
        }

        // 重新获取借用，创建或更新 bar
        {
            let mut inner = self.inner.write().unwrap();
            
            if new_minute {
                let new_bar = RustBarData {
                    symbol: tick.symbol.clone(),
                    exchange: tick.exchange,
                    datetime: tick.datetime.as_ref().map(|dt| dt.clone_ref(py)),
                    interval: Some(RustInterval::MINUTE),
                    volume: 0.0,
                    open_interest: 0.0,
                    open_price: tick.last_price,
                    high_price: tick.last_price,
                    low_price: tick.last_price,
                    close_price: tick.last_price,
                    gateway_name: tick.gateway_name.clone(),
                    vt_symbol: tick.vt_symbol.clone(),
                };
                inner.bar = Some(new_bar);
            } else {
                if let Some(ref mut bar) = inner.bar {
                    bar.high_price = bar.high_price.max(tick.last_price);
                    bar.low_price = bar.low_price.min(tick.last_price);
                    bar.close_price = tick.last_price;
                    bar.datetime = tick.datetime.as_ref().map(|dt| dt.clone_ref(py));
                }
            }

            if let Some(ref mut bar) = inner.bar {
                bar.open_interest = tick.open_interest;
            }

            if inner.last_tick.is_some() {
                if let Some(ref mut bar) = inner.bar {
                    bar.volume += volume_change;
                }
            }

            inner.last_tick = Some(tick);
        }
        
        Ok(())
    }

    fn update_bar_internal(&self, py: Python, bar: RustBarData) -> PyResult<()> {
        let bar_dt = bar.get_datetime_chrono(py)?
            .ok_or_else(|| PyValueError::new_err("Bar缺少datetime"))?;

        // 第一阶段：获取 last_bar 时间并处理 window_bar 初始化和更新
        let (last_dt_opt, window_bar_to_callback) = {
            let mut inner = self.inner.write().unwrap();
            
            let last_dt_opt = if let Some(ref last_bar) = inner.last_bar {
                last_bar.get_datetime_chrono(py)?
            } else {
                None
            };

            // 初始化或更新 window_bar
            if inner.window_bar.is_none() {
                let dt = match self.interval {
                    RustInterval::MINUTE => bar_dt.with_second(0).unwrap().with_nanosecond(0).unwrap(),
                    RustInterval::HOUR => bar_dt.with_minute(0).unwrap().with_second(0).unwrap().with_nanosecond(0).unwrap(),
                    RustInterval::DAILY => (bar_dt + Duration::days(1)).date_naive().and_hms_opt(0, 0, 0).unwrap().and_local_timezone(*TZ_INFO).unwrap(),
                    RustInterval::WEEKLY => (bar_dt + Duration::weeks(1)).date_naive().and_hms_opt(0, 0, 0).unwrap().and_local_timezone(*TZ_INFO).unwrap(),
                    RustInterval::MONTHLY => {
                        let (y, m) = if bar_dt.month() == 12 {
                            (bar_dt.year() + 1, 1)
                        } else {
                            (bar_dt.year(), bar_dt.month() + 1)
                        };
                        match bar_dt.timezone().from_local_datetime(
                            &NaiveDate::from_ymd_opt(y, m, 1).unwrap().and_hms_opt(0, 0, 0).unwrap()
                        ) {
                            chrono::LocalResult::Single(t) => t,
                            _ => bar_dt,
                        }
                    }
                    _ => bar_dt,
                };

                let py_dt = PyDateTime::new(
                    py,
                    dt.year(),
                    dt.month() as u8,
                    dt.day() as u8,
                    dt.hour() as u8,
                    dt.minute() as u8,
                    dt.second() as u8,
                    dt.nanosecond() / 1000,
                    None
                )?;

                let new_window_bar = RustBarData {
                    symbol: bar.symbol.clone(),
                    exchange: bar.exchange,
                    datetime: Some(py_dt.into()),
                    interval: Some(self.interval),
                    volume: 0.0,
                    open_interest: bar.open_interest,
                    open_price: bar.open_price,
                    high_price: bar.high_price,
                    low_price: bar.low_price,
                    close_price: bar.close_price,
                    gateway_name: bar.gateway_name.clone(),
                    vt_symbol: bar.vt_symbol.clone(),
                };
                inner.window_bar = Some(new_window_bar);
            } else {
                if let Some(ref mut window_bar) = inner.window_bar {
                    window_bar.high_price = window_bar.high_price.max(bar.high_price);
                    window_bar.low_price = window_bar.low_price.min(bar.low_price);
                }
            }

            // 更新 close_price, volume, open_interest
            if let Some(ref mut window_bar) = inner.window_bar {
                window_bar.close_price = bar.close_price;
                window_bar.volume += bar.volume;
                window_bar.open_interest = bar.open_interest;
            }

            // 计算是否需要触发回调
            let now_value = self.get_interval_value_from_dt(&bar_dt);
            let mut finished = false;

            if let Some(ref last_dt) = last_dt_opt {
                let last_value = self.get_interval_value_from_dt(last_dt);

                if now_value != last_value {
                    // 判断是否使用目标时间点检查模式
                    let use_target_check = match self.interval {
                        RustInterval::MINUTE => {
                            if self.interval_slice {
                                if self.window < 60 {
                                    60 % self.window == 0
                                } else {
                                    1440 % self.window == 0
                                }
                            } else {
                                false
                            }
                        }
                        RustInterval::HOUR => self.interval_slice && 24 % self.window == 0,
                        RustInterval::DAILY => self.interval_slice && 7 % self.window == 0,
                        RustInterval::WEEKLY => self.interval_slice && 52 % self.window == 0,
                        _ => self.interval_slice,
                    };

                    if use_target_check && self.check_target_value(now_value) {
                        finished = true;
                    } else if !use_target_check {
                        // 对于 DAILY/WEEKLY/MONTHLY 或不能整除的情况，使用计数器方式
                        // 每次日期值变化时递增计数器
                        inner.interval_count += 1;
                        
                        // 当计数达到 window 时触发
                        if inner.interval_count % self.window == 0 {
                            finished = true;
                        }
                    }
                }
            }

            // 如果需要触发回调，取出 window_bar
            let window_bar_to_callback = if finished {
                let wb = inner.window_bar.take();
                inner.reset_count = 0;
                inner.interval_count = 0;
                inner.bar_push_status.clear();
                wb
            } else {
                None
            };

            (last_dt_opt, window_bar_to_callback)
        };  // inner 借用在这里释放

        // 第二阶段：在 RefCell 借用释放后执行回调
        if let Some(window_bar_data) = window_bar_to_callback {
            if let Some(ref callback) = self.on_window_bar {
                if let Err(e) = callback.call1(py, (window_bar_data,)) {
                        eprintln!("Error in on_window_bar callback: {:?}", e);
                    }
            }
        }

        // 第三阶段：更新 last_bar
        {
            let mut inner = self.inner.write().unwrap();
            inner.last_bar = Some(bar);
        }
        
        Ok(())
    }

    #[inline(always)]
    fn get_interval_value_from_dt(&self, dt: &DateTime<chrono_tz::Tz>) -> u32 {
        match self.interval {
            RustInterval::MINUTE => {
                if self.interval_slice && self.window >= 60 {
                    // 对于大于等于60分钟的窗口，返回从0点开始的总分钟数
                    dt.hour() * 60 + dt.minute()
                } else {
                    dt.minute()
                }
            }
            RustInterval::HOUR => dt.hour(),
            RustInterval::DAILY => dt.day(),
            RustInterval::WEEKLY => dt.iso_week().week(),
            RustInterval::MONTHLY => dt.month(),
            _ => 0,
        }
    }

    fn check_target_value(&self, value: u32) -> bool {
        match self.interval {
            RustInterval::MINUTE => {
                if self.interval_slice && self.window >= 60 {
                    // 对于大于等于60分钟的窗口，检查总分钟数是否是window的倍数
                    (value as usize) % self.window == 0
                } else {
                    self.target_minutes.contains(&value)
                }
            }
            RustInterval::HOUR => self.target_hours.contains(&value),
            RustInterval::DAILY => self.target_days.contains(&value),
            RustInterval::WEEKLY => self.target_weeks.contains(&value),
            RustInterval::MONTHLY => self.target_months.contains(&value),
            _ => false,
        }
    }


}

// ================================================================================================
// Python 模块定义
// ================================================================================================
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



