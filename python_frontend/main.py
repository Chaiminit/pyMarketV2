#!/usr/bin/env python3
"""
PyMarket V2 - Python Frontend
Communicates with Rust backend for high-performance market simulation
"""

import subprocess
import sys
import time
import threading
import os
from decimal import Decimal
from typing import Dict, Tuple

# Add parent directory to path for imports
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from client import BackendClient
from gui import start_gui


def start_backend() -> subprocess.Popen:
    """Start the Rust backend process"""
    # Determine the backend executable path
    backend_dir = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), 'rust_backend')

    # Check if compiled binary exists
    if os.name == 'nt':  # Windows
        exe_path = os.path.join(backend_dir, 'target', 'release', 'pymarket_backend.exe')
        if not os.path.exists(exe_path):
            exe_path = os.path.join(backend_dir, 'target', 'debug', 'pymarket_backend.exe')
    else:  # Unix-like
        exe_path = os.path.join(backend_dir, 'target', 'release', 'pymarket_backend')
        if not os.path.exists(exe_path):
            exe_path = os.path.join(backend_dir, 'target', 'debug', 'pymarket_backend')

    if not os.path.exists(exe_path):
        print("Backend executable not found. Building...")
        # Build the backend
        build_result = subprocess.run(
            ['cargo', 'build', '--release'],
            cwd=backend_dir,
            capture_output=True,
            text=True
        )
        if build_result.returncode != 0:
            print(f"Build failed: {build_result.stderr}")
            # Try debug build
            build_result = subprocess.run(
                ['cargo', 'build'],
                cwd=backend_dir,
                capture_output=True,
                text=True
            )
            if build_result.returncode != 0:
                print(f"Debug build also failed: {build_result.stderr}")
                sys.exit(1)

        # Recalculate path
        if os.name == 'nt':
            exe_path = os.path.join(backend_dir, 'target', 'release', 'pymarket_backend.exe')
            if not os.path.exists(exe_path):
                exe_path = os.path.join(backend_dir, 'target', 'debug', 'pymarket_backend.exe')
        else:
            exe_path = os.path.join(backend_dir, 'target', 'release', 'pymarket_backend')
            if not os.path.exists(exe_path):
                exe_path = os.path.join(backend_dir, 'target', 'debug', 'pymarket_backend')

    print(f"Starting backend: {exe_path}")
    process = subprocess.Popen(
        [exe_path],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True
    )

    # Wait for backend to start
    time.sleep(2)

    # Check if process is still running
    if process.poll() is not None:
        stdout, stderr = process.communicate()
        print(f"Backend failed to start:")
        print(f"stdout: {stdout}")
        print(f"stderr: {stderr}")
        sys.exit(1)

    return process


def setup_market(client: BackendClient) -> None:
    """Setup the market with tokens, trading pairs, and bots"""
    print("Setting up market...")

    # Create tokens
    print("Creating tokens...")
    client.create_token("USDT")
    client.create_token("BTC")
    client.create_token("ETH")

    # Create trading pairs
    print("Creating trading pairs...")
    client.create_trading_pair("BTC", "USDT", Decimal("40000"))
    client.create_trading_pair("ETH", "USDT", Decimal("2500"))

    # Create bots
    print("Creating bots...")
    asset_configs: Dict[str, Tuple[Decimal, Decimal]] = {
        "USDT": (Decimal("10000"), Decimal("1000000")),
        "BTC": (Decimal("1"), Decimal("100")),
        "ETH": (Decimal("20"), Decimal("2000"))
    }

    client.create_bots(
        count=100,
        asset_configs=asset_configs,
        name_prefix="MarketBot",
        trend=50.0,
        view=30.0
    )

    print("Market setup complete!")


