"""Local deterministic protocol fixture. Never forwards a request to a provider."""
import http.server
import json

FIXED = "def mean(values):\n    if not values:\n        raise ValueError('empty values')\n    return sum(values) / len(values)\n"


class Model(http.server.BaseHTTPRequestHandler):
    calls = 0

    def log_message(self, *args):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        if self.path != "/v1/responses" or body.get("model") != "fixture" or self.headers.get("Authorization") != "Bearer fixture-key":
            self.send_error(400, "Only the local fixture model is allowed")
            return
        type(self).calls += 1
        call = type(self).calls
        if call == 1:
            name, args = "read_file", {"path": "maths.py"}
        elif call == 2:
            name, args = "edit_file", {"path": "maths.py", "old": "def mean(values):\n    return sum(values)\n", "new": FIXED}
        elif call == 3:
            name, args = None, None
        else:
            self.send_error(400, "Unexpected extra fixture request")
            return
        output = ([{"type": "function_call", "name": name, "call_id": f"fixture-{call}", "arguments": json.dumps(args)}]
                  if name else [{"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "Fixed mean; notes.txt is unchanged."}]}])
        value = {"id": f"fixture-response-{call}", "status": "completed", "output": output, "usage": {"input_tokens": 120, "output_tokens": 30}}
        data = ("data: " + json.dumps({"type": "response.completed", "response": value}) + "\n\ndata: [DONE]\n\n").encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


if __name__ == "__main__":
    http.server.HTTPServer(("127.0.0.1", 8765), Model).serve_forever()
