import datetime

import finplot as fplt
import pandas as pd
import numpy as np
import time
from typing import List, Tuple, Callable
from decimal import Decimal
from finance import TradingPair


def calculate_candles(trade_log: List[Tuple[float, Decimal, Decimal]], 
                     period: float, max_candles: int) -> pd.DataFrame:
    """
    根据交易日志计算K线数据（优化版本）
    从旧往新遍历log，将第一个数据作为第一根k线的起始时间，
    一直遍历直到时间差达到或超过设定的k线周期（可累计一个误差值，将误差计入下一个周期的计算中）
    """
    if not trade_log:
        # 返回空的K线数据框
        return pd.DataFrame(columns=['time', 'open', 'close', 'high', 'low', 'volume'])
    
    # 缓存上次计算的结果，避免重复计算
    if not hasattr(calculate_candles, '_cache'):
        calculate_candles._cache = {}
    
    cache_key = (id(trade_log), period, max_candles)
    if cache_key in calculate_candles._cache:
        cached_result, cached_length = calculate_candles._cache[cache_key]
        if len(trade_log) == cached_length:
            return cached_result.copy()
    
    # 按时间排序交易日志（从旧到新）
    sorted_log = sorted(trade_log, key=lambda x: x[0])
    
    candles = []
    current_candle_start = None
    current_candle_data = []
    accumulated_error = 0.0  # 累计误差时间
    
    # 预分配变量避免重复计算
    open_price = close_price = high_price = low_price = total_volume = 0.0
    
    for trade in sorted_log:
        trade_time, price, volume = trade
        
        # 将Decimal转换为float用于绘图
        price_float = float(price) if isinstance(price, Decimal) else price
        volume_float = float(volume) if isinstance(volume, Decimal) else volume
        
        if current_candle_start is None:
            # 第一根K线的起始时间
            current_candle_start = trade_time
            current_candle_data = [(trade_time, price_float, volume_float)]
            continue
        
        # 计算当前时间与K线起始时间的时间差（考虑累计误差）
        time_diff = trade_time - current_candle_start + accumulated_error
        
        if time_diff < period:
            # 时间差未达到周期，继续累积到当前K线
            current_candle_data.append((trade_time, price_float, volume_float))
        else:
            # 时间差达到或超过周期，完成当前K线
            if current_candle_data:
                # 优化OHLCV计算：避免使用zip，直接遍历
                open_price = current_candle_data[0][1]
                close_price = current_candle_data[-1][1]
                high_price = low_price = open_price
                total_volume = 0
                
                for _, p, v in current_candle_data:
                    if p > high_price:
                        high_price = p
                    if p < low_price:
                        low_price = p
                    total_volume += v
                
                candle = {
                    'time': datetime.datetime.fromtimestamp(current_candle_start),
                    'open': open_price,
                    'close': close_price,
                    'high': high_price,
                    'low': low_price,
                    'volume': total_volume
                }
                candles.append(candle)
            
            # 计算误差时间（当前交易时间与理想结束时间的差值）
            ideal_end_time = current_candle_start + period
            error_time = trade_time - ideal_end_time
            accumulated_error = max(0, error_time)  # 只保留正误差
            
            # 开始新的K线，以当前交易作为新K线的开始
            current_candle_start = trade_time
            current_candle_data = [(trade_time, price_float, volume_float)]
    
    # 处理最后一根未完成的K线
    if current_candle_data:
        # 优化OHLCV计算
        open_price = current_candle_data[0][1]
        close_price = current_candle_data[-1][1]
        high_price = low_price = open_price
        total_volume = 0
        
        for _, p, v in current_candle_data:
            if p > high_price:
                high_price = p
            if p < low_price:
                low_price = p
            total_volume += v
        
        candle = {
            'time': datetime.datetime.fromtimestamp(current_candle_start),
            'open': open_price,
            'close': close_price,
            'high': high_price,
            'low': low_price,
            'volume': total_volume
        }
        candles.append(candle)
    
    # 转换为DataFrame
    candles_df = pd.DataFrame(candles)
    
    # 限制最大K线数量
    if len(candles_df) > max_candles:
        candles_df = candles_df.iloc[-max_candles:].reset_index(drop=True)
    
    # 缓存结果
    calculate_candles._cache[cache_key] = (candles_df.copy(), len(trade_log))
    
    # 清理过期的缓存（避免内存泄漏）
    if len(calculate_candles._cache) > 10:
        oldest_key = next(iter(calculate_candles._cache.keys()))
        del calculate_candles._cache[oldest_key]

    return candles_df


def start_gui(trading_pairs: List[TradingPair], max_candles: int = 100,
              candle_period: float = 1) -> Callable:
    """启动GUI并返回更新函数"""
    # 创建图表
    plots = {}
    volumes = {}
    axs = {}
    
    # 为每个交易对创建图表
    for i, pair in enumerate(trading_pairs):
        # 创建子图
        if i == 0:
            ax = fplt.create_plot(f"{pair.base_token}/{pair.quote_token} - K线图", rows=1, init_zoom_periods=max_candles)
            axv = ax.overlay()
        else:
            ax = fplt.create_plot(f"{pair.base_token}/{pair.quote_token} - K线图", rows=1, init_zoom_periods=max_candles, maximize=False)
            axv = ax.overlay()
        
        axs[pair] = (ax, axv)
        
        # 初始K线数据
        candles_df = calculate_candles(pair.log, candle_period, max_candles)
        
        if not candles_df.empty:
            # 确保数据格式正确 - 直接使用DataFrame而不是.values
            # 绘制K线图
            plots[pair] = fplt.candlestick_ochl(candles_df[['time', 'open', 'close', 'high', 'low']], ax=ax)
            # 绘制成交量
            volumes[pair] = fplt.volume_ocv(candles_df[['time', 'open', 'close', 'volume']], ax=axv)
        else:
            # 创建空的图表对象
            plots[pair] = None
            volumes[pair] = None
    
    # 定义更新函数
    def update_gui():
        """更新GUI图表（优化版本）"""
        # 检查是否有新的交易数据需要更新
        has_new_data = False
        
        for pair in trading_pairs:
            # 检查交易日志是否有变化
            if not hasattr(pair, '_last_log_length'):
                pair._last_log_length = 0
            
            if len(pair.log) == pair._last_log_length:
                # 没有新交易数据，跳过更新
                continue
            
            has_new_data = True
            pair._last_log_length = len(pair.log)
            
            # 计算新的K线数据
            candles_df = calculate_candles(pair.log, candle_period, max_candles)
            
            if candles_df.empty:
                continue
            
            ax, axv = axs[pair]
            
            if plots.get(pair) is None:
                # 首次绘制
                plots[pair] = fplt.candlestick_ochl(candles_df[['time', 'open', 'close', 'high', 'low']], ax=ax)
                volumes[pair] = fplt.volume_ocv(candles_df[['time', 'open', 'close', 'volume']], ax=axv)
            else:
                # 更新现有图表
                plots[pair].update_data(candles_df[['time', 'open', 'close', 'high', 'low']])
                volumes[pair].update_data(candles_df[['time', 'open', 'close', 'volume']])
        
        # 只有有新的交易数据时才刷新图表
        if has_new_data:
            fplt.refresh()
    
    # 启动定时器回调（降低更新频率以减少卡顿）
    fplt.timer_callback(update_gui, 1/30)
    
    # 显示GUI（这会阻塞主线程，但定时器回调会继续工作）
    fplt.show()
    
    # 返回更新函数（虽然GUI是阻塞的，但定时器回调会继续运行）
    return update_gui