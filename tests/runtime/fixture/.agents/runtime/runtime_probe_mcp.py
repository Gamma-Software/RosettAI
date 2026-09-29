#!/usr/bin/env python3

import json
import sys
from pathlib import Path


def send(message):
    sys.stdout.write(json.dumps(message, separators=(",", ":")) + "\n")
    sys.stdout.flush()


for line in sys.stdin:
    try:
        request = json.loads(line)
        method = request.get("method")
        request_id = request.get("id")

        if method == "initialize":
            send(
                {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "result": {
                        "protocolVersion": "2025-06-18",
                        "capabilities": {"tools": {}},
                        "serverInfo": {"name": "rosettai-runtime-probe", "version": "1.0.0"},
                    },
                }
            )
        elif method == "tools/list":
            send(
                {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "result": {
                        "tools": [
                            {
                                "name": "record_runtime_probe",
                                "description": "Record deterministic proof that the RosettAI MCP server was invoked.",
                                "inputSchema": {"type": "object", "properties": {}, "additionalProperties": False},
                            }
                        ]
                    },
                }
            )
        elif method == "tools/call":
            Path("mcp-runtime-proof.txt").write_text("ROSETTAI_MCP_LOADED\n", encoding="utf-8")
            send(
                {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "result": {
                        "content": [{"type": "text", "text": "ROSETTAI_MCP_LOADED"}],
                        "isError": False,
                    },
                }
            )
        elif request_id is not None:
            send(
                {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "error": {"code": -32601, "message": f"Method not found: {method}"},
                }
            )
    except Exception as error:
        print(f"runtime probe error: {error}", file=sys.stderr, flush=True)
