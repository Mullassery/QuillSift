"""End-to-end smoke tests against a local HTTP server.

Run after installing the wheel into a virtualenv:
    python tests/smoke.py
"""

import json
import os
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import scrapeowl

HTML = (
    b"<html><head><title>Demo</title>"
    b"<meta name='description' content='demo page'></head>"
    b"<body><h1>Hello</h1><p>Body text.</p>"
    b"<a href='/next'>next</a></body></html>"
)
SCHEMA = {
    "type": "object",
    "required": ["title"],
    "properties": {"title": {"type": "string"}},
}


class Handler(BaseHTTPRequestHandler):
    calls = 0
    fail_first = False

    def _send(self, body, content_type):
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        self._send(HTML, "text/html; charset=utf-8")

    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        self.rfile.read(length)
        type(self).calls += 1
        if self.path.endswith("/chat/completions"):
            assert self.headers.get("Authorization") == "Bearer test-token"
            content = (
                "{}"
                if type(self).fail_first and type(self).calls == 1
                else '{"title":"Extracted"}'
            )
            body = {"choices": [{"message": {"role": "assistant", "content": content}}]}
            self._send(json.dumps(body).encode(), "application/json")
        elif self.path.endswith("/api/chat"):
            body = {"message": {"role": "assistant", "content": '{"title":"Extracted"}'}}
            self._send(json.dumps(body).encode(), "application/json")
        else:
            self.send_error(404)

    def log_message(self, *args):
        pass


def main():
    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    base = "http://127.0.0.1:%d" % server.server_address[1]

    try:
        markdown = scrapeowl.fetch(base, allow_private=True)
        assert isinstance(markdown, str) and "Hello" in markdown, markdown

        text = scrapeowl.fetch(base, format="text", allow_private=True)
        assert "Body text." in text, text

        data = scrapeowl.fetch(base, format="json", allow_private=True)
        assert set(data) >= {"url", "text", "markdown", "links", "images", "metadata"}
        assert data["metadata"]["title"] == "Demo"
        assert data["links"] and data["links"][0].startswith(base), data["links"]

        try:
            scrapeowl.fetch(base)
        except PermissionError:
            pass
        else:
            raise AssertionError("loopback fetch should have been blocked")

        report = scrapeowl.crawl(
            base, max_depth=1, max_pages=3, allow_private=True
        )
        assert set(report) == {"seeds", "pages", "failures", "truncated"}, report
        assert len(report["pages"]) == 2, report
        assert report["pages"][0]["url"] == base + "/", report["pages"][0]
        assert report["pages"][1]["url"] == base + "/next", report["pages"][1]
        assert report["pages"][0]["content"]["metadata"]["title"] == "Demo"
        assert not report["truncated"], report
        assert not report["failures"], report["failures"]

        try:
            scrapeowl.crawl(base, max_pages=0, allow_private=True)
        except ValueError:
            pass
        else:
            raise AssertionError("zero max_pages should raise ValueError")

        os.environ["TEST_API_KEY"] = "test-token"
        Handler.calls = 0
        Handler.fail_first = True
        result = scrapeowl.extract(
            base,
            SCHEMA,
            base_url=base + "/v1",
            model="test-model",
            api_key_env="TEST_API_KEY",
            allow_private=True,
            max_attempts=2,
        )
        assert result == {"title": "Extracted"}, result
        assert Handler.calls == 2, Handler.calls

        result = scrapeowl.extract(
            base, json.dumps(SCHEMA), provider="ollama",
            base_url=base, model="test-model", allow_private=True,
        )
        assert result == {"title": "Extracted"}, result

        try:
            scrapeowl.extract(base, SCHEMA, provider="unknown", model="m")
        except ValueError:
            pass
        else:
            raise AssertionError("unknown provider should raise ValueError")

        try:
            scrapeowl.extract(base, {"type": "not-a-type"}, model="m")
        except ValueError:
            pass
        else:
            raise AssertionError("invalid schema should raise ValueError")

        print("python smoke tests passed (version %s)" % scrapeowl.__version__)
    finally:
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    main()
