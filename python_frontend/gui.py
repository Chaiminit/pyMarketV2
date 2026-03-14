"""
GUI module using finplot for visualization
"""

import datetime
from typing import List, Dict, Any, Callable, Optional
from decimal import Decimal

import finplot as fplt
import pandas as pd
import numpy as np

from client import BackendClient


def calculate_candles(trade_log: List[Dict[str, Any]],
                     period: float, max_candles: int) -> pd.DataFrame:
    """
    Calculate candlestick data from trade log
    """
    if not trade_log:
        return pd.DataFrame(columns=['time', 'open', 'close', 'high', 'low', 'volume'])

    # Sort by timestamp
    sorted_log = sorted(trade_log, key=lambda x: x['timestamp'])

    candles = []
    current_candle_start = None
    current_candle_data = []
    accumulated_error = 0.0

    for trade in sorted_log:
        trade_time = trade['timestamp'] / 1000.0  # Convert ms to seconds
        price = float(trade['price'])
        volume = float(trade['volume'])

        if current_candle_start is None:
            current_candle_start = trade_time
            current_candle_data = [(trade_time, price, volume)]
            continue

        time_diff = trade_time - current_candle_start + accumulated_error

        if time_diff < period:
            current_candle_data.append((trade_time, price, volume))
        else:
            if current_candle_data:
                open_price = current_candle_data[0][1]
                close_price = current_candle_data[-1][1]
                high_price = max(p for _, p, _ in current_candle_data)
                low_price = min(p for _, p, _ in current_candle_data)
                total_volume = sum(v for _, _, v in current_candle_data)

                candle = {
                    'time': datetime.datetime.fromtimestamp(current_candle_start),
                    'open': open_price,
                    'close': close_price,
                    'high': high_price,
                    'low': low_price,
                    'volume': total_volume
                }
                candles.append(candle)

            ideal_end_time = current_candle_start + period
            error_time = trade_time - ideal_end_time
            accumulated_error = max(0, error_time)

            current_candle_start = trade_time
            current_candle_data = [(trade_time, price, volume)]

    # Process last candle
    if current_candle_data:
        open_price = current_candle_data[0][1]
        close_price = current_candle_data[-1][1]
        high_price = max(p for _, p, _ in current_candle_data)
        low_price = min(p for _, p, _ in current_candle_data)
        total_volume = sum(v for _, _, v in current_candle_data)

        candle = {
            'time': datetime.datetime.fromtimestamp(current_candle_start),
            'open': open_price,
            'close': close_price,
            'high': high_price,
            'low': low_price,
            'volume': total_volume
        }
        candles.append(candle)

    candles_df = pd.DataFrame(candles)

    if len(candles_df) > max_candles:
        candles_df = candles_df.iloc[-max_candles:].reset_index(drop=True)

    return candles_df


class MarketGUI:
    """Market visualization GUI"""

    def __init__(self, client: BackendClient, max_candles: int = 100,
                 candle_period: float = 1.0):
        self.client = client
        self.max_candles = max_candles
        self.candle_period = candle_period
        self.trading_pairs: List[str] = []
        self.plots: Dict[str, Any] = {}
        self.volumes: Dict[str, Any] = {}
        self.axs: Dict[str, Any] = {}
        self._last_log_lengths: Dict[str, int] = {}

    def initialize(self):
        """Initialize GUI with trading pairs"""
        response = self.client.get_all_trading_pairs()
        if response.get('type') != 'trading_pairs_list':
            print("Failed to get trading pairs")
            return False

        pairs = response.get('pairs', [])
        if not pairs:
            print("No trading pairs available")
            return False

        self.trading_pairs = [p['id'] for p in pairs]

        # Create plots for each trading pair
        for i, pair_info in enumerate(pairs):
            pair_id = pair_info['id']

            if i == 0:
                ax = fplt.create_plot(f"{pair_id} - K线图", rows=1,
                                     init_zoom_periods=self.max_candles)
                axv = ax.overlay()
            else:
                ax = fplt.create_plot(f"{pair_id} - K线图", rows=1,
                                     init_zoom_periods=self.max_candles,
                                     maximize=False)
                axv = ax.overlay()

            self.axs[pair_id] = (ax, axv)

            # Get initial data
            response = self.client.get_trade_log(pair_id, limit=1000)
            if response.get('type') == 'trade_log':
                trades = response.get('trades', [])
                candles_df = calculate_candles(trades, self.candle_period, self.max_candles)

                if not candles_df.empty:
                    self.plots[pair_id] = fplt.candlestick_ochl(
                        candles_df[['time', 'open', 'close', 'high', 'low']], ax=ax)
                    self.volumes[pair_id] = fplt.volume_ocv(
                        candles_df[['time', 'open', 'close', 'volume']], ax=axv)
                    self._last_log_lengths[pair_id] = len(trades)
                else:
                    self.plots[pair_id] = None
                    self.volumes[pair_id] = None
                    self._last_log_lengths[pair_id] = 0

        return True

    def update(self):
        """Update GUI with new data"""
        has_new_data = False

        for pair_id in self.trading_pairs:
            response = self.client.get_trade_log(pair_id, limit=1000)
            if response.get('type') != 'trade_log':
                continue

            trades = response.get('trades', [])
            last_length = self._last_log_lengths.get(pair_id, 0)

            if len(trades) == last_length:
                continue

            has_new_data = True
            self._last_log_lengths[pair_id] = len(trades)

            candles_df = calculate_candles(trades, self.candle_period, self.max_candles)

            if candles_df.empty:
                continue

            ax, axv = self.axs[pair_id]

            if self.plots.get(pair_id) is None:
                self.plots[pair_id] = fplt.candlestick_ochl(
                    candles_df[['time', 'open', 'close', 'high', 'low']], ax=ax)
                self.volumes[pair_id] = fplt.volume_ocv(
                    candles_df[['time', 'open', 'close', 'volume']], ax=axv)
            else:
                self.plots[pair_id].update_data(
                    candles_df[['time', 'open', 'close', 'high', 'low']])
                self.volumes[pair_id].update_data(
                    candles_df[['time', 'open', 'close', 'volume']])

        if has_new_data:
            fplt.refresh()

    def run(self):
        """Run the GUI"""
        if not self.initialize():
            return

        # Set up update timer
        fplt.timer_callback(self.update, 1/30)

        # Show GUI
        fplt.show()


def start_gui(client: BackendClient, max_candles: int = 100,
              candle_period: float = 1.0):
    """Start the GUI"""
    gui = MarketGUI(client, max_candles, candle_period)
    gui.run()
