package main

import (
	"bufio"
	"context"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/tailscale/tailcat"
)

type observedFlow struct {
	tailcat.ConnPacketConn
	snapshots *atomic.Int64
}

func (f observedFlow) Write(data []byte) (int, error) {
	n, err := f.ConnPacketConn.Write(data)
	// Rivet Data header, session token, and Scrapburn snapshot tag.
	if err == nil && n > 16 && string(data[:4]) == "RVNT" && data[6] == 3 && data[15] == 3 {
		f.snapshots.Add(1)
	}
	return n, err
}

// Opt in on a machine with a GPU and the release game. Both processes render offscreen.
func TestReleaseRace(t *testing.T) {
	executable := os.Getenv("SCRAPBURN_TEST_GAME")
	if executable == "" {
		t.Skip("set SCRAPBURN_TEST_GAME to the release executable")
	}
	game, err := filepath.Abs(executable)
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(t.Context(), 40*time.Second)
	defer cancel()
	host := exec.CommandContext(ctx, game, "--host-loopback", "--bench", "18", "--offscreen", "--size", "640x360")
	host.Dir = t.TempDir()
	out, err := host.StdoutPipe()
	if err != nil {
		t.Fatal(err)
	}
	if err := host.Start(); err != nil {
		t.Fatal(err)
	}
	defer func() { cancel(); host.Wait() }()
	lines := bufio.NewScanner(out)
	var port int
	for lines.Scan() {
		value, ok := strings.CutPrefix(lines.Text(), "SCRAPBURN_HOST_PORT=")
		if ok {
			port, err = strconv.Atoi(value)
			break
		}
	}
	if err != nil || port == 0 {
		t.Fatal("release game did not open its private lobby")
	}
	forward := hostFlow(&net.UDPAddr{IP: net.IPv4(127, 0, 0, 1), Port: port})
	var snapshots atomic.Int64
	s := &tailcat.Server{Region: localRegion(t), OnUDP: func(port uint16) func(tailcat.ConnPacketConn) {
		handler := forward(port)
		if handler == nil {
			return nil
		}
		return func(c tailcat.ConnPacketConn) { handler(observedFlow{c, &snapshots}) }
	}}
	if err := s.StartContext(ctx); err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	client := tailcat.NewClient(s.TailcatAddr())
	defer client.Close()
	tunnel, err := client.DialUDPPort(ctx, racePort)
	if err != nil {
		t.Fatal(err)
	}
	local, err := net.ListenUDP("udp4", &net.UDPAddr{IP: net.IPv4(127, 0, 0, 1)})
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- relay(local, tunnel) }()
	defer func() { local.Close(); tunnel.Close(); <-done }()
	guest := exec.CommandContext(ctx, game, "--join", local.LocalAddr().String(), "--bench", "5", "--offscreen", "--size", "640x360")
	guest.Dir = t.TempDir()
	if err := guest.Run(); err != nil {
		t.Fatal(err)
	}
	if snapshots.Load() < 10 {
		t.Fatalf("only %d race snapshots", snapshots.Load())
	}
	t.Logf("Forwarded %d live race snapshots to the guest", snapshots.Load())
}
