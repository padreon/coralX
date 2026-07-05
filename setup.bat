@echo off
echo Setting up coralX...

where python >nul 2>&1
if errorlevel 1 (
    echo Python not found. Install Python 3.11+ from https://python.org
    pause
    exit /b 1
)

python -m venv .venv
.venv\Scripts\pip install --upgrade pip -q
.venv\Scripts\pip install -r requirements.txt

echo.
echo Setup complete. Run the app with:
echo   .venv\Scripts\python -m src.main
pause
