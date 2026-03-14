from typing import Dict, Optional
from decimal import Decimal, getcontext
from finance import Token, TradingPair, Order

# 设置decimal精度
getcontext().prec = 28


class Trader:
    """交易者类，表示市场中的一个参与者"""
    def __init__(self, name: str):
        self.name = name
        self.assets: Dict[Token, Decimal] = {}  # 资产字典（使用Decimal）
        self.orders: list[Order] = []  # 活跃订单列表
        self.cliented_trading_pairs = set()
    
    def __str__(self) -> str:
        return f"Trader({self.name})"
    
    def __repr__(self) -> str:
        return self.__str__()
    
    def add_asset(self, token: Token, amount: float) -> None:
        """添加资产"""
        decimal_amount = Decimal(str(amount))
        if token in self.assets:
            self.assets[token] += decimal_amount
        else:
            self.assets[token] = decimal_amount
    
    def submit(self, trading_pair: TradingPair, direction: str, price: float, volume: float) -> Optional[Order]:
        """提交限价订单"""
        self.cliented_trading_pairs.add(trading_pair)
        # 验证交易方向
        if direction not in ['buy', 'sell']:
            raise ValueError("交易方向必须是 'buy' 或 'sell'")
        
        # 验证价格和数量
        if price <= 0 or volume <= 0:
            raise ValueError("价格和数量必须大于0")

        if trading_pair.quote_token not in self.assets:
            self.add_asset(trading_pair.quote_token, 0)
        if trading_pair.base_token not in self.assets:
            self.add_asset(trading_pair.base_token, 0)
        
        # 检查资产是否充足
        if direction == 'buy':
            # 买入需要计价代币
            required_amount = Decimal(str(price * volume))
            if self.assets.get(trading_pair.quote_token, Decimal('0')) < required_amount:
                return None
        else:  # 'sell'
            # 卖出需要基准代币
            required_volume = Decimal(str(volume))
            if self.assets.get(trading_pair.base_token, Decimal('0')) < required_volume:
                return None
        
        # 创建并提交订单
        try:
            order = Order(self, trading_pair, direction, price, volume)
            trading_pair.recv(order)
            return order
        except Exception as e:
            print(f"提交订单失败: {e}")
            return None
    
    def submit_market(self, trading_pair: TradingPair, direction: str, volume: float) -> bool:
        """提交市价订单"""
        self.cliented_trading_pairs.add(trading_pair)

        # 验证交易方向
        if direction not in ['buy', 'sell']:
            raise ValueError("交易方向必须是 'buy' 或 'sell'")
        
        # 验证数量
        if volume <= 0:
            raise ValueError("数量必须大于0")

        if trading_pair.quote_token not in self.assets:
            self.add_asset(trading_pair.quote_token, 0)
        if trading_pair.base_token not in self.assets:
            self.add_asset(trading_pair.base_token, 0)
        
        # 调用交易对的市价单处理方法
        return trading_pair.recv_market(self, direction, volume)
    
    def cancel_order(self, order: Order) -> bool:
        """取消订单"""
        if order in self.orders and order.submitter == self:
            order.close()
            return True
        return False
    
    def get_asset_value(self, token: Token) -> Decimal:
        """获取特定代币的资产价值"""
        return self.assets.get(token, Decimal('0'))
    
    def get_total_value(self) -> Decimal:
        """计算总资产价值（以USDT为单位，包含订单中被冻结部分）"""
        total_value = Decimal('0')
        
        # 查找USDT交易对
        usdt_pairs = [pair for pair in self.cliented_trading_pairs if pair.quote_token.name == 'USDT']
        
        # 1. 计算当前持有的资产价值
        for token, amount in self.assets.items():
            if token.name == 'USDT':
                total_value += amount
            else:
                # 查找对应的USDT交易对
                usdt_pair = next((pair for pair in usdt_pairs if pair.base_token == token), None)
                if usdt_pair and usdt_pair.price:
                    total_value += amount * Decimal(str(usdt_pair.price))
        
        # 2. 计算订单中被冻结的资产价值
        for order in self.orders:
            if order.direction == 'buy':
                # 买单冻结的是计价代币（如USDT）
                frozen_amount = Decimal(str(order.frozen_amount))
                if order.trading_pair.quote_token.name == 'USDT':
                    total_value += frozen_amount
                else:
                    # 如果计价代币不是USDT，需要转换为USDT
                    quote_usdt_pair = next((pair for pair in usdt_pairs if pair.base_token == order.trading_pair.quote_token), None)
                    if quote_usdt_pair and quote_usdt_pair.price:
                        total_value += frozen_amount * Decimal(str(quote_usdt_pair.price))
            else:  # 'sell'
                # 卖单冻结的是基准代币
                frozen_amount = Decimal(str(order.frozen_amount))
                base_usdt_pair = next((pair for pair in usdt_pairs if pair.base_token == order.trading_pair.base_token), None)
                if base_usdt_pair and base_usdt_pair.price:
                    total_value += frozen_amount * Decimal(str(base_usdt_pair.price))
        
        return total_value
