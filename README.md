# PyMarket V2

高性能市场模拟系统，使用 Rust 实现后端核心功能，Python 实现前端 UI 和可视化。

## 架构

- **Rust 后端**: 高性能订单撮合、资产管理、机器人策略计算
- **Python 前端**: UI 和可视化（使用 finplot）
- **通信方式**: TCP Socket，使用 JSON 协议

## 项目结构

```
pyMarketV2/
├── rust_backend/          # Rust 后端
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs        # 入口
│       ├── finance.rs     # Token, Order, TradingPair
│       ├── trader.rs      # Trader
│       ├── bot.rs         # RandomBot, BotManager
│       ├── utils.rs       # 工具函数
│       ├── protocol.rs    # 通信协议
│       └── server.rs      # TCP 服务器
├── python_frontend/       # Python 前端
│   ├── main.py           # 主程序
│   ├── client.py         # 后端通信客户端
│   └── gui.py            # finplot GUI
├── main.py               # 启动器
└── README.md
```

## 安装

### 1. 安装 Rust

```bash
# 访问 https://rustup.rs/ 安装 Rust
```

### 2. 安装 Python 依赖

```bash
cd python_frontend
pip install -r requirements.txt
```

## 运行

### 方式 1: 使用启动器

```bash
python main.py
```

### 方式 2: 分别启动

1. 启动后端:
```bash
cd rust_backend
cargo run --release
```

2. 启动前端:
```bash
cd python_frontend
python main.py
```

## 使用

启动后，系统会自动：
1. 启动 Rust 后端服务器
2. 创建代币（USDT, BTC, ETH）
3. 创建交易对（BTC/USDT, ETH/USDT）
4. 创建 100 个交易机器人
5. 启动市场模拟
6. 打开 GUI 显示 K 线图

### 游戏命令

在控制台输入：
- `b1 <数量>` - 在交易对 1 买入
- `s1 <数量>` - 在交易对 1 卖出
- `b2 <数量>` - 在交易对 2 买入
- `s2 <数量>` - 在交易对 2 卖出
- `info` - 显示玩家信息
- `quit` - 退出

## 性能优化

相比 V1 版本，V2 有以下性能改进：

1. **Rust 核心**: 使用 Rust 的高性能数据结构和并发原语
2. **无锁数据结构**: 使用 DashMap 和 parking_lot 实现高效并发
3. **二进制堆订单簿**: O(log n) 的订单插入和匹配
4. **进程分离**: Python 只负责 UI，不阻塞计算
5. **Decimal 精度**: 使用 rust_decimal 保持金融计算精度

## 协议

前后端通过 TCP 通信，使用 JSON 格式：

### 请求示例
```json
{
  "type": "submit_market_order",
  "trader_id": 1,
  "trading_pair_id": "BTC/USDT",
  "direction": "buy",
  "volume": "10.5"
}
```

### 响应示例
```json
{
  "type": "market_order_executed",
  "trades": [
    {"timestamp": 1234567890, "price": "40000.00", "volume": "10.5"}
  ]
}
```
