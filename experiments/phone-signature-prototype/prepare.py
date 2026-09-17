#!/usr/bin/env python3
"""Prepare pinned upstream sources and build the isolated prototype. No deployment."""
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import signal
import time
import tarfile
import urllib.request

root = Path(__file__).resolve().parent
dist = root / 'dist'
dist.mkdir(exist_ok=True)
policy = json.loads((root.parent / 'gpui-migration/gpui-migration/build-guard-policy.json').read_text())
if shutil.disk_usage(root).free // 1024 < policy['preflightFreeKiB']:
    raise SystemExit('Insufficient host storage for prototype preparation')
sources = json.loads((root / 'sources.json').read_text())
for name, source in sources.items():
    archive = dist / (name + '.tar.gz')
    if not archive.exists():
        archive.write_bytes(urllib.request.urlopen(source['url'], timeout=30).read())
    data = archive.read_bytes()
    algorithm = 'sha256' if 'sha256' in source else 'sha512'
    if hashlib.new(algorithm, data).hexdigest() != source[algorithm]:
        raise SystemExit(f'{name} checksum mismatch')
    with tarfile.open(fileobj=io.BytesIO(data)) as tar:
        tar.extractall(dist, filter='data')
upstream = dist / ('qrcp-' + sources['qrcp']['revision'])
subprocess.run(['patch', '-p1', '--fuzz=0', '-i', str(root / 'qrcp-memory-adapter.patch')], cwd=upstream, check=True)
cmd = upstream / 'cmd/bp-prototype'
cmd.mkdir(parents=True, exist_ok=True)
shutil.copy2(root / 'main.go', cmd / 'main.go')
shutil.copytree(root / 'web', cmd / 'assets', dirs_exist_ok=True)
shutil.copy2(dist / 'package/dist/signature_pad.umd.min.js', cmd / 'assets/signature_pad.umd.min.js')
shutil.copy2(dist / 'package/LICENSE', cmd / 'assets/SIGNATURE_PAD_LICENSE')
shutil.copy2(upstream / 'LICENSE', dist / 'QRCP_LICENSE')
env = os.environ | {'GOTOOLCHAIN': 'local', 'GOMAXPROCS': '2', 'GOCACHE': str(dist / 'gocache'), 'GOMODCACHE': str(dist / 'gomodcache')}
process = subprocess.Popen(['go', 'build', '-mod=readonly', '-p=1', '-o', str(dist / 'signature-prototype'), './cmd/bp-prototype'], cwd=upstream, env=env, start_new_session=True)
started = time.monotonic()
try:
    while process.poll() is None:
        allocated = sum(p.stat().st_size for p in dist.rglob('*') if p.is_file())
        if (time.monotonic() - started > 600 or allocated > 512 * 1024 * 1024
                or shutil.disk_usage(root).free // 1024 < policy['runtimeStopFreeKiB']):
            raise RuntimeError('Prototype build exceeded its time or storage budget')
        time.sleep(1)
    if process.returncode: raise RuntimeError('Prototype build failed')
finally:
    if process.poll() is None:
        os.killpg(process.pid, signal.SIGTERM)
        try: process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
print(dist / 'signature-prototype')
