"""
Python frontend client for PyMarket V2
Communicates with Rust backend via TCP socket
"""

import json
import socket
import threading
from typing import Optional, Dict, List, Any, Tuple
from decimal import Decimal


class BackendClient:
    """Client for communicating with Rust backend"""

    def __init__(self, host: str = "127.0.0.1", port: int = 8765):
        self.host = host
        self.port = port
        self.socket: Optional[socket.socket] = None
        self.lock = threading.Lock()
        self._connect()

    def _connect(self):
        """Establish connection to backend"""
        self.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.socket.connect((self.host, self.port))

    def _send_request(self, request: Dict[str, Any]) -> Dict[str, Any]:
        """Send request and receive response"""
        with self.lock:
            try:
                # Send request
                data = json.dumps(request) + "\n"
                self.socket.sendall(data.encode())

                # Receive response
                response_data = b""
                while not response_data.endswith(b"\n"):
                    chunk = self.socket.recv(4096)
                    if not chunk:
                        raise ConnectionError("Connection closed by server")
                    response_data += chunk

                return json.loads(response_data.decode().strip())
            except (ConnectionError, BrokenPipeError):
                # Reconnect and retry
                self._connect()
                data = json.dumps(request) + "\n"
                self.socket.sendall(data.encode())

                response_data = b""
                while not response_data.endswith(b"\n"):
                    chunk = self.socket.recv(4096)
                    if not chunk:
                        raise ConnectionError("Connection closed by server")
                    response_data += chunk

                return json.loads(response_data.decode().strip())

    def create_token(self, name: str) -> Dict[str, Any]:
        """Create a new token"""
        return self._send_request({"type": "create_token", "name": name})

    def create_trading_pair(self, base_token: str, quote_token: str, initial_price: Decimal) -> Dict[str, Any]:
        """Create a new trading pair"""
        return self._send_request({
            "type": "create_trading_pair",
            "base_token": base_token,
            "quote_token": quote_token,
            "initial_price": str(initial_price)
        })

    def create_bots(self, count: int, asset_configs: Dict[str, Tuple[Decimal, Decimal]],
                    name_prefix: str = "Bot", trend: float = 0.0, view: float = 10.0) -> Dict[str, Any]:
        """Create trading bots"""
        configs = {
            token: (str(min_amount), str(max_amount))
            for token, (min_amount, max_amount) in asset_configs.items()
        }
        return self._send_request({
            "type": "create_bots",
            "count": count,
            "asset_configs": configs,
            "name_prefix": name_prefix,
            "trend": trend,
            "view": view
        })

    def start_simulation(self) -> Dict[str, Any]:
        """Start market simulation"""
        return self._send_request({"type": "start_simulation"})

    def stop_simulation(self) -> Dict[str, Any]:
        """Stop market simulation"""
        return self._send_request({"type": "stop_simulation"})

    def submit_limit_order(self, trader_id: int, trading_pair_id: str,
                          direction: str, price: Decimal, volume: Decimal) -> Dict[str, Any]:
        """Submit a limit order"""
        return self._send_request({
            "type": "submit_limit_order",
            "trader_id": trader_id,
            "trading_pair_id": trading_pair_id,
            "direction": direction,
            "price": str(price),
            "volume": str(volume)
        })

    def submit_market_order(self, trader_id: int, trading_pair_id: str,
                           direction: str, volume: Decimal) -> Dict[str, Any]:
        """Submit a market order"""
        return self._send_request({
            "type": "submit_market_order",
            "trader_id": trader_id,
            "trading_pair_id": trading_pair_id,
            "direction": direction,
            "volume": str(volume)
        })

    def cancel_order(self, trader_id: int, order_id: int, trading_pair_id: str) -> Dict[str, Any]:
        """Cancel an order"""
        return self._send_request({
            "type": "cancel_order",
            "trader_id": trader_id,
            "order_id": order_id,
            "trading_pair_id": trading_pair_id
        })

    def get_trade_log(self, trading_pair_id: str, limit: Optional[int] = None) -> Dict[str, Any]:
        """Get trade log for a trading pair"""
        request = {
            "type": "get_trade_log",
            "trading_pair_id": trading_pair_id
        }
        if limit is not None:
            request["limit"] = limit
        return self._send_request(request)

    def get_order_book(self, trading_pair_id: str) -> Dict[str, Any]:
        """Get order book for a trading pair"""
        return self._send_request({
            "type": "get_order_book",
            "trading_pair_id": trading_pair_id
        })

    def get_trader_info(self, trader_id: int) -> Dict[str, Any]:
        """Get trader information"""
        return self._send_request({
            "type": "get_trader_info",
            "trader_id": trader_id
        })

    def get_all_trading_pairs(self) -> Dict[str, Any]:
        """Get all trading pairs"""
        return self._send_request({"type": "get_all_trading_pairs"})

    def get_market_data(self, trading_pair_id: str) -> Dict[str, Any]:
        """Get market data for a trading pair"""
        return self._send_request({
            "type": "get_market_data",
            "trading_pair_id": trading_pair_id
        })

    def create_player(self, name: str, assets: Dict[str, Decimal]) -> Dict[str, Any]:
        """Create a player trader"""
        return self._send_request({
            "type": "create_player",
            "name": name,
            "assets": {token: str(amount) for token, amount in assets.items()}
        })

    def close(self):
        """Close connection"""
        if self.socket:
            self.socket.close()
            self.socket = None

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.close()
