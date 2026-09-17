// Isolated qrcp adapter. No application documents or persistent signature store.
package main

import (
	"bytes"
	"crypto/rand"
	"crypto/sha256"
	"embed"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"flag"
	"fmt"
	"image/png"
	"io"
	"net"
	"net/http"
	"os"
	"os/signal"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/claudiodangelis/qrcp/config"
	"github.com/claudiodangelis/qrcp/qr"
	"github.com/claudiodangelis/qrcp/server"
)

//go:embed assets/*
var assets embed.FS

const maxBytes = 1024 * 1024

func main() {
	bind := flag.String("bind", "127.0.0.1", "exact local interface IP; use a LAN IP for a physical phone")
	mode := flag.String("mode", "draw", "draw or image")
	watchParent := flag.Bool("watch-parent", false, "stop when the application pipe closes")
	ttl := flag.Duration("ttl", 5*time.Minute, "session lifetime, at most five minutes")
	flag.Parse()
	if *mode != "draw" && *mode != "image" {
		panic("invalid mode")
	}
	if *bind == "auto" {
		interfaces, _ := net.Interfaces()
		for _, iface := range interfaces {
			if iface.Flags&net.FlagUp == 0 || iface.Flags&net.FlagLoopback != 0 || iface.Flags&net.FlagPointToPoint != 0 {
				continue
			}
			addresses, _ := iface.Addrs()
			for _, address := range addresses {
				candidate, _, _ := net.ParseCIDR(address.String())
				if candidate.To4() != nil && candidate.IsPrivate() {
					*bind = candidate.String()
					break
				}
			}
			if *bind != "auto" {
				break
			}
		}
	}
	if *ttl <= 0 || *ttl > 5*time.Minute {
		panic("invalid session lifetime")
	}
	ip := net.ParseIP(*bind)
	if ip == nil || ip.To4() == nil || (!ip.IsPrivate() && !ip.IsLoopback()) {
		panic("bind must be a private or loopback IPv4 interface address")
	}
	ifaces, err := net.Interfaces()
	if err != nil {
		panic(err)
	}
	ifaceName := ""
	for _, iface := range ifaces {
		addresses, _ := iface.Addrs()
		for _, address := range addresses {
			a, _, _ := net.ParseCIDR(address.String())
			if a.Equal(ip) {
				ifaceName = iface.Name
			}
		}
	}
	if ifaceName == "" {
		panic("bind address does not belong to this computer")
	}
	token := make([]byte, 32)
	if _, err = rand.Read(token); err != nil {
		panic(err)
	}
	path := "/receive/" + base64.RawURLEncoding.EncodeToString(token)
	clear(token)
	expires := time.Now().Add(*ttl)
	var mu sync.Mutex
	var received []byte
	var origin string
	// qrcp's registered receive route remains the transfer entry point.
	cfg := config.Config{Interface: ifaceName, Bind: *bind, Path: strings.TrimPrefix(path, "/receive/")}
	cfg.ReceiveHandler = func(w http.ResponseWriter, r *http.Request) bool {
		mu.Lock()
		defer mu.Unlock()
		if time.Now().After(expires) || received != nil {
			http.Error(w, "Session unavailable", http.StatusGone)
			return false
		}
		if r.Method == http.MethodGet {
			b, _ := assets.ReadFile("assets/index.html")
			w.Header().Set("Content-Type", "text/html; charset=utf-8")
			page := strings.ReplaceAll(string(b), "./PLACEHOLDER/", path+"/")
			page = strings.ReplaceAll(page, "MODE_PLACEHOLDER", *mode)
			w.Write([]byte(page))
			return false
		}
		if r.Method != http.MethodPost {
			http.Error(w, "Method not allowed", 405)
			return false
		}
		if r.Header.Get("Origin") != origin {
			http.Error(w, "Origin rejected", 403)
			return false
		}
		if r.Header.Get("Content-Type") != "image/png" {
			http.Error(w, "Use PNG", 415)
			return false
		}
		body, err := io.ReadAll(http.MaxBytesReader(w, r.Body, maxBytes))
		if err != nil {
			clear(body)
			http.Error(w, "Image exceeds limit", 413)
			return false
		}
		defer clear(body)
		dimensions, err := png.DecodeConfig(bytes.NewReader(body))
		if err != nil || dimensions.Width < 1 || dimensions.Height < 1 || dimensions.Width > 4096 || dimensions.Height > 4096 || dimensions.Width*dimensions.Height > 4*1024*1024 {
			http.Error(w, "Invalid image dimensions", 422)
			return false
		}
		// Decode before accepting; this prototype returns the original bounded PNG.
		if _, err = png.Decode(bytes.NewReader(body)); err != nil {
			http.Error(w, "Invalid PNG", 422)
			return false
		}
		received = bytes.Clone(body)
		w.WriteHeader(http.StatusCreated)
		w.Write([]byte("Signature received"))
		return true
	}
	cfg.Handler = http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Cache-Control", "no-store")
		w.Header().Set("Referrer-Policy", "no-referrer")
		w.Header().Set("X-Content-Type-Options", "nosniff")
		w.Header().Set("Content-Security-Policy", "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data: blob:; frame-ancestors 'none'; base-uri 'none'; form-action 'self'")
		mu.Lock()
		expectedHost := strings.TrimPrefix(origin, "http://")
		mu.Unlock()
		if expectedHost == "" || r.Host != expectedHost {
			http.Error(w, "Host rejected", 403)
			return
		}
		if r.URL.RawQuery != "" {
			http.NotFound(w, r)
			return
		}
		if r.URL.Path == path {
			http.DefaultServeMux.ServeHTTP(w, r)
			return
		}
		for _, name := range []string{"app.js", "signature_pad.umd.min.js", "style.css"} {
			if r.Method == "GET" && r.URL.Path == path+"/"+name {
				b, _ := assets.ReadFile("assets/" + name)
				if strings.HasSuffix(name, ".css") {
					w.Header().Set("Content-Type", "text/css")
				} else {
					w.Header().Set("Content-Type", "text/javascript")
				}
				w.Write(b)
				return
			}
		}
		http.NotFound(w, r)
	})
	srv, err := server.New(&cfg)
	if err != nil {
		panic(err)
	}
	mu.Lock()
	origin = srv.BaseURL
	mu.Unlock()
	var qrPng bytes.Buffer
	png.Encode(&qrPng, qr.RenderImage(srv.ReceiveURL))
	json.NewEncoder(os.Stdout).Encode(map[string]any{"event": "ready", "url": srv.ReceiveURL, "expiresAt": expires.UnixMilli(), "qrPng": base64.StdEncoding.EncodeToString(qrPng.Bytes())})
	done := make(chan error, 1)
	go func() { done <- srv.Wait() }()
	signals := make(chan os.Signal, 1)
	signal.Notify(signals, os.Interrupt, syscall.SIGTERM)
	defer signal.Stop(signals)
	timer := time.NewTimer(time.Until(expires))
	defer timer.Stop()
	parentClosed := make(chan struct{})
	if *watchParent {
		go func() { io.Copy(io.Discard, os.Stdin); close(parentClosed) }()
	}
	reason := "received"
	select {
	case err = <-done:
	case <-parentClosed:
		reason = "cancelled"
		srv.Shutdown()
		err = <-done
	case <-signals:
		reason = "cancelled"
		srv.Shutdown()
		err = <-done
	case <-timer.C:
		reason = "expired"
		srv.Shutdown()
		err = <-done
	}
	if err != nil {
		fmt.Fprintln(os.Stderr, "Server shutdown failed")
		os.Exit(1)
	}
	mu.Lock()
	defer mu.Unlock()
	defer clear(received)
	if len(received) > 0 {
		sum := sha256.Sum256(received)
		json.NewEncoder(os.Stdout).Encode(map[string]any{"event": "received", "png": base64.StdEncoding.EncodeToString(received), "bytes": len(received), "sha256": hex.EncodeToString(sum[:])})
	} else {
		json.NewEncoder(os.Stdout).Encode(map[string]string{"event": reason})
	}
}
