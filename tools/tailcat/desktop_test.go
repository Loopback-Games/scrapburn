package main

import (
	"bufio"
	"context"
	"io"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/tailscale/tailcat"
	"tailscale.com/tailcfg"
	"tailscale.com/types/key"
)

func TestReadInvite(t *testing.T) {
	srv := &tailcat.Server{Region: localRegion(t)}
	if err := srv.StartContext(t.Context()); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { srv.Close() })
	valid := string(srv.TailcatAddr())
	for _, tc := range []struct {
		name, input string
		ok          bool
	}{
		{"pasted line", valid + "\n", true},
		{"surrounding spaces", "  " + valid + " \r\n", true},
		{"no newline before EOF", valid, true},
		{"empty", "", false},
		{"blank line", "\n", false},
		{"not an invite", "hello friends\n", false},
		{"oversized", strings.Repeat("a", maxInvite+10) + "\n", false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			got, err := readInvite(bufio.NewReaderSize(strings.NewReader(tc.input), maxInvite+1))
			if (err == nil) != tc.ok {
				t.Fatalf("readInvite error = %v, want ok=%v", err, tc.ok)
			}
			if tc.ok && string(got) != valid {
				t.Fatalf("readInvite = %q, want %q", got, valid)
			}
		})
	}
}

func TestCompactInvite(t *testing.T) {
	full := tailcat.ConnInfo{
		ServerPublic: tailcat.NodePublic{NodePublic: key.NewNode().Public()},
		Region: []*tailcfg.DERPRegion{{Nodes: []*tailcfg.DERPNode{
			{HostName: "relay.example", IPv4: "203.0.113.7", IPv6: "2001:db8::7"},
		}}},
	}
	full.PresharedKey[0] = 1
	addr := full.Addr()
	short := compactInvite(addr)
	if len(short) >= len(addr) {
		t.Fatalf("compact invite is %d bytes, full is %d", len(short), len(addr))
	}
	got, err := tailcat.ParseAddr(short)
	if err != nil {
		t.Fatal(err)
	}
	if got.ServerPublic != full.ServerPublic || got.PresharedKey != full.PresharedKey {
		t.Fatal("compacting changed the keys")
	}
	n := got.Region[0].Nodes[0]
	if n.HostName != "relay.example" || n.IPv4 != "" || n.IPv6 != "" {
		t.Fatalf("relay node = %+v, want hostname only", n)
	}

	// A relay known only by IP keeps its address.
	full.Region[0].Nodes[0].HostName = ""
	if ipOnly := full.Addr(); compactInvite(ipOnly) != ipOnly {
		t.Fatal("an IP-only relay lost its address")
	}
}

// TestPackagedInvite drives a built helper the way the game does: the host prints an
// invite, and a guest that is handed that invite on stdin opens a working tunnel.
func TestPackagedInvite(t *testing.T) {
	helper := os.Getenv("SCRAPBURN_TEST_HELPER")
	if helper == "" {
		t.Skip("set SCRAPBURN_TEST_HELPER to a built scrapburn-tailcat")
	}
	helper, err := filepath.Abs(helper)
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(t.Context(), 25*time.Second)
	defer cancel()
	start := func(args ...string) (io.WriteCloser, *bufio.Scanner) {
		cmd := exec.CommandContext(ctx, helper, args...)
		cmd.Dir = t.TempDir()
		out, err := cmd.StdoutPipe()
		if err != nil {
			t.Fatal(err)
		}
		in, err := cmd.StdinPipe()
		if err != nil {
			t.Fatal(err)
		}
		if err := cmd.Start(); err != nil {
			t.Fatal(err)
		}
		t.Cleanup(func() { in.Close(); cmd.Wait() })
		return in, bufio.NewScanner(out)
	}

	// The host announces an invite, then readiness.
	_, host := start("desktop-host", "27960")
	if !host.Scan() || !strings.HasPrefix(host.Text(), "INVITE tc") {
		t.Fatal("host did not print an invite: " + host.Text())
	}
	if !host.Scan() || host.Text() != "READY" {
		t.Fatal("host did not become ready: " + host.Text())
	}

	// A garbled paste is refused.
	in, invalid := start("desktop-join")
	if _, err := io.WriteString(in, "not an invite\n"); err != nil {
		t.Fatal(err)
	}
	if !invalid.Scan() || invalid.Text() != "ERROR invalid invite" {
		t.Fatal("invalid invite was not rejected: " + invalid.Text())
	}

	// A valid invite from a real server reachable through a local DERP region.
	srv := &tailcat.Server{Region: localRegion(t), OnUDP: func(port uint16) func(tailcat.ConnPacketConn) {
		if port != racePort {
			return nil
		}
		return func(c tailcat.ConnPacketConn) {
			defer c.Close()
			buf := make([]byte, 1232)
			for {
				n, err := c.Read(buf)
				if err != nil {
					return
				}
				if _, err = c.Write(buf[:n]); err != nil {
					return
				}
			}
		}
	}}
	if err := srv.StartContext(ctx); err != nil {
		t.Fatal(err)
	}
	defer srv.Close()
	in, guest := start("desktop-join")
	if _, err := io.WriteString(in, string(srv.TailcatAddr())+"\n"); err != nil {
		t.Fatal(err)
	}
	if !guest.Scan() || !strings.HasPrefix(guest.Text(), "PORT ") {
		t.Fatal("guest startup failed: " + guest.Text())
	}
	port, err := strconv.Atoi(strings.TrimPrefix(guest.Text(), "PORT "))
	if err != nil {
		t.Fatal(err)
	}
	udp, err := net.DialUDP("udp4", nil, &net.UDPAddr{IP: net.IPv4(127, 0, 0, 1), Port: port})
	if err != nil {
		t.Fatal(err)
	}
	defer udp.Close()
	if err := udp.SetDeadline(time.Now().Add(8 * time.Second)); err != nil {
		t.Fatal(err)
	}
	if _, err := udp.Write([]byte("private race")); err != nil {
		t.Fatal(err)
	}
	buf := make([]byte, 64)
	n, err := udp.Read(buf)
	if err != nil || string(buf[:n]) != "private race" {
		t.Fatalf("guest round trip failed: %v", err)
	}
}
