import time
from typing import List, Tuple, Dict, Optional
from decimal import Decimal, getcontext

# 设置decimal精度
getcontext().prec = 28

# 导入全局锁
from global_lock import global_lock


class Token:
    """代币类，用于表示交易中的各种代币"""
    def __init__(self, name: str):
        self.name = name
    
    def __str__(self) -> str:
        return self.name
    
    def __repr__(self) -> str:
        return f"Token({self.name})"
    
    def __hash__(self) -> int:
        return hash(self.name)
    
    def __eq__(self, other) -> bool:
        if isinstance(other, Token):
            return self.name == other.name
        return False



class Order:
    """订单类，表示一个交易订单"""
    def __init__(self, submitter, trading_pair, direction: str, price: float, volume: float):
        self.submitter = submitter  # 订单提交者
        self.trading_pair = trading_pair  # 目标交易对
        self.direction = direction  # 'buy' 或 'sell'
        self.price = Decimal(str(price))  # 订单价格（使用Decimal）
        self.total_volume = Decimal(str(volume))  # 总交易量（使用Decimal）
        self.executed_volume = Decimal('0')  # 已结算交易量（使用Decimal）
        self.frozen_amount = Decimal('0')  # 冻结量（使用Decimal）
        self.submit_time = time.time()  # 提交时间
        
        # 计算并冻结资产
        if direction == 'buy':
            # 买入需要冻结计价代币
            with global_lock:
                self.frozen_amount = self.price * self.total_volume
                submitter.assets[trading_pair.quote_token] -= self.frozen_amount
        else:  # 'sell'
            # 卖出需要冻结基准代币
            with global_lock:
                self.frozen_amount = self.total_volume
                submitter.assets[trading_pair.base_token] -= self.frozen_amount
    
    def __str__(self) -> str:
        status = "进行中" if self.executed_volume < self.total_volume else "已完成"
        return f"{self.direction.upper()} {self.trading_pair.base_token}/{self.trading_pair.quote_token} @ {float(self.price):.4f} x {float(self.executed_volume):.4f}/{float(self.total_volume):.4f} ({status})"
    
    def close(self) -> None:
        """关闭订单，返还剩余冻结资产"""
        # 从交易对的订单簿中移除
        if self.direction == 'buy':
            if self in self.trading_pair.buy_orders:
                self.trading_pair.buy_orders.remove(self)
        else:
            if self in self.trading_pair.sell_orders:
                self.trading_pair.sell_orders.remove(self)
        
        # 从提交者的订单簿中移除
        if self in self.submitter.orders:
            self.submitter.orders.remove(self)
        
        # 返还剩余冻结量
        with global_lock:
            if self.frozen_amount > Decimal('0'):
                if self.direction == 'buy':
                    self.submitter.assets[self.trading_pair.quote_token] += self.frozen_amount
                else:
                    self.submitter.assets[self.trading_pair.base_token] += self.frozen_amount
        
        # 删除自身引用
        del self


