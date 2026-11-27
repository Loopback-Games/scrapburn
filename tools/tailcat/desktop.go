// Desktop mode is owned by the game process. The host prints its private invite
// on stdout for the game to copy; a guest receives the invite as the first line on
// stdin. Closing stdin ends the session.
package main

import (
	"bufio"
	"context"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/tailscale/tailcat"
	"tailscale.com/types/logger"
)

// maxInvite bounds the pasted invite line; real addresses are a few hundred bytes.
const maxInvite = 8192

// compactInvite drops the relay nodes' IP addresses from an invite, keeping their
// hostnames, which guests resolve with DNS. Keys and the pre-shared secret are kept,
// so the invite is shorter to paste without being any weaker.
func compactInvite(addr tailcat.Addr) tailcat.Addr {
	ci, err := tailcat.ParseAddr(addr)
	if err != nil {
		return addr
	}
	for _, r := range ci.Region {
		for _, n := range r.Nodes {
			if n.HostName == "" {
				return addr // Without a hostname the IPs are the only way to reach it.
			}
			n.IPv4, n.IPv6 = "", ""
		}
	}
	return ci.Addr()
}

// readInvite takes the first stdin line as a Tailcat address.
func readInvite(in *bufio.Reader) (tailcat.Addr, error) {
	line, err := in.ReadString('\n')
	if err != nil && !(errors.Is(err, io.EOF) && line != "") {
		return "", errors.New("missing invite")
	}
	if len(line) > maxInvite {
		return "", errors.New("invalid invite")
	}
	addr := tailcat.Addr(strings.TrimSpace(line))
	if _, err := tailcat.ParseAddr(addr); err != nil {
		return "", errors.New("invalid invite")
	}
	return addr, nil
}

func desktop(ctx context.Context, hostPort string) error {
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	in := bufio.NewReaderSize(os.Stdin, maxInvite+1)
	hosting := hostPort != ""
	var addr tailcat.Addr
	if !hosting {
		var err error
		if addr, err = readInvite(in); err != nil {
			return err
		}
	}
	go func() { io.Copy(io.Discard, in); cancel() }()
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
		fmt.Printf("INVITE %s\n", compactInvite(srv.TailcatAddr()))
		fmt.Println("READY")
		<-ctx.Done()
		return nil
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