def create_player(client: BackendClient) -> int:
    """Create a player trader"""
    assets = {
        "USDT": Decimal("100000000"),
        "BTC": Decimal("0"),
        "ETH": Decimal("0")
    }

    response = client.create_player("Player", assets)
    if response.get('type') == 'player_created':
        trader_id = response['trader_id']
        print(f"Player created with ID: {trader_id}")
        return trader_id
    else:
        print(f"Failed to create player: {response}")
        return 0


def game_loop(client: BackendClient, player_id: int, trading_pairs: list):
    """Interactive game loop for player"""
    print("\n=== Game Commands ===")
    print("b<n> <amount> - Buy on trading pair n (e.g., b1 10.5)")
    print("s<n> <amount> - Sell on trading pair n (e.g., s1 5.0)")
    print("info - Show player info")
    print("quit - Exit game")
    print("===================\n")

    while True:
        try:
            cmd = input(">> ").strip()

            if not cmd:
                continue

            if cmd == "quit":
                break

            if cmd == "info":
                response = client.get_trader_info(player_id)
                if response.get('type') == 'trader_info':
                    info = response['info']
                    print(f"\nPlayer: {info['name']}")
                    print(f"Assets: {info['assets']}")
                    print(f"Orders: {info['orders']}")
                else:
                    print(f"Error: {response}")
                continue

            # Parse buy/sell commands
            if cmd[0] in ('b', 's') and len(cmd) > 1:
                try:
                    pair_idx = int(cmd[1]) - 1
                    if pair_idx < 0 or pair_idx >= len(trading_pairs):
                        print(f"Invalid trading pair index. Use 1-{len(trading_pairs)}")
                        continue

                    amount = Decimal(cmd[2:].strip())
                    direction = "buy" if cmd[0] == 'b' else "sell"
                    pair_id = trading_pairs[pair_idx]

                    response = client.submit_market_order(
                        player_id, pair_id, direction, amount
                    )

                    if response.get('type') == 'market_order_executed':
                        trades = response.get('trades', [])
                        print(f"Order executed! {len(trades)} trades")
                    else:
                        print(f"Order failed: {response.get('message', 'Unknown error')}")

                except (ValueError, IndexError) as e:
                    print(f"Invalid command format: {e}")
            else:
                print("Unknown command")

        except KeyboardInterrupt:
            break
        except Exception as e:
            print(f"Error: {e}")


def main():
    """Main function"""
    print("=== PyMarket V2 ===")
    print("High-performance market simulation with Rust backend\n")

    # Start backend
    backend_process = None
    try:
        backend_process = start_backend()
        print("Backend started successfully\n")

        # Connect to backend
        time.sleep(1)  # Wait for server to be ready
        client = BackendClient()

        # Setup market
        setup_market(client)

        # Get trading pairs
        response = client.get_all_trading_pairs()
        if response.get('type') == 'trading_pairs_list':
            trading_pairs = [p['id'] for p in response['pairs']]
            print(f"Trading pairs: {trading_pairs}")
        else:
            print("Failed to get trading pairs")
            trading_pairs = []

        # Create player
        player_id = create_player(client)

        # Start simulation
        print("\nStarting simulation...")
        response = client.start_simulation()
        if response.get('type') == 'success':
            print("Simulation started!")
        else:
            print(f"Failed to start simulation: {response}")

        # Start game loop in separate thread
        if player_id > 0 and trading_pairs:
            game_thread = threading.Thread(
                target=game_loop,
                args=(client, player_id, trading_pairs),
                daemon=True
            )
            game_thread.start()

        # Start GUI (blocks until closed)
        print("\nStarting GUI...")
        start_gui(client)

    except KeyboardInterrupt:
        print("\nShutting down...")
    except Exception as e:
        print(f"Error: {e}")
        import traceback
        traceback.print_exc()
    finally:
        # Stop backend
        if backend_process:
            print("Stopping backend...")
            backend_process.terminate()
            try:
                backend_process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                backend_process.kill()

        print("Goodbye!")


if __name__ == "__main__":
    main()