class TradingPair:
    """交易对类，管理交易撮合和订单簿"""
    def __init__(self, base_token: Token, quote_token: Token, price: float):
        self.quote_token = quote_token  # 计价代币（如USDT）
        self.base_token = base_token    # 基准代币（如BTC）
        self.buy_orders: List[Order] = []   # 买方订单队列
        self.sell_orders: List[Order] = []  # 卖方订单队列
        self.log: List[Tuple[float, Decimal, Decimal]] = []  # 交易日志 [(时间戳, 价格, 成交量)]（使用Decimal）
        self.price = Decimal(str(price))  # 价格（使用Decimal）
        self.clients = set() # 记录所有的交易者
    
    def __str__(self) -> str:
        return f"{self.base_token}/{self.quote_token}"
    
    def recv(self, order: Order) -> None:
        """接收限价订单"""
        # 根据订单方向添加到相应队列
        if order.direction == 'buy':
            # 买方队列按价格降序排列，价格相同则时间早的在前
            self._insert_order(self.buy_orders, order, reverse=True)
        else:  # 'sell'
            # 卖方队列按价格升序排列，价格相同则时间早的在前
            self._insert_order(self.sell_orders, order, reverse=False)

        # 添加到提交者的订单列表
        order.submitter.orders.append(order)

        # 记录交易者
        self.clients.add(order.submitter)

        # 尝试撮合订单
        self.update()
    
    def recv_market(self, submitter, direction: str, volume: float) -> bool:
        """接收市价订单"""
        # 记录交易者
        self.clients.add(submitter)

        decimal_volume = Decimal(str(volume))

        # 检查是否有足够的资产
        if direction == 'buy':
            # 计算最大所需资金
            max_needed = Decimal('0')
            temp_volume = decimal_volume
            for order in sorted(self.sell_orders, key=lambda x: (x.price, x.submit_time)):
                match_volume = min(temp_volume, order.total_volume - order.executed_volume)
                max_needed += match_volume * order.price
                temp_volume -= match_volume
                if temp_volume <= Decimal('0'):
                    break
            
            if submitter.assets.get(self.quote_token, Decimal('0')) < max_needed:
                return False
        else:  # 'sell'
            if submitter.assets.get(self.base_token, Decimal('0')) < decimal_volume:
                return False
        
        # 直接执行市价单
        temp_volume = decimal_volume
        # print(len(self.sell_orders), len(self.buy_orders), len(self.log))
        while temp_volume > Decimal('0') and (direction == 'buy' and self.sell_orders or direction == 'sell' and self.buy_orders):
            if direction == 'buy':
                # 从卖方队列取订单
                sell_order = self.sell_orders[0]
                match_volume = min(temp_volume, sell_order.total_volume - sell_order.executed_volume)
                match_price = sell_order.price

                # 执行匹配
                with global_lock:
                    submitter.assets[self.base_token] += match_volume
                    submitter.assets[self.quote_token] -= match_volume * match_price

                    sell_order.executed_volume += match_volume
                    sell_order.frozen_amount -= match_volume

                    # 更新卖家资产
                    sell_order.submitter.assets[self.quote_token] += match_volume * match_price
                
                # 记录交易
                self.log.append((time.time(), match_price, match_volume))
                self.price = match_price
                
                temp_volume -= match_volume
                
                # 检查卖单是否完成
                if sell_order.executed_volume >= sell_order.total_volume:
                    sell_order.close()
            else:  # 'sell'
                # 从买方队列取订单
                buy_order = self.buy_orders[0]
                match_volume = min(temp_volume, buy_order.total_volume - buy_order.executed_volume)
                match_price = buy_order.price
                
                # 执行匹配
                with global_lock:
                    submitter.assets[self.base_token] -= match_volume
                    submitter.assets[self.quote_token] += match_volume * match_price

                    buy_order.executed_volume += match_volume
                    buy_order.frozen_amount -= match_volume * match_price

                    # 更新买家资产
                    buy_order.submitter.assets[self.base_token] += match_volume
                
                # 记录交易
                self.log.append((time.time(), match_price, match_volume))
                self.price = match_price
                
                temp_volume -= match_volume
                
                # 检查买单是否完成
                if buy_order.executed_volume >= buy_order.total_volume:
                    buy_order.close()
        
        return True
    
    def update(self) -> None:
        """撮合订单"""
        while self.buy_orders and self.sell_orders:
            buy_order = self.buy_orders[0]
            sell_order = self.sell_orders[0]
            
            # 检查是否有价格交叉
            if buy_order.price < sell_order.price:
                break
            
            # 以先提交的订单价格为准
            if buy_order.submit_time <= sell_order.submit_time:
                match_price = buy_order.price
            else:
                match_price = sell_order.price
            
            # 以较小的量为准
            buy_remaining = buy_order.total_volume - buy_order.executed_volume
            sell_remaining = sell_order.total_volume - sell_order.executed_volume
            match_volume = min(buy_remaining, sell_remaining)
            
            # 更新已成交量
            buy_order.executed_volume += match_volume
            sell_order.executed_volume += match_volume
            
            # 更新冻结量
            with global_lock:
                buy_order.frozen_amount -= match_volume * match_price
                sell_order.frozen_amount -= match_volume

                # 转移资产
                # 买家获得基准代币，卖家获得计价代币
                buy_order.submitter.assets[self.base_token] += match_volume
                sell_order.submitter.assets[self.quote_token] += match_volume * match_price
            
            # 记录交易
            self.log.append((time.time(), match_price, match_volume))
            self.price = match_price
            
            # 检查订单是否完成
            if buy_order.executed_volume >= buy_order.total_volume:
                buy_order.close()
            
            if sell_order.executed_volume >= sell_order.total_volume:
                sell_order.close()
    
    def _insert_order(self, order_list: List[Order], order: Order, reverse: bool) -> None:
        """根据价格和时间插入订单到正确位置，使用二分查找优化"""
        # 使用二分查找找到插入位置
        left, right = 0, len(order_list)
        
        while left < right:
            mid = (left + right) // 2
            
            if reverse:
                # 买单队列：价格降序，时间升序
                # 当前订单应该插入到mid位置之前的情况：
                # 1. 价格更高
                # 2. 价格相同但提交时间更早
                if order.price > order_list[mid].price or \
                   (order.price == order_list[mid].price and order.submit_time < order_list[mid].submit_time):
                    right = mid
                else:
                    left = mid + 1
            else:
                # 卖单队列：价格升序，时间升序
                # 当前订单应该插入到mid位置之前的情况：
                # 1. 价格更低
                # 2. 价格相同但提交时间更早
                if order.price < order_list[mid].price or \
                   (order.price == order_list[mid].price and order.submit_time < order_list[mid].submit_time):
                    right = mid
                else:
                    left = mid + 1
        
        # 在找到的位置插入订单
        order_list.insert(left, order)
