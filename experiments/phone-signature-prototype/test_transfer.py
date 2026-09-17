"""Synthetic-only end-to-end checks of the built qrcp adapter."""
import base64
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import unittest
import urllib.error
import urllib.request
import zlib

ROOT = Path(__file__).resolve().parent
BINARY = ROOT / 'dist/signature-prototype'

def png():
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 20, 10, 8, 6, 0, 0, 0)) + chunk(b'IDAT', zlib.compress((b'\x00' + b'\x17\x17\x17\xff' * 20) * 10)) + chunk(b'IEND', b'')

class Transfer(unittest.TestCase):
    def start(self, ttl='10s', extra=()):
        p = subprocess.Popen([str(BINARY), '--ttl', ttl, *extra], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        def cleanup():
            if p.poll() is None: p.terminate(); p.wait(timeout=12)
            p.stdin.close(); p.stdout.close(); p.stderr.close()
        self.addCleanup(cleanup)
        ready = json.loads(p.stdout.readline())
        self.assertEqual(ready['event'], 'ready')
        return p, ready['url']

    def request(self, url, data=None, origin=True, method=None, content_type='image/png'):
        headers = {'Content-Type': content_type}
        if origin: headers['Origin'] = url.rsplit('/receive/', 1)[0]
        try:
            with urllib.request.urlopen(urllib.request.Request(url, data=data, headers=headers, method=method), timeout=12) as r:
                return r.status, r.read(), r.headers
        except urllib.error.HTTPError as e:
            try: return e.code, e.read(), e.headers
            finally: e.close()

    def test_receive_rejects_invalid_requests_then_delivers_exact_png_and_closes(self):
        p, url = self.start()
        status, page, headers = self.request(url)
        self.assertEqual(status, 200)
        self.assertIn(b'Signature drawing area', page)
        self.assertEqual(headers['Cache-Control'], 'no-store')
        self.assertIn("script-src 'self'", headers['Content-Security-Policy'])
        status, library, _ = self.request(url + '/signature_pad.umd.min.js')
        self.assertEqual(status, 200); self.assertIn(b'SignaturePad', library)
        self.assertEqual(self.request(url.replace('/receive/', '/send/'))[0], 404)
        self.assertEqual(self.request(url+'wrong')[0],404)
        self.assertEqual(self.request(url, png(), origin=False)[0],403)
        self.assertEqual(self.request(url, b'not-png')[0],422)
        self.assertEqual(self.request(url, b'x'*(1024*1024+1))[0],413)
        self.assertEqual(self.request(url, png(), content_type='text/plain')[0],415)
        self.assertEqual(self.request(url, png())[0],201)
        result = json.loads(p.stdout.readline())
        self.assertEqual(result['event'],'received')
        self.assertEqual(base64.b64decode(result['png']),png())
        self.assertEqual(result['sha256'],hashlib.sha256(png()).hexdigest())
        self.assertEqual(p.wait(timeout=12),0)
        with self.assertRaises(urllib.error.URLError): self.request(url)

    def test_cancel_releases_listener(self):
        p,url=self.start(); p.terminate()
        self.assertEqual(json.loads(p.stdout.readline())['event'],'cancelled')
        self.assertEqual(p.wait(timeout=12),0)
        with self.assertRaises(urllib.error.URLError): self.request(url)

    def test_parent_pipe_closure_releases_listener(self):
        p,url=self.start(extra=('--watch-parent',))
        p.stdin.close()
        self.assertEqual(json.loads(p.stdout.readline())['event'],'cancelled')
        self.assertEqual(p.wait(timeout=12),0)
        with self.assertRaises(urllib.error.URLError): self.request(url)

    def test_image_mode_serves_upload_page_and_accepts_png(self):
        p,url=self.start(extra=('--mode','image'))
        self.assertIn(b'data-mode="image"', self.request(url)[1])
        self.assertEqual(self.request(url,png())[0],201)
        self.assertEqual(json.loads(p.stdout.readline())['event'],'received')
        self.assertEqual(p.wait(timeout=12),0)

    def test_expiry_releases_listener(self):
        p,url=self.start('200ms')
        self.assertEqual(json.loads(p.stdout.readline())['event'],'expired')
        self.assertEqual(p.wait(timeout=12),0)
        with self.assertRaises(urllib.error.URLError): self.request(url)

if __name__ == '__main__': unittest.main()
