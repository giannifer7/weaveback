#!/usr/bin/env python3
# serve_graph.py
# I’d Really Rather You Didn’t edit this generated file.
import sqlite3
import json
import sys
import os


import http.server
import socketserver
import os

class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_request(self, code='-', size='-'):
        # completely squelch livereload logging
        if self.path == '/__events':
            return
        super().log_request(code, size)

    def do_GET(self):
        if self.path == '/__events':
            # Send an infinite retry SSE to quiet the livereload script
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.send_header('Cache-Control', 'no-cache')
            self.end_headers()
            self.wfile.write(b'retry: 3600000\n\n')
            return
        super().do_GET()

def main():
    port = 8000
    os.chdir("../..")
    socketserver.TCPServer.allow_reuse_address = True
    with socketserver.TCPServer(("", port), QuietHandler) as httpd:
        print(f"Opening visualization at http://localhost:{port}/examples/visualization/")
        httpd.serve_forever()


if __name__ == "__main__":
    main()
