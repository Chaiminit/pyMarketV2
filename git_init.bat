@echo off
cd /d F:\OneDrive\Projects\python\pyMarketV2
git init
git remote add origin git@github.com:Chaiminit/pyMarketV2.git
git add .
git commit -m "Initial commit: PyMarket V2"
git branch -M main
git push -u origin main
pause
