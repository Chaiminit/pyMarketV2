#!/usr/bin/env python3
"""
PyMarket V2 Launcher
Starts both Rust backend and Python frontend
"""

import subprocess
import sys
import os

def main():
    """Launch the application"""
    frontend_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'python_frontend')
    main_script = os.path.join(frontend_dir, 'main.py')

    if not os.path.exists(main_script):
        print(f"Error: Frontend script not found at {main_script}")
        sys.exit(1)

    # Run the frontend (which will start the backend)
    try:
        result = subprocess.run([sys.executable, main_script], cwd=frontend_dir)
        sys.exit(result.returncode)
    except KeyboardInterrupt:
        print("\nApplication terminated by user")
        sys.exit(0)

if __name__ == "__main__":
    main()
