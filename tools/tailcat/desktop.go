// Desktop mode is owned by the game process. Only readiness and loopback ports
// cross stdout. The private invite travels in a file selected by the player.
package main

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"github.com/tailscale/tailcat"
	"tailscale.com/types/logger"
)

// Replace the selected file only after the complete private invite has been written.
func saveInvite(path string, addr tailcat.Addr) error {
	file, err := os.CreateTemp(filepath.Dir(path), ".scrapburn-invite-*")
	if err != nil {
		return err
	}
	defer os.Remove(file.Name())
	_, writeErr := fmt.Fprintln(file, addr)
	closeErr := file.Close()
	if writeErr != nil {
		return writeErr
	}
	if closeErr != nil {
		return closeErr
	}
	return os.Rename(file.Name(), path)
}

func desktop(ctx context.Context, hostPort string) error {
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	go func() { io.Copy(io.Discard, os.Stdin); cancel() }()
	hosting := hostPort != ""
	path, err := invitePath(ctx, hosting)
	if err != nil {
		return errors.New("could not open the desktop file picker")
	}
	if path == "" {
		fmt.Println("CANCEL")
		return nil
	}
	startup, stop := context.WithTimeout(ctx, 45*time.Second)
	defer stop()
	if hosting {
		port, err := strconv.ParseUint(hostPort, 10, 16)
		if err != nil || port == 0 {
			return errors.New("invalid lobby port")
		}
		srv := &tailcat.Server{Logf: logger.Discard, OnUDP: hostFlow(&net.UDPAddr{IP: net.IPv4(127, 0, 0, 1), Port: int(port)})}
		if err := srv.StartContext(startup); err != nil {
			return errors.New("could not reach Tailcat")
		}
		defer srv.Close()
		if err := saveInvite(path, srv.TailcatAddr()); err != nil {
			return errors.New("could not save invite file")
		}
		fmt.Println("READY")
		<-ctx.Done()
		return nil
	}
	f, err := os.Open(path)
	if err != nil {
		return errors.New("could not open invite file")
	}
	defer f.Close()
	info, err := f.Stat()
	if err != nil || !info.Mode().IsRegular() || info.Size() > 8192 {
		return errors.New("invalid invite file")
	}
	data, err := io.ReadAll(io.LimitReader(f, 8193))
	if err != nil || len(data) > 8192 {
		return errors.New("invalid invite file")
	}
	addr := tailcat.Addr(strings.TrimSpace(string(data)))
	if _, err := tailcat.ParseAddr(addr); err != nil {
		return errors.New("invalid invite file")
	}
	client := tailcat.NewClient(addr)
	client.Logf = logger.Discard
	defer client.Close()
	tunnel, err := client.DialUDPPort(startup, racePort)
	if err != nil {
		return errors.New("invite expired or host unreachable")
	}
	defer tunnel.Close()
	local, err := net.ListenUDP("udp4", &net.UDPAddr{IP: net.IPv4(127, 0, 0, 1)})
	if err != nil {
		return errors.New("could not open local game connection")
	}
	defer local.Close()
	closeOnExit := context.AfterFunc(ctx, func() { local.Close(); tunnel.Close() })
	defer closeOnExit()
	fmt.Printf("PORT %d\n", local.LocalAddr().(*net.UDPAddr).Port)
	return relay(local, tunnel)
}
