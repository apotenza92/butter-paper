#!/usr/bin/env python3
"""Interactive prototype launcher: show pairing QR, verify receipt, discard image."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--bind', default='127.0.0.1', help='private IPv4 address of this computer; default tests on this computer only')
args = parser.parse_args()
process = subprocess.Popen([str(root / 'dist/signature-prototype'), '--bind', args.bind], stdout=subprocess.PIPE, text=True)
try:
    with tempfile.TemporaryDirectory(prefix='bp-signature-qr-') as directory:
        ready = json.loads(process.stdout.readline())
        path = Path(directory) / 'pairing.png'
        path.write_bytes(base64.b64decode(ready['qrPng']))
        os.chmod(path, 0o600)
        print('Open this QR image and scan it with a phone on the same network:', path, flush=True)
        print('Or open:', ready['url'], flush=True)
        print('HTTP is unencrypted. This session stops after one upload, Ctrl+C or five minutes.', flush=True)
        result = json.loads(process.stdout.readline())
        if result['event'] == 'received':
            image = bytearray(base64.b64decode(result.pop('png')))
            if hashlib.sha256(image).hexdigest() != result['sha256']: raise RuntimeError('Image checksum mismatch')
            print(f'Received {len(image)} PNG bytes; discarded by this isolated prototype.')
            image[:] = bytes(len(image))
        else: print('Session', result['event'])
except KeyboardInterrupt:
    print('\nCancelling local session.')
finally:
    if process.poll() is None: process.terminate()
    try: process.wait(timeout=12)
    except subprocess.TimeoutExpired: process.kill(); process.wait()
    process.stdout.close()
