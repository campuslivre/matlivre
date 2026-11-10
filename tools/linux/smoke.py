"""Exercise built Linux binaries without a display, MATLAB, or third-party Python packages."""

import argparse
import base64
import json
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import xml.etree.ElementTree as ET


def check_png(data):
    assert data[:8] == b"\x89PNG\r\n\x1a\n", "expected a PNG"
    assert data[12:16] == b"IHDR", "expected PNG dimensions"
    width, height = struct.unpack(">II", data[16:24])
    assert width > 0 and height > 0, "figure must have positive dimensions"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, default=Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "debug")
    binaries = parser.parse_args().bin_dir.resolve()
    cli = binaries / "matlivre-cli"
    app = binaries / "matlivre"
    assert cli.is_file() and app.is_file(), "build the workspace binaries first"

    with tempfile.TemporaryDirectory(prefix="matlivre-linux-smoke-") as temporary:
        root = Path(temporary)
        # Never connect to an existing GUI's MCP server or write personal settings.
        env = dict(os.environ, APPDATA=str(root / "settings"), XDG_CONFIG_HOME=str(root / "config"))
        script = root / "linear.m"
        script.write_text("A = [3 2 -1; 2 -2 4; -1 0.5 -1];\nb = [1; -2; 0];\nassert(norm(A\\b - [1; -2; -2]) < 1e-10);\ndisp('CLI_OK');\n")
        result = subprocess.run([str(cli), str(script)], env=env, cwd=root, capture_output=True, text=True, timeout=30, check=True)
        assert "CLI_OK" in result.stdout, result.stdout
        print("PASS: CLI runs a script and solves a linear system")

        script.write_text("assert(false);\n")
        result = subprocess.run([str(cli), str(script)], env=env, cwd=root, capture_output=True, text=True, timeout=30)
        assert result.returncode == 1, "CLI must report a script failure"
        print("PASS: CLI reports script errors with a failing exit status")

        code = "figure; plot([0 1], [0 1]); title('Linux smoke ABC 123'); exportgraphics(gcf, 'figure.png'); exportgraphics(gcf, 'figure.svg'); disp('MCP_OK');"
        messages = [
            {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-03-26"}},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "run_code", "arguments": {"code": code}}},
        ]
        result = subprocess.run([str(app), "--mcp"], input="".join(json.dumps(message) + "\n" for message in messages), env=env, cwd=root, capture_output=True, text=True, timeout=45, check=True)
        replies = [json.loads(line) for line in result.stdout.splitlines()]
        assert [reply.get("id") for reply in replies] == [1, 2], "MCP must answer both requests"
        assert replies[0]["result"]["serverInfo"]["name"] == "matlivre"
        response = replies[1]["result"]
        assert not response.get("isError"), response
        assert any("MCP_OK" in item.get("text", "") for item in response["content"]), response
        images = [item for item in response["content"] if item["type"] == "image"]
        assert images, "MCP must return the figure as an image"
        for image in images:
            assert image["mimeType"] == "image/png"
            check_png(base64.b64decode(image["data"], validate=True))
        check_png((root / "figure.png").read_bytes())
        svg = ET.parse(root / "figure.svg")
        assert "Linux smoke ABC 123" in "".join(svg.getroot().itertext())
        print("PASS: headless MCP initializes, runs code, returns PNG, and exports PNG/SVG")


if __name__ == "__main__":
    main()
