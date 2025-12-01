
## 前置要求

### 安装 Rust

```bash
# Linux/macOS
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Windows
# 下载并运行 rustup-init.exe
# https://rustup.rs/
```

安装完成后，重启终端并验证安装：

```bash
rustc --version
cargo --version
```
## 安装步骤

```bash
pip install maturin
```
**生产构建：**
```bash
cd rust_rest_client_project
maturin build --release
pip install target/wheels/*.whl
```
## 使用示例
```
# RsutTickData,RustBatData,get_local_datetime的datetime无时区数据
from rust_bar_generator import BarGenerator


    def __init__(self, strategy_engine: StrategyEngine, strategy_name: str, vt_symbols: List[str], setting: dict):
    self.bar_generator: Dict[str, BarGenerator] = {}  # K线生成器
        for vt_symbol in self.vt_symbols:
            self.bar_generator[vt_symbol] = BarGenerator(self.on_bar, 30, self.on_x_minute_bar, Interval.MINUTE, interval_slice=True,)
```
