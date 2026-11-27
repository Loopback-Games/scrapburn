package main

import (
	"bufio"
	"context"
	"net"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/godbus/dbus/v5"
	"github.com/tailscale/tailcat"
)

// The test owns an isolated session bus so no player dialogs or settings are affected.
type testPicker struct {
	conn *dbus.Conn
	uri  string
}

func (p testPicker) respond(code uint32) (dbus.ObjectPath, *dbus.Error) {
	const handle = dbus.ObjectPath("/org/freedesktop/portal/desktop/request/test/invite")
	err := p.conn.Emit(handle, "org.freedesktop.portal.Request.Response", code, map[string]dbus.Variant{"uris": dbus.MakeVariant([]string{p.uri})})
	if err != nil {
		return "", dbus.MakeFailedError(err)
	}
	return handle, nil
}

func (p testPicker) OpenFile(_, _ string, _ map[string]dbus.Variant) (dbus.ObjectPath, *dbus.Error) {
	return p.respond(0)
}
func (p testPicker) SaveFile(_, _ string, _ map[string]dbus.Variant) (dbus.ObjectPath, *dbus.Error) {
	return p.respond(1)
}

func TestPackagedInvite(t *testing.T) {
	helper := os.Getenv("SCRAPBURN_TEST_HELPER")
	if helper == "" {
		t.Skip("run under dbus-run-session with SCRAPBURN_TEST_HELPER set")
	}
	helper, err := filepath.Abs(helper)
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(t.Context(), 25*time.Second)
	defer cancel()
	conn, err := dbus.ConnectSessionBus()
	if err != nil {
		t.Fatal(err)
	}
	defer conn.Close()
	path := filepath.Join(t.TempDir(), "Race invite.scrapburn")
	picker := testPicker{conn, (&url.URL{Scheme: "file", Path: path}).String()}
	if err := conn.Export(picker, "/org/freedesktop/portal/desktop", "org.freedesktop.portal.FileChooser"); err != nil {
		t.Fatal(err)
	}
	ownership, err := conn.RequestName("org.freedesktop.portal.Desktop", dbus.NameFlagDoNotQueue)
	if err != nil || ownership != dbus.RequestNameReplyPrimaryOwner {
		t.Fatal("test needs an isolated session bus")
	}
	start := func(args ...string) (*exec.Cmd, *bufio.Scanner) {
		cmd := exec.CommandContext(ctx, helper, args...)
		cmd.Dir = t.TempDir()
		cmd.Env = append(os.Environ(), "PATH="+cmd.Dir)
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
		return cmd, bufio.NewScanner(out)
	}
	_, host := start("desktop-host", "27960")
	if !host.Scan() || host.Text() != "CANCEL" {
		t.Fatal("Save dialog cancellation failed: " + host.Text())
	}
	if err := os.WriteFile(path, []byte("bad invite"), 0600); err != nil {
		t.Fatal(err)
	}
	_, invalid := start("desktop-join")
	if !invalid.Scan() || invalid.Text() != "ERROR invalid invite file" {
		t.Fatal("invalid file was not rejected: " + invalid.Text())
	}
	// Valid invite from a real server reachable through a local DERP region.
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
	if err := saveInvite(path, srv.TailcatAddr()); err != nil {
		t.Fatal(err)
	}
	_, guest := start("desktop-join")
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
