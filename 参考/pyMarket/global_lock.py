"""
全局锁管理模块
用于管理全项目范围的线程锁，避免循环导入问题
"""

import threading

# 创建全局线程锁
global_lock = threading.Lock()