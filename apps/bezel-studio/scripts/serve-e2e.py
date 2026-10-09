"""The static server of the e2e run: the studio's src/ as Tauri embeds it.

`python -m http.server` listens with a backlog of 5 connections. Parallel
Playwright workers open more than that at once while a page loads its ES
modules, and on Windows the connections past the backlog are refused, so a
module never loads and the page stays half built. It also speaks HTTP/1.0,
one connection per file: a full run opens tens of thousands of them and
Windows runs out of sockets (net::ERR_NO_BUFFER_SPACE). This one keeps a
deep backlog, answers each connection on its own thread and keeps
connections alive (HTTP/1.1; every answer has its Content-Length).
"""

import functools
import http.server
import sys


class Server(http.server.ThreadingHTTPServer):
    request_queue_size = 256
    daemon_threads = True


class Handler(http.server.SimpleHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, format, *args):  # noqa: A002 - the base class's name
        pass


def main() -> None:
    port = int(sys.argv[1])
    directory = sys.argv[2]
    handler = functools.partial(Handler, directory=directory)
    with Server(("127.0.0.1", port), handler) as server:
        server.serve_forever()


if __name__ == "__main__":
    main()
