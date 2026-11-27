package main

import (
	"bytes"
	"context"
	"net"
	"testing"
	"time"

	"github.com/tailscale/tailcat"
	"net/http/httptest"
	"tailscale.com/derp/derpserver"
	"tailscale.com/net/stun/stuntest"
	"tailscale.com/tailcfg"
	"tailscale.com/types/key"
	"tailscale.com/types/nettype"
)

func localRegion(t *testing.T) *tailcfg.DERPRegion {
	t.Helper()
	derp := derpserver.New(key.NewNode(), t.Logf)
	t.Cleanup(func() { derp.Close() })
	https := httptest.NewTLSServer(derpserver.Handler(derp))
	t.Cleanup(https.Close)
	stun, stop := stuntest.ServeWithPacketListener(t, nettype.Std{})
	t.Cleanup(stop)
	return &tailcfg.DERPRegion{RegionID: 1, RegionCode: "local", Nodes: []*tailcfg.DERPNode{{
		Name: "local1", RegionID: 1, HostName: "127.0.0.1", IPv4: "127.0.0.1", IPv6: "none",
		STUNPort: stun.Port, DERPPort: https.Listener.Addr().(*net.TCPAddr).Port, InsecureForTests: true,
	}}}
}

// Exercise the same datagram path as a race, with a real local DERP and STUN server.
func TestRaceTunnel(t *testing.T) {
	ctx, cancel := context.WithTimeout(t.Context(), 25*time.Second)
	defer cancel()
	listen := func() *net.UDPConn {
		s, err := net.ListenUDP("udp4", &net.UDPAddr{IP: net.IPv4(127, 0, 0, 1)})
		if err != nil {
			t.Fatal(err)
		}
		t.Cleanup(func() { s.Close() })
		return s
	}
	game := listen()
	region := localRegion(t)
	flow := hostFlow(game.LocalAddr().(*net.UDPAddr))
	if flow(racePort+1) != nil {
		t.Fatal("unexpected port exposed")
	}
	server := &tailcat.Server{Region: region, OnUDP: flow}
	if err := server.StartContext(ctx); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { server.Close() })
	seen := map[string]bool{}
	for _, size := range []int{32, 1216} {
		client := tailcat.NewClient(server.TailcatAddr())
		t.Cleanup(func() { client.Close() })
		if _, err := client.Ping(ctx); err != nil {
			t.Fatal(err)
		}
		remote, err := client.DialUDPPort(ctx, racePort)
		if err != nil {
			t.Fatal(err)
		}
		proxy := listen()
		done := make(chan error, 1)
		go func() { done <- relay(proxy, remote) }()
		t.Cleanup(func() { proxy.Close(); remote.Close(); <-done })
		guest := listen()
		data := bytes.Repeat([]byte{byte(size)}, size)
		deadline := time.Now().Add(5 * time.Second)
		if err := game.SetDeadline(deadline); err != nil {
			t.Fatal(err)
		}
		if err := guest.SetDeadline(deadline); err != nil {
			t.Fatal(err)
		}
		if _, err := guest.WriteTo(data, proxy.LocalAddr()); err != nil {
			t.Fatal(err)
		}
		buf := make([]byte, 2048)
		n, peer, err := game.ReadFrom(buf)
		if err != nil {
			t.Fatal(err)
		}
		if !bytes.Equal(buf[:n], data) || seen[peer.String()] {
			t.Fatal("corrupt datagram or shared peer address")
		}
		seen[peer.String()] = true
		if _, err := game.WriteTo(buf[:n], peer); err != nil {
			t.Fatal(err)
		}
		n, _, err = guest.ReadFrom(buf)
		if err != nil || !bytes.Equal(buf[:n], data) {
			t.Fatalf("reply size %d, error %v", n, err)
		}
	}
}
