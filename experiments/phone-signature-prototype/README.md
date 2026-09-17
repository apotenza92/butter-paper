# Local phone signature prototype

**Signature Pad 5.1.4 + qrcp** helper, used by the native migration app.
The standalone launcher remains available for development.

## Run

Requires Go and Python 3.12+ on the development machine. Preparation downloads
checksum-pinned MIT-licensed sources plus Go dependencies verified by `go.sum`.
The build runs with one compiler job, a ten-minute deadline, a 512 MiB prototype
output budget and the existing host free-space thresholds.

```sh
python3 prepare.py
python3 test_transfer.py
python3 run.py --bind 192.168.1.25
```

Replace the example address with this computer's LAN IPv4 address. Both devices
must be on a network that permits device-to-device connections. On macOS,
`ipconfig getifaddr en0` commonly gives the Wi-Fi address; verify the interface.
Without `--bind`, the launcher uses loopback for same-computer testing only.
The launcher prints a QR image path and phone URL. Open the QR image and scan it.

The page bundles Signature Pad locally and captures at 2048 pixels on its longest
side independently of display density. It draws, clears, preserves strokes when
resized, and sends one PNG directly to the paired computer. The launcher reports
receipt and discards the image. The temporary QR image is also removed on exit.
No CDN, Cloudflare, Firebase, account or phone installation is used at runtime.

## Adapter and boundaries

`qrcp-memory-adapter.patch` adds optional HTTP/receive hooks to pinned qrcp and
bounds HTTP timeouts. qrcp still owns network binding, QR generation, the receive
route and shutdown. The adapter uses a 256-bit random path, rejects unrelated
routes/hosts/origins, caps uploads at 1 MiB, validates PNG decoding/dimensions,
and returns image bytes over stdout instead of qrcp's general file-writing path.
Do not redirect that process pipe to a log when using a real signature.

The listener closes after successful receipt, cancellation or expiry (maximum
five minutes). The source does not save the signature to disk; process memory,
OS buffers or swap are not a secure-erasure guarantee. HTTP remains unencrypted
and cannot prevent interception or tampering by an attacker on the network.
This is a trusted-network prototype, not production security qualification.

## Verification

- Five automated process/HTTP tests cover byte-exact PNG receipt, rejected
  payloads/origins/routes, image mode, cancellation, expiry, parent-pipe closure
  and the closed listener.
- Browser checks cover blank rejection, drawing, Clear, a 390×844 layout,
  resize preservation, successful send and canvas clearing after receipt.
- Physical phone scanning/touch, LAN firewall behaviour and integration with
  physical-device placement/Recent storage remain unverified.

Sources and checksums are in `sources.json`. qrcp and Signature Pad use MIT;
upstream notices are preserved in ignored `dist/` and the bundled assets.
Generated sources, caches, binary and disposable evidence stay under `dist/`.
