// Command scrapburn-tailcat launches a race through a single-port UDP tunnel.
package main

import (
	"bufio"
	"context"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"os/exec"
	"os/signal"
	"strconv"
	"strings"
	"time"

	"github.com/tailscale/tailcat"
	"tailscale.com/types/logger"
)

const racePort = 27960

// Each remote flow gets a different source port at the authoritative game host.
func hostFlow(local *net.UDPAddr) func(uint16) func(tailcat.ConnPacketConn) {
	return func(port uint16) func(tailcat.ConnPacketConn) {
		if port != racePort {
			return nil
		}
		return func(remote tailcat.ConnPacketConn) {
			socket, err := net.DialUDP("udp4", nil, local)
			if err != nil {
				remote.Close()
				return
			}
			tailcat.ProxyPacketConns(socket, remote)
		}
	}
}

func host(ctx context.Context, game string) error {
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	cmd := exec.CommandContext(ctx, game, "--host-loopback")
	cmd.Stderr = os.Stderr
	output, err := cmd.StdoutPipe()
	if err != nil {
		return err
	}
	if err := cmd.Start(); err != nil {
		return err
	}
	waited := false
	defer func() {
		if !waited {
			cancel()
			cmd.Wait() // Reap the child after a startup failure.
		}
	}()
	lines := bufio.NewScanner(output)
	for lines.Scan() {
		if lines.Text() == "SCRAPBURN_HOST_CLOSED" {
			return errors.New("game could not start a local lobby")
		}
		value, ok := strings.CutPrefix(lines.Text(), "SCRAPBURN_HOST_PORT=")
		if !ok {
			continue
		}
		port, err := strconv.ParseUint(value, 10, 16)
		if err != nil || port == 0 {
			return errors.New("invalid local game port")
		}
		srv := &tailcat.Server{Logf: logger.Discard, OnUDP: hostFlow(&net.UDPAddr{IP: net.IPv4(127, 0, 0, 1), Port: int(port)})}
		startup, stop := context.WithTimeout(ctx, 45*time.Second)
		err = srv.StartContext(startup)
		stop()
		if err != nil {
			return fmt.Errorf("starting Tailcat host: %w", err)
		}
		fmt.Println("Share this private race invite with your friends:")
		fmt.Println(srv.TailcatAddr())
		// Leaving the hosting session closes the tunnel, even if the window stays open.
		for lines.Scan() {
			if lines.Text() == "SCRAPBURN_HOST_CLOSED" {
				break
			}
		}
		srv.Close()
		// The player can keep using the main menu after leaving the private lobby.
		for lines.Scan() {
		}
		if err := lines.Err(); err != nil {
			return err
		}
		waited = true
		return cmd.Wait()
	}
	return errors.New("game exited before opening its lobby")
}

// Pin the relay to the first local client and preserve one write per datagram.
func relay(local *net.UDPConn, tunnel net.Conn) error {
	buf := make([]byte, 65535)
	n, peer, err := local.ReadFromUDPAddrPort(buf)
	if err != nil {
		return err
	}
	if _, err = tunnel.Write(buf[:n]); err != nil {
		return err
	}
	replies := make(chan error, 1)
	go func() {
		packet := make([]byte, 65535)
		for {
			n, err := tunnel.Read(packet)
			if err == nil {
				_, err = local.WriteToUDPAddrPort(packet[:n], peer)
			}
			if err != nil {
				replies <- err
				local.Close()
				return
			}
		}
	}()
	defer func() { tunnel.Close(); <-replies }()
	for {
		n, src, err := local.ReadFromUDPAddrPort(buf)
		if err != nil {
			return err
		}
		if src != peer {
			continue
		}
		if _, err := tunnel.Write(buf[:n]); err != nil {
			return err
		}
	}
}

func join(ctx context.Context, game string, input io.Reader) error {
	fmt.Fprintln(os.Stderr, "Paste the host's private Tailcat invite and press Enter.")
	scanner := bufio.NewScanner(io.LimitReader(input, 8193))
	if !scanner.Scan() || len(scanner.Bytes()) > 8192 {
		return errors.New("missing or oversized invite")
	}
	addr := tailcat.Addr(strings.TrimSpace(scanner.Text()))
	if _, err := tailcat.ParseAddr(addr); err != nil {
		return errors.New("invalid Tailcat invite")
	}
	fmt.Fprintln(os.Stderr, "Connecting to the private lobby…")
	client := tailcat.NewClient(addr)
	client.Logf = logger.Discard
	defer client.Close()
	startup, cancel := context.WithTimeout(ctx, 45*time.Second)
	remote, err := client.DialUDPPort(startup, racePort)
	cancel()
	if err != nil {
		return errors.New("could not open the Tailcat tunnel; check the invite and host")
	}
	defer remote.Close()
	local, err := net.ListenUDP("udp4", &net.UDPAddr{IP: net.IPv4(127, 0, 0, 1)})
	if err != nil {
		return err
	}
	defer local.Close()
	run, stop := context.WithCancel(ctx)
	defer stop()
	cmd := exec.CommandContext(run, game, "--join", local.LocalAddr().String())
	cmd.Stdout, cmd.Stderr = os.Stdout, os.Stderr
	if err := cmd.Start(); err != nil {
		return err
	}
	done := make(chan error, 1)
	go func() { done <- relay(local, remote); stop() }()
	gameErr := cmd.Wait()
	local.Close()
	remote.Close()
	relayErr := <-done
	if gameErr != nil && relayErr != nil && !errors.Is(relayErr, net.ErrClosed) {
		return fmt.Errorf("race tunnel disconnected: %w", relayErr)
	}
	return gameErr
}

func main() {
	if (len(os.Args) == 3 && os.Args[1] == "desktop-host") || (len(os.Args) == 2 && os.Args[1] == "desktop-join") {
		port := ""
		if len(os.Args) == 3 {
			port = os.Args[2]
		}
		if err := desktop(context.Background(), port); err != nil {
			fmt.Println("ERROR " + err.Error())
			os.Exit(1)
		}
		return
	}
	if len(os.Args) != 3 || (os.Args[1] != "host" && os.Args[1] != "join") {
		fmt.Fprintln(os.Stderr, "usage: scrapburn-tailcat host|join PATH-TO-SCRAPBURN")
		os.Exit(2)
	}
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt)
	defer stop()
	var err error
	if os.Args[1] == "host" {
		err = host(ctx, os.Args[2])
	} else {
		err = join(ctx, os.Args[2], os.Stdin)
	}
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
