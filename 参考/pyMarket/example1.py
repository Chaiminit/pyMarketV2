#!/usr/bin/env python3

import sys
import time
import threading
import subprocess
import random
from typing import List
from decimal import Decimal, getcontext
from trader import Trader

# 设置decimal精度
getcontext().prec = 28

# 导入自定义模块
from finance import Token, TradingPair
from bot import BotManager, RandomBot
from gui import start_gui
from global_lock import global_lock


def market_simulation(trading_pairs: List[TradingPair], bot_manager: BotManager, stop_event: threading.Event):
    """市场模拟函数，在后台线程中运行（优化版本）"""
    print("开始市场模拟...")
    step_interval = 10  # 降低模拟步长，减少CPU占用
    
    try:
        while not stop_event.is_set():
            # 使用BotManager的step方法来统一管理机器人交易
            all_trading_pairs = trading_pairs
            bot_manager.step(all_trading_pairs)

            time.sleep(step_interval/len(bot_manager.bots))
    
    except KeyboardInterrupt:
        print("\n市场模拟已停止")


def game(player: Trader, trading_pairs: List[TradingPair]):
    while True:
        op = input('>>')
        try:
            if op == 'quit':
                break
            elif op == '':
                pass
            else:
                if op[0] == 'b':
                    num = Decimal(op[3:])
                    print(player.submit_market(trading_pairs[int(op[1])-1], 'buy', num))
                elif op[0] == 's':
                    num = Decimal(op[3:])
                    print(player.submit_market(trading_pairs[int(op[1])-1], 'sell', num))

        except Exception as e:
            print(e)

        finally:
            print(player.get_total_value())
            print(player.assets)


def main():
    """主函数"""
    print("=== 市场模拟系统启动 ===")
    
    # 检查依赖
    print("检查依赖...")
    check_and_install_dependencies()
    
    # 创建代币
    print("创建代币...")
    usdt = Token("USDT")
    btc = Token("BTC")
    eth = Token("ETHER")

    # 创建交易对
    print("创建交易对...")
    btc_usdt = TradingPair(btc, usdt,4000)
    eth_usdt = TradingPair(eth, usdt,190)
    
    trading_pairs = [eth_usdt]
    
    # 创建机器人管理器并批量创建机器人
    print("初始化机器人管理器...")
    bot_manager = BotManager()
    
    # 批量创建100个交易机器人
    num_bots = 100
    
    # 定义资产配置方案（包含债券代币）
    asset_configs = {
        usdt: {"min": Decimal('10000'), "max": Decimal('1000000')},  # USDT
        btc: {"min": Decimal('1'), "max": Decimal('100')},    # BTC
        eth: {"min": Decimal('20'), "max": Decimal('2000')}
    }
    
    # 批量创建机器人并分配资产
    bot_manager.create_bots_batch(
        count=num_bots,
        asset_configs=asset_configs,
        name_prefix="MarketBot",
        trend=200,
        view=30,
    )
    
    # 为所有机器人设置可交易的交易对（包含债券交易对）
    all_trading_pairs = trading_pairs
    bot_manager.set_trading_pairs(all_trading_pairs)
    
    player = Trader('Player')
    player.assets = {usdt: Decimal('100000000'), btc: Decimal('0')}

    print(f"交易对: {[f'{pair.base_token.name}/{pair.quote_token.name}' for pair in all_trading_pairs]}")
    
    # 创建停止事件
    stop_event = threading.Event()
    
    # 启动市场模拟线程
    sim_thread = threading.Thread(target=market_simulation, args=(trading_pairs, bot_manager, stop_event), daemon=True)
    sim_thread.start()

    time.sleep(1)
    # for i in all_trading_pairs:
    #     i.log = i.log[20:]

    game_thread = threading.Thread(target=game, args=(player, all_trading_pairs,), daemon=True)
    game_thread.start()

    # 启动GUI（在主线程中运行，这会阻塞主线程）
    print("启动GUI...")
    try:
        start_gui(all_trading_pairs)
    except Exception as e:
        print(f"GUI出错: {e}")
    finally:
        # GUI关闭时停止市场模拟
        stop_event.set()
        print("程序已退出")


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\n程序已退出")
    except Exception as e:
        print(f"程序出错: {e}")
        import traceback
        traceback.print_exc()