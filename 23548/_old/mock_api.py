from http.server import HTTPServer, BaseHTTPRequestHandler
import json

class SimpleHTTPRequestHandler(BaseHTTPRequestHandler):
    def do_PATCH(self):
        content_length = int(self.headers['Content-Length'])
        post_data = self.rfile.read(content_length)
        data = json.loads(post_data)
        
        print(f"\n[PATCH] {self.path}")
        print(f"Payload: {json.dumps(data, indent=2)}")
        
        self.send_response(200)
        self.send_header('Content-type', 'application/json')
        self.end_headers()
        self.wfile.write(b'{"status": "success"}')

    def do_GET(self):
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b'OK')

httpd = HTTPServer(('localhost', 8080), SimpleHTTPRequestHandler)
print("Mock API Gateway running on port 8080...")
httpd.serve_forever()
